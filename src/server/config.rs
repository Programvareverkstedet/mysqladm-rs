use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use sqlx::{ConnectOptions, mysql::MySqlConnectOptions};

pub const DEFAULT_PORT: u16 = 3306;
fn default_mysql_port() -> u16 {
    DEFAULT_PORT
}

pub const DEFAULT_TIMEOUT: u64 = 2;
fn default_mysql_timeout() -> u64 {
    DEFAULT_TIMEOUT
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename = "mysql")]
pub struct MysqlConfig {
    pub socket_path: Option<PathBuf>,
    pub host: Option<String>,
    #[serde(default = "default_mysql_port")]
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub password_file: Option<PathBuf>,
    #[serde(default = "default_mysql_timeout")]
    pub timeout: u64,
}

impl MysqlConfig {
    pub fn as_mysql_connect_options(&self) -> anyhow::Result<MySqlConnectOptions> {
        let mut options = MySqlConnectOptions::new()
            .database("mysql")
            .log_statements(tracing::log::LevelFilter::Trace);

        if let Some(username) = &self.username {
            options = options.username(username);
        }

        if let Some(password) = &self.password {
            options = options.password(password);
        }

        if let Some(socket_path) = &self.socket_path {
            options = options.socket(socket_path);
        } else if let Some(host) = &self.host {
            options = options.host(host);
            options = options.port(self.port);
        } else {
            anyhow::bail!("No MySQL host or socket path provided");
        }

        Ok(options)
    }

    pub fn log_connection_notice(&self) {
        let mut display_config = self.to_owned();
        display_config.password = display_config
            .password
            .as_ref()
            .map(|_| "<REDACTED>".to_owned());
        tracing::debug!(
            "Connecting to MySQL server with parameters: {:#?}",
            display_config
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ServerConfig {
    pub socket_path: Option<PathBuf>,
    pub mysql: MysqlConfig,
}

impl ServerConfig {
    /// Reads the server configuration from the specified path, or the default path if none is provided.
    pub fn read_config_from_path(config_path: &Path) -> anyhow::Result<Self> {
        tracing::debug!("Reading config file at {:?}", config_path);

        fs::read_to_string(config_path)
            .context(format!("Failed to read config file at {:?}", config_path))
            .and_then(|c| toml::from_str(&c).context("Failed to parse config file"))
            .context(format!("Failed to parse config file at {:?}", config_path))
    }
}
