use std::{fmt::Display, net::IpAddr, sync::Arc};

use hyper::{
    body::{Buf, Bytes},
    Request, Response, StatusCode, Uri,
};
use hyper_util::rt::TokioIo;

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::RwLock,
};

use http_body_util::{BodyExt, Empty, Full};

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
    Uri::builder()
        .scheme("http")
        .authority(format!("{}:{}", message.host, message.port))
        .path_and_query(message.path)
        .build()
}

async fn call_downstream_server(uri: Uri) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
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
    count: usize,
}

impl Server {
    pub async fn new(listener: TcpListener, backends: Vec<Arc<RwLock<Backend>>>) -> Self {
        Self {
            listener,
            backends,
            count: 0,
        }
    }

    pub async fn serve(&mut self) {
        loop {
            // this should fail if we cant allow incoming connections on a listener
            let (stream, _) = self.listener.accept().await.unwrap();

            let backend = self.next_server_address();

            tokio::spawn(async move {
                if let Err(err) = process_stream(stream, backend).await {
                    // How do I safley log why the stream could not be processed
                    // log it then safley close the thread?
                    panic!("Could not process stream with err: {}", err)
                }
            });
        }
    }

    fn next_server_address(&mut self) -> Arc<RwLock<Backend>> {
        if self.count == self.backends.len() {
            self.reset_count();
        };

        let backend = self.backends.get(self.count).unwrap();
        self.count += 1;

        backend.clone()
    }

    fn reset_count(&mut self) {
        self.count = 0;
    }
}

#[derive(Debug)]
pub struct Backend {
    ipaddr: IpAddr,
    port: u32,
}

impl Backend {
    pub fn new(ipaddr: IpAddr, port: u32) -> Self {
        Self { ipaddr, port }
    }
}

impl Display for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.ipaddr, self.port)
    }
}
