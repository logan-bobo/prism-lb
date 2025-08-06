use std::sync::Arc;

use anyhow::Error;
use prism_lb::config::Config;
use prism_lb::server::Server;

use log::{error, info};
use tokio::fs::File;
use tokio::io::AsyncReadExt;

#[tokio::main]
async fn main() -> Result<(), Error> {
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
