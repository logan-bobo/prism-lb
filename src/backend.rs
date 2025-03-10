use std::{fmt::Display, net::IpAddr, sync::atomic::AtomicUsize};

use crate::Ordering;

use derive_getters::Getters;
use http_body_util::Empty;
use hyper::{body::Bytes, Request, StatusCode, Uri};
use hyper_util::rt::TokioIo;
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;

#[derive(Serialize, Deserialize, Debug, Getters)]
pub struct Backend {
    ipaddr: IpAddr,
    port: u16,
    health_path: String,
    #[serde(skip_deserializing)]
    health_failures: AtomicUsize,
}

impl Backend {
    pub fn new(ipaddr: IpAddr, port: u16, health_path: String) -> Self {
        Self {
            ipaddr,
            port,
            health_path,
            health_failures: AtomicUsize::new(0),
        }
    }

    pub fn increment_health_failure(&self) {
        self.health_failures.fetch_add(1, Ordering::SeqCst);
    }

    pub fn is_health_failures_larger_than_threshold(&self, failure_threshold: usize) -> bool {
        self.health_failures.load(Ordering::SeqCst) >= failure_threshold
    }

    pub async fn is_healthy(&self) -> bool {
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
