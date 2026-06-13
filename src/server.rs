use crate::{
    backend::{Backend, BackendConfig},
    config::{Config, HealthCheck},
};
use anyhow::{anyhow, Context, Error};
use derive_getters::Getters;
use http_body_util::{combinators::BoxBody, BodyExt};
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
use std::{
    str::FromStr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tokio::{
    net::TcpListener,
    time::{interval, Duration},
};

#[derive(Debug, Getters)]
pub struct Server {
    listener: TcpListener,
    client: Arc<Client<HttpConnector, BoxBody<Bytes, hyper::Error>>>,
    backends: Arc<Vec<Backend>>,
    routing_counter: AtomicUsize,
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
                Ok(Backend::new(backend_config))
            })
            .collect::<Result<Vec<Backend>, Error>>()?;

        Ok(Self {
            listener,
            client: spawn_client(),
            backends: Arc::new(backends),
            routing_counter: AtomicUsize::new(0),
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
        downstream_server: &Backend,
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
        let backend = self
            .next_routable_backend()
            .await
            .context("no routable backend found in bool")?;

        info!("forwaring request to backend {}", backend.to_string());
        self.handle_request(req, backend).await
    }

    async fn next_routable_backend(&self) -> Option<&Backend> {
        for _ in 0..self.backends.len() {
            let current_count = self.routing_counter.load(Ordering::SeqCst);
            let index = current_count % self.backends.len();

            // TODO: What if the backend does not exist???
            let backend = self.backends.get(index).unwrap();

            self.routing_counter.fetch_add(1, Ordering::SeqCst);

            if backend.is_healthy() {
                return Some(backend);
            } else {
                continue;
            }
        }

        None
    }

    async fn health_worker(&self, interval_duration: Duration) {
        let mut interval = interval(interval_duration);

        loop {
            interval.tick().await;
            self.backend_health_check().await;
        }
    }

    async fn backend_health_check(&self) {
        for backend in self.backends.iter() {
            let is_healthy = { backend.check_health(self.client.clone()).await.unwrap() };

            if !is_healthy {
                backend.zero_health();
                backend.increment_health_failure();
            } else {
                backend.incremenet_health_success();
            }

            if backend.health_failures_larger_than_threshold(*self.health_check.failure_threshold())
            {
                info!(
                    "backend health failure threshhold met, removing backend from pool: {}",
                    backend.to_string()
                );
                backend.mark_none_routable();
                continue;
            };

            if backend.health_success_larger_than_threshold(*self.health_check.healthy_threshold())
            {
                info!(
                    "backend health threshhold met, adding backend to pool: {}",
                    backend.to_string()
                );
                backend.mark_routable();
            }
        }
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
