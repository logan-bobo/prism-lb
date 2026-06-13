use anyhow::Error;
use log::{error, info};
use prism_lb::{config::Config, server::Server};
use std::sync::Arc;
use tokio::{fs::File, io::AsyncReadExt};

#[tokio::main]
async fn main() -> Result<(), Error> {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "debug");
    }

    env_logger::init();

    info!("building configuration");

    let mut file = File::open("backends.yaml").await?;

    let mut contents = String::new();

    file.read_to_string(&mut contents).await?;

    let config = Config::try_from(contents)?;

    info!("building server");

    let server = Arc::new(Server::new(config).await?);

    if let Err(error) = server.serve().await {
        error!("{error}");
    };

    Ok(())
}
