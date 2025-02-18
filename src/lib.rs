pub mod parser;

use crate::parser::HealthCheck;

use std::{
    fmt::Display,
    net::IpAddr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use http_body_util::{BodyExt, Empty, Full};
use hyper::{
    body::{Buf, Bytes},
    Request, Response, StatusCode, Uri,
};
use hyper_util::rt::TokioIo;
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::RwLock,
};

#[derive(Debug)]
struct ForwardMessage {
    path: String,
    host: IpAddr,
    port: u32,
}

async fn produce_message_from_stream(
    stream: &mut TcpStream,
    downstream_server: Arc<RwLock<Backend>>,
) -> Result<ForwardMessage, Box<dyn std::error::Error>> {
    let mut buf_reader = BufReader::new(stream).lines();
    let read_request_line = buf_reader.next_line().await?;

    let request_line = match read_request_line {
        Some(value) => value,
        None => {
            return Err("TCP stream contained no data".into());
        }
    };

    let request: Vec<&str> = request_line.split(" ").collect();

    let path = request[1].to_string();

    let backend_data = downstream_server.read().await;

    Ok(ForwardMessage {
        path,
        host: backend_data.ipaddr,
        port: backend_data.port,
    })
}

async fn build_uri_from_message(message: ForwardMessage) -> Result<Uri, hyper::http::Error> {
    let uri = Uri::builder()
        .scheme("http")
        .authority(format!("{}:{}", message.host, message.port))
        .path_and_query(message.path)
        .build();

    debug!("uri constructed: {:?}", uri);

    uri
}

async fn call_downstream_server(uri: Uri) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    info!("calling downstream bakend: {:?}", uri);
    let host = match uri.host() {
        Some(value) => value,
        None => return Err("empty host in URI".into()),
    };

    let port = match uri.port_u16() {
        Some(value) => value,
        None => return Err("empty port in URI".into()),
    };

    let authority = match uri.authority() {
        Some(value) => value.clone(),
        None => return Err("empty authority in URI".into()),
    };

    let address = format!("{}:{}", host, port);

    let stream = TcpStream::connect(address).await?;

    debug!("tcp connection established to downstream server");

    let io = TokioIo::new(stream);

    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;

    tokio::task::spawn(async move {
        conn.await?;
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    });

    let req = Request::builder()
        .uri(uri)
        .header(hyper::header::HOST, authority.as_str())
        .body(Empty::<Bytes>::new())?;

    let mut res = sender.send_request(req).await?;

    debug!("downstream request successful");

    let mut response_bytes: Vec<u8> = Vec::new();

    while let Some(next) = res.frame().await {
        let frame = next?;
        if let Some(chunk) = frame.data_ref() {
            chunk.chunk().iter().for_each(|item| {
                response_bytes.push(*item);
            });
        }
    }

    Ok(response_bytes)
}

async fn respond_to_client(
    bytes: Vec<u8>,
    mut stream: TcpStream,
) -> Result<(), Box<dyn std::error::Error>> {
    let response = Response::builder()
        .version(hyper::Version::HTTP_11)
        .status(StatusCode::OK)
        .header("Content-Type", "text/plain")
        .header("Content-Length", bytes.len())
        .body(Full::new(Bytes::from(bytes.clone())))?;

    let headers = format!(
        "HTTP/1.1 {}\r\n{:?}\r\n\r\n",
        response.status(),
        response.headers()
    );

    stream.write_all(headers.as_bytes()).await?;
    stream
        .write_all(&response.collect().await?.to_bytes())
        .await?;

    info!("data written back to client via tcp stream");

    stream.shutdown().await?;

    Ok(())
}

pub async fn process_stream(
    mut stream: TcpStream,
    downstream_server: Arc<RwLock<Backend>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let message = produce_message_from_stream(&mut stream, downstream_server).await?;

    let uri = build_uri_from_message(message).await?;

    let downstream_response_bytes = call_downstream_server(uri).await?;

    respond_to_client(downstream_response_bytes, stream).await?;

    Ok(())
}

#[derive(Debug)]
pub struct Server {
    listener: TcpListener,
    backends: Vec<Arc<RwLock<Backend>>>,
    count: AtomicUsize,
    health_check: HealthCheck,
}

impl Server {
    pub async fn new(
        listener: TcpListener,
        backends: Vec<Arc<RwLock<Backend>>>,
        health_check: HealthCheck,
    ) -> Self {
        Self {
            listener,
            backends,
            count: AtomicUsize::new(0),
            health_check,
        }
    }

    pub async fn serve(&self) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            let (stream, _) = self.listener.accept().await?;

            let backend = self.next_server_address();

            tokio::spawn(async move {
                if let Err(err) = process_stream(stream, backend).await {
                    error!("Could not process stream with err: {}", err)
                }
            });
        }
    }

    fn next_server_address(&self) -> Arc<RwLock<Backend>> {
        if self.count.load(Ordering::SeqCst) == self.backends.len() {
            self.reset_count();
        };

        let backend = self
            .backends
            .get(self.count.load(Ordering::SeqCst))
            .unwrap();

        self.count.fetch_add(1, Ordering::SeqCst);

        backend.clone()
    }

    fn reset_count(&self) {
        self.count.store(0, Ordering::SeqCst);
    }

    async fn backend_health_check(&self) -> Vec<Arc<RwLock<Backend>>> {
        let mut new_healthy_backends = Vec::new();

        for value in self.backends.iter() {
            let read_backend = value.read().await;

            if !read_backend.is_healthy().await {
                let mut write_backend = value.write().await;

                write_backend.health_failures += 1;
            };

            if read_backend.health_failures >= *self.health_check.failure_threshold() {
                continue;
            };

            new_healthy_backends.push(Arc::new(RwLock::new(Backend::new(
                read_backend.ipaddr,
                read_backend.port,
                read_backend.health_path.clone(),
            ))));
        }

        new_healthy_backends
    }
}

#[derive(PartialEq, Serialize, Deserialize, Debug)]
pub struct Backend {
    ipaddr: IpAddr,
    port: u32,
    health_path: String,
    #[serde(skip_deserializing)]
    health_failures: u32,
}

impl Backend {
    pub fn new(ipaddr: IpAddr, port: u32, health_path: String) -> Self {
        Self {
            ipaddr,
            port,
            health_path,
            health_failures: 0,
        }
    }

    async fn is_healthy(&self) -> bool {
        let address = format!("{}:{}", self.ipaddr, self.port);

        let uri = Uri::builder()
            .scheme("http")
            .authority(address)
            .path_and_query(self.health_path.clone())
            .build()
            .unwrap();

        let stream = match TcpStream::connect(uri.authority().unwrap().to_string()).await {
            Ok(stream) => stream,
            Err(_) => return false,
        };

        let io = TokioIo::new(stream);

        let (mut sender, conn) = match hyper::client::conn::http1::handshake(io).await {
            Ok((sender, conn)) => (sender, conn),
            Err(_) => return false,
        };

        tokio::task::spawn(async move {
            conn.await?;
            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        });

        let req = match Request::builder()
            .uri(uri.clone())
            .header(hyper::header::HOST, uri.authority().unwrap().as_str())
            .body(Empty::<Bytes>::new())
        {
            Ok(req) => req,
            Err(_) => return false,
        };

        let res = match sender.send_request(req).await {
            Ok(res) => res,
            Err(_) => return false,
        };

        res.status() == StatusCode::OK
    }
}

impl Display for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.ipaddr, self.port)
    }
}
