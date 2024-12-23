use std::env;
use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, Default)]
#[allow(unused)]
pub struct Settings {
    /// hostname of this rendering server
    pub bind_to_host: String,
    /// port to listen on
    pub port: usize,
    /// path to the root certificate
    pub root_ca: String,
    /// path to the client certificate
    pub client_cert: String,
    /// path to the client key
    pub client_key: String,
}

impl Settings{
    pub fn new() -> Result<Self, ConfigError>{
        let run_mode = env::var("RUN_MODE").unwrap_or_else(|_| "development".into());

        let s = Config::builder().add_source(File::with_name("config/default"))
            .add_source( File::with_name(&format!("config/{}", run_mode))
                             .required(false),)
            .add_source(File::with_name("config/local").required(false))
            .add_source(Environment::with_prefix("app"))
            .build()?;

        s.try_deserialize()
    }
}