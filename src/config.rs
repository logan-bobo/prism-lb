use std::{collections::HashMap, net::IpAddr};

use derive_getters::Getters;
use serde_derive::Deserialize;

#[derive(Debug, Deserialize, Getters)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    bind_interface: IpAddr,
    bind_port: u16,
    backends: Vec<HashMap<String, String>>,
    pub health_check: HealthCheck,
}

#[derive(Debug, Deserialize, Getters)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheck {
    pub interval: u64,
    failure_threshold: usize,
}

impl TryFrom<String> for Config {
    type Error = serde_yaml::Error;

    fn try_from(value: String) -> Result<Self, serde_yaml::Error> {
        let config: Config = serde_yaml::from_str(&value)?;

        Ok(config)
    }
}
