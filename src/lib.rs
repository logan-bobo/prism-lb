pub mod backend;
pub mod parser;

use crate::backend::Backend;
use crate::parser::HealthCheck;

use std::{
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
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::RwLock,
    time::{interval, Duration},
};

#[derive(Debug)]
struct ForwardMessage {
    path: String,
    host: IpAddr,
    port: u16,
}

async fn produce_message_from_stream(
    stream: &mut TcpStream,
    downstream_server: Arc<Backend>,
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

    Ok(ForwardMessage {
        path,
        host: *downstream_server.ipaddr(),
        port: *downstream_server.port(),
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
    downstream_server: Arc<Backend>,
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
    backends: Arc<RwLock<Vec<Arc<Backend>>>>,
    count: AtomicUsize,
    health_check: HealthCheck,
}

impl Server {
    pub async fn new(
        listener: TcpListener,
        backends: Vec<Arc<Backend>>,
        health_check: HealthCheck,
    ) -> Self {
        Self {
            listener,
            backends: Arc::new(RwLock::new(backends)),
            count: AtomicUsize::new(0),
            health_check,
        }
    }

    pub async fn serve(self: Arc<Self>) -> Result<(), Box<dyn std::error::Error>> {
        let server_clone = Arc::clone(&self);
        tokio::spawn(async move {
            server_clone
                .health_worker(Duration::new(*server_clone.health_check.interval(), 0))
                .await;
        });

        loop {
            let (stream, _) = self.listener.accept().await?;

            let backend = self.next_server_address().await;

            tokio::spawn(async move {
                if let Err(err) = process_stream(stream, backend).await {
                    error!("Could not process stream with err: {}", err)
                }
            });
        }
    }

    async fn next_server_address(&self) -> Arc<Backend> {
        let backend_read = self.backends.read().await;

        if self.count.load(Ordering::SeqCst) == backend_read.len() {
            self.reset_count();
        };

        let backend = backend_read.get(self.count.load(Ordering::SeqCst)).unwrap();

        self.count.fetch_add(1, Ordering::SeqCst);

        backend.clone()
    }

    fn reset_count(&self) {
        self.count.store(0, Ordering::SeqCst);
    }

    async fn backend_health_check(&self) -> Vec<Arc<Backend>> {
        let mut new_healthy_backends = Vec::new();
        let backends_read = self.backends.read().await;

        for value in backends_read.iter() {
            let is_healthy = { value.is_healthy().await };

            if !is_healthy {
                value.increment_health_failure();
            }

            if value
                .is_health_failures_larger_than_threshold(*self.health_check.failure_threshold())
            {
                continue;
            };

            new_healthy_backends.push(value.clone());
        }

        new_healthy_backends
    }

    async fn health_worker(&self, interval_duration: Duration) {
        let mut interval = interval(interval_duration);

        loop {
            debug!("starting health check");

            interval.tick().await;

            let healthy_backends = self.backend_health_check().await;
            debug!("new backend pool {:?}", healthy_backends);

            let mut backend_write = self.backends.write().await;
            *backend_write = healthy_backends;

            debug!("finished health check");
        }
    }
}
