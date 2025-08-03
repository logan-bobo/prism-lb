use std::{
    fmt::Display,
    sync::{atomic::AtomicUsize, Arc},
};

use crate::Ordering;

use anyhow::Error;
use derive_getters::Getters;
use derive_new::new;
use http::StatusCode;
use http_body_util::combinators::BoxBody;
use http_body_util::BodyExt;
use http_body_util::Empty;
use hyper::{
    body::{Bytes, Incoming},
    Method, Request, Uri,
};
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::TokioExecutor,
};
use serde::Deserialize;

#[derive(Debug, Getters)]
pub struct Backend {
    config: BackendConfig,
    health_failures: AtomicUsize,
}

#[derive(Deserialize, Debug, Getters, new)]
pub struct BackendConfig {
    address: String,
    port: u16,
    health_path: String,
}

impl Backend {
    pub fn new(config: BackendConfig) -> Self {
        Self {
            config: config,
            health_failures: AtomicUsize::new(0),
        }
    }

    pub fn increment_health_failure(&self) {
        self.health_failures.fetch_add(1, Ordering::SeqCst);
    }

    pub fn is_health_failures_larger_than_threshold(&self, failure_threshold: usize) -> bool {
        self.health_failures.load(Ordering::SeqCst) >= failure_threshold
    }

    pub async fn check_health(
        &self,
        client: Arc<Client<HttpConnector, BoxBody<Bytes, hyper::Error>>>,
    ) -> Result<bool, Error> {
        let uri = Uri::builder()
            .scheme("http")
            .authority(self.to_string())
            .path_and_query(self.config.health_path.clone())
            .build()?;

        let req = Request::builder().method(Method::GET).uri(uri).body(
            Empty::<Bytes>::new()
                .map_err(|never| match never {})
                .boxed(),
        )?;

        match client.request(req).await {
            Ok(resp) => {
                if resp.status() != StatusCode::OK {
                    return Ok(false);
                } else {
                    return Ok(true);
                }
            }
            Err(_) => return Ok(false),
        }
    }
}

impl Display for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.config.address, self.config.port)
    }
}

pub fn spawn_client() -> Arc<Client<HttpConnector, Incoming>> {
    Arc::new(Client::builder(TokioExecutor::new()).build(HttpConnector::new()))
}
