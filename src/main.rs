use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;

use prism_lb::parser::Config;
use prism_lb::{Backend, Server};

use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tokio::sync::RwLock;

use log::{error, info};

#[tokio::main]
async fn main() {
    env_logger::init();

    let mut file = File::open("backends.yaml")
        .await
        .expect("Could not open config file, please ensure `backends.yaml exists");

    let mut contents = String::new();

    file.read_to_string(&mut contents)
        .await
        .expect("Could not process config");

    let config: Config =
        Config::try_from(contents).expect("Could not build intenral Config from config file");

    let listener = TcpListener::bind(format!(
        "{}:{}",
        config.bind_interface(),
        config.bind_port()
    ))
    .await
    .expect("could not bind to interface/port");

    let mut backends: Vec<Arc<RwLock<Backend>>> = Vec::new();

    config.backends().iter().for_each(|backend| {
        backends.push(Arc::new(RwLock::new(Backend::new(
            IpAddr::from_str(backend.get("host").expect("invalid host")).expect("invalid host"),
            u32::from_str(backend.get("port").expect("invalid port")).expect("invalid port"),
        ))))
    });

    info!(
        "starting lb... \nbackends: {:?} \ninterface: {:?}\nport: {:?}",
        backends,
        config.bind_interface(),
        config.bind_port()
    );

    let mut server = Server::new(listener, backends).await;

    if let Err(error) = server.serve().await {
        error!("{error}");
    };
}
