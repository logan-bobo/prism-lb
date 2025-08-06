use crate::backend::{Backend, BackendConfig};
use crate::config::{Config, HealthCheck};

use std::{
    str::FromStr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use anyhow::{anyhow, Context, Error};
use http_body_util::combinators::BoxBody;
use http_body_util::BodyExt;
use hyper::{
    body::{Bytes, Incoming},
    server::conn::http1,
    service::service_fn,
    Request, Response, Uri,
};
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

#[derive(Debug)]
pub struct Server {
    listener: TcpListener,
    client: Arc<Client<HttpConnector, BoxBody<Bytes, hyper::Error>>>,
    backends: Arc<RwLock<Vec<Arc<Backend>>>>,
    count: AtomicUsize,
    health_check: HealthCheck,
}

impl Server {
    pub async fn new(config: Config) -> Result<Self, Error> {
        let listener = TcpListener::bind(format!(
            "{}:{}",
            config.bind_interface(),
            config.bind_port()
        ))
        .await?;

        let backends = config
            .backends()
            .iter()
            .enumerate()
            .map(|(index, backend)| {
                let host = backend
                    .get("host")
                    .ok_or_else(|| anyhow!("backend {} missing 'host'", index))?
                    .to_string();

                let port_str = backend
                    .get("port")
                    .ok_or_else(|| anyhow!("backend '{}' missing 'port'", host))?;

                let port = u16::from_str(port_str).with_context(|| {
                    format!("backend '{}' has invalid port '{}'", host, port_str)
                })?;

                let health_path = backend
                    .get("healthPath")
                    .ok_or_else(|| anyhow!("Backend '{}' missing healthPath", host))?
                    .to_string();

                let backend_config = BackendConfig::new(host, port, health_path);
                Ok(Arc::new(Backend::new(backend_config)))
            })
            .collect::<Result<Vec<Arc<Backend>>, Error>>()?;

        Ok(Self {
            listener,
            client: spawn_client(),
            backends: Arc::new(RwLock::new(backends)),
            count: AtomicUsize::new(0),
            health_check: config.health_check,
        })
    }

    pub async fn serve(self: Arc<Self>) -> Result<(), Error> {
        let server_clone = Arc::clone(&self);
        tokio::spawn(async move {
            info!(
                "starting health worker on interval: {}",
                server_clone.health_check.interval()
            );

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

                if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
                    error!("error serving connection: {:?}", err);
                }
            });
        }
    }

    async fn handle_request(
        &self,
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

        Ok(self.client.request(req.map(|body| body.boxed())).await?)
    }

    async fn route_request(&self, req: Request<Incoming>) -> Result<Response<Incoming>, Error> {
        let backend = self.next_server_address().await;

        info!("forwaring request to backend {}", backend.to_string());
        self.handle_request(req, backend).await
    }

    async fn next_server_address(&self) -> Arc<Backend> {
        let backend_read = self.backends.read().await;
        let current_count = self.count.load(Ordering::SeqCst);

        if current_count >= backend_read.len() {
            self.reset_count();
            let backend = backend_read.get(0).unwrap();
            self.count.fetch_add(1, Ordering::SeqCst);
            backend.clone()
        } else {
            let backend = backend_read.get(current_count).unwrap();
            self.count.fetch_add(1, Ordering::SeqCst);
            backend.clone()
        }
    }

    fn reset_count(&self) {
        self.count.store(0, Ordering::SeqCst);
    }

    async fn health_worker(&self, interval_duration: Duration) {
        let mut interval = interval(interval_duration);

        loop {
            interval.tick().await;

            let healthy_backends = self.backend_health_check().await;

            let mut backend_write = self.backends.write().await;
            *backend_write = healthy_backends;
        }
    }

    async fn backend_health_check(&self) -> Vec<Arc<Backend>> {
        let mut new_healthy_backends = Vec::new();
        let backends_read = self.backends.read().await;

        for value in backends_read.iter() {
            let is_healthy = { value.check_health(self.client.clone()).await.unwrap() };

            if !is_healthy {
                value.increment_health_failure();
            }

            if value
                .is_health_failures_larger_than_threshold(*self.health_check.failure_threshold())
            {
                info!(
                    "backend health failure threshhold met, removing backend from pool: {}",
                    value.to_string()
                );
                continue;
            };

            new_healthy_backends.push(value.clone());
        }

        new_healthy_backends
    }
}

pub fn spawn_client() -> Arc<Client<HttpConnector, BoxBody<Bytes, hyper::Error>>> {
    let mut connector = HttpConnector::new();
    connector.set_keepalive(Some(Duration::from_secs(60)));
    connector.set_connect_timeout(Some(Duration::from_secs(30)));
    connector.set_happy_eyeballs_timeout(Some(Duration::from_millis(300)));

    Arc::new(
        Client::builder(TokioExecutor::new())
            .pool_idle_timeout(Duration::from_secs(60))
            .pool_max_idle_per_host(200)
            .build(connector),
    )
}
