use std::{collections::HashMap, net::IpAddr};

use derive_getters::Getters;
use serde_derive::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize, Getters)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    bind_interface: IpAddr,
    bind_port: u32,
    backends: Vec<HashMap<String, String>>,
    health_check: HealthCheck,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
