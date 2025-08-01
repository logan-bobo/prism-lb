pub mod backend;
pub mod parser;

use crate::backend::Backend;
use crate::parser::HealthCheck;

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use anyhow::Error;
use hyper::{body::Incoming, service::service_fn, Request, Response, Uri};
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::{TokioExecutor, TokioIo},
};
use log::{error, info};
use tokio::{
    net::TcpListener,
    sync::RwLock,
    time::{interval, Duration},
};

async fn handle_request(
    mut req: Request<Incoming>,
    downstream_server: Arc<Backend>,
) -> Result<Response<Incoming>, Error> {
    let uri = Uri::builder()
        .scheme("http")
        .authority(downstream_server.to_string())
        .path_and_query(
            req.uri()
                .path_and_query()
                .map(|pq| pq.as_str())
                .unwrap_or("/"),
        )
        .build()?;

    *req.uri_mut() = uri;

    let client = Client::builder(TokioExecutor::new()).build(HttpConnector::new());

    Ok(client.request(req).await?)
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

    pub async fn serve(self: Arc<Self>) -> Result<(), Error> {
        let server_clone = Arc::clone(&self);
        tokio::spawn(async move {
            server_clone
                .health_worker(Duration::new(*server_clone.health_check.interval(), 0))
                .await;
        });

        loop {
            let (stream, _) = self.listener.accept().await?;
            let service_server = Arc::clone(&self);

            tokio::spawn(async move {
                let io = TokioIo::new(stream);

                let service = service_fn(move |req| {
                    let server = service_server.clone();
                    async move { server.route_request(req).await }
                });

                if let Err(err) = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, service)
                    .await
                {
                    error!("error serving connection: {:?}", err);
                }
            });
        }
    }

    async fn route_request(&self, req: Request<Incoming>) -> Result<Response<Incoming>, Error> {
        let backend = self.next_server_address().await;
        info!("forwaring request to backend {}", backend.to_string());
        handle_request(req, backend).await
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
            interval.tick().await;

            let healthy_backends = self.backend_health_check().await;
            info!("new backend pool {:?}", healthy_backends);

            let mut backend_write = self.backends.write().await;
            *backend_write = healthy_backends;
        }
    }
}
