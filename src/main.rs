use std::str::FromStr;
use std::sync::Arc;

use prism_lb::backend::{Backend, BackendConfig};
use prism_lb::parser::Config;
use prism_lb::Server;

use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;

use log::{error, info};

#[tokio::main]
async fn main() {
    env_logger::init();

    let mut file = File::open("backends.yaml")
        .await
        .expect("Could not open config file, please ensure `backends.yaml` exists");

    let mut contents = String::new();

    file.read_to_string(&mut contents)
        .await
        .expect("Could not process config");

    let config: Config =
        Config::try_from(contents).expect("Could not build internal Config from config file");

    let listener = TcpListener::bind(format!(
        "{}:{}",
        config.bind_interface(),
        config.bind_port()
    ))
    .await
    .expect("could not bind to interface/port");

    let mut backends: Vec<Arc<Backend>> = Vec::new();

    config.backends().iter().for_each(|backend| {
        let backend_config = BackendConfig::new(
            backend
                .get("host")
                .expect("host must be provided in a backend")
                .to_string(),
            u16::from_str(backend.get("port").expect("invalid port")).expect("invalid port"),
            backend
                .get("healthPath")
                .expect("health path is required for all layer 7 backends")
                .to_string(),
        );

        backends.push(Arc::new(Backend::new(backend_config)));
    });

    info!(
        "starting lb... \nbackends: {:?}\ninterface: {:?}\nport: {:?}",
        backends,
        config.bind_interface(),
        config.bind_port()
    );

    let server = Arc::new(Server::new(listener, backends, config.health_check).await);

    if let Err(error) = server.serve().await {
        error!("{error}");
    };
}
