use std::net::IpAddr;

use derive_getters::Getters;
use serde_derive::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize, Getters)]
pub struct Config {
    bind_interface: IpAddr,
    bind_port: u32,
    backends: Vec<(IpAddr, u32)>,
    health_check: HealthCheck,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct HealthCheck {
    interval: u32,
    failure_threshold: u32,
}

impl TryFrom<String> for Config {
    type Error = serde_yaml::Error;

    fn try_from(value: String) -> Result<Self, serde_yaml::Error> {
        let config: Config = serde_yaml::from_str(&value)?;

        Ok(config)
    }
}
