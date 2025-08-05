use std::sync::Arc;

use anyhow::Error;
use prism_lb::config::Config;
use prism_lb::server::Server;
use prism_lb::telemetry::{get_subscriber, init_subscriber};

use tokio::fs::File;
use tokio::io::AsyncReadExt;

use tracing::{error, info};

#[tokio::main]
#[tracing::instrument]
async fn main() -> Result<(), Error> {
    let subscriber = get_subscriber("prism-lb".into(), "info".into(), std::io::stdout);
    init_subscriber(subscriber);

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
