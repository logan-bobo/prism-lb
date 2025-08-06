use std::{
    fmt::Display,
    sync::{atomic::AtomicUsize, Arc},
};

use std::sync::atomic::Ordering;

use anyhow::Error;
use derive_getters::Getters;
use derive_new::new;
use http::StatusCode;
use http_body_util::combinators::BoxBody;
use http_body_util::BodyExt;
use http_body_util::Empty;
use hyper::{body::Bytes, Method, Request, Uri};
use hyper_util::client::legacy::{connect::HttpConnector, Client};
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

#[cfg(test)]
mod tests {
    use super::{Backend, BackendConfig};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_backend_constructor_with_domain() {
        let backend_config = BackendConfig::new("example.com".to_string(), 80, "/".to_string());
        let backend = Backend::new(backend_config);

        assert_eq!(backend.config.address(), "example.com");
        assert_eq!(backend.config.port(), &80);
        assert_eq!(backend.config.health_path(), "/");
    }

    #[test]
    fn test_backend_display() {
        let backend_config = BackendConfig::new("example.com".to_string(), 80, "/".to_string());
        let backend = Backend::new(backend_config);

        // this test is important baceause to string is used to build the authority in URIs
        assert_eq!(backend.to_string(), "example.com:80");
    }

    #[test]
    fn test_backend_increment_health_failure() {
        let backend_config = BackendConfig::new("example.com".to_string(), 80, "/".to_string());
        let backend = Backend::new(backend_config);

        backend.increment_health_failure();

        let atomic = AtomicUsize::new(1);

        let actual = backend.health_failures.load(Ordering::SeqCst);
        let exepected = atomic.load(Ordering::SeqCst);

        assert_eq!(exepected, actual);
    }

    #[test]
    fn test_backend_failure_threshold_true() {
        let backend_config = BackendConfig::new("example.com".to_string(), 80, "/".to_string());
        let backend = Backend::new(backend_config);

        backend.increment_health_failure();
        backend.increment_health_failure();

        let expected = backend.is_health_failures_larger_than_threshold(1);

        assert!(expected)
    }

    #[test]
    fn test_backend_failure_threshold_false() {
        let backend_config = BackendConfig::new("example.com".to_string(), 80, "/".to_string());
        let backend = Backend::new(backend_config);

        backend.increment_health_failure();
        backend.increment_health_failure();

        let expected = backend.is_health_failures_larger_than_threshold(5);

        assert!(!expected)
    }
}
