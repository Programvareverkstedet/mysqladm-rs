use std::{fs, path::PathBuf, time::Duration};

use anyhow::{anyhow, Context};
use clap::Parser;
use serde::{Deserialize, Serialize};
use sqlx::{mysql::MySqlConnectOptions, ConnectOptions, MySqlConnection};

use crate::core::common::DEFAULT_CONFIG_PATH;

pub const DEFAULT_PORT: u16 = 3306;
pub const DEFAULT_TIMEOUT: u64 = 2;

// NOTE: this might look empty now, and the extra wrapping for the mysql
//       config seems unnecessary, but it will be useful later when we
//       add more configuration options.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub mysql: MysqlConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename = "mysql")]
pub struct MysqlConfig {
    pub host: String,
    pub port: Option<u16>,
    pub username: String,
    pub password: String,
    pub timeout: Option<u64>,
}

#[derive(Parser, Debug, Clone)]
pub struct ServerConfigArgs {
    /// Hostname of the MySQL server.
    #[arg(long, value_name = "HOST", global = true)]
    mysql_host: Option<String>,

    /// Port of the MySQL server.
    #[arg(long, value_name = "PORT", global = true)]
    mysql_port: Option<u16>,

    /// Username to use for the MySQL connection.
    #[arg(long, value_name = "USER", global = true)]
    mysql_user: Option<String>,

    /// Path to a file containing the MySQL password.
    #[arg(long, value_name = "PATH", global = true)]
    mysql_password_file: Option<String>,

    /// Seconds to wait for the MySQL connection to be established.
    #[arg(long, value_name = "SECONDS", global = true)]
    mysql_connect_timeout: Option<u64>,
}

/// Use the arguments and whichever configuration file which might or might not
/// be found and default values to determine the configuration for the program.
pub fn read_config_from_path_with_arg_overrides(
    config_path: Option<PathBuf>,
    args: ServerConfigArgs,
) -> anyhow::Result<ServerConfig> {
    let config = read_config_form_path(config_path)?;

    let mysql = &config.mysql;

    let password = if let Some(path) = args.mysql_password_file {
        fs::read_to_string(path)
            .context("Failed to read MySQL password file")
            .map(|s| s.trim().to_owned())?
    } else {
        mysql.password.to_owned()
    };

    Ok(ServerConfig {
        mysql: MysqlConfig {
            host: args.mysql_host.unwrap_or(mysql.host.to_owned()),
            port: args.mysql_port.or(mysql.port),
            username: args.mysql_user.unwrap_or(mysql.username.to_owned()),
            password,
            timeout: args.mysql_connect_timeout.or(mysql.timeout),
        },
    })
}

pub fn read_config_form_path(config_path: Option<PathBuf>) -> anyhow::Result<ServerConfig> {
    let config_path = config_path.unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH));

    log::debug!("Reading config from {:?}", &config_path);

    fs::read_to_string(&config_path)
        .context(format!(
            "Failed to read config file from {:?}",
            &config_path
        ))
        .and_then(|c| toml::from_str(&c).context("Failed to parse config file"))
        .context(format!(
            "Failed to parse config file from {:?}",
            &config_path
        ))
}

/// Use the provided configuration to establish a connection to a MySQL server.
pub async fn create_mysql_connection_from_config(
    config: &MysqlConfig,
) -> anyhow::Result<MySqlConnection> {
    let mut display_config = config.clone();
    "<REDACTED>".clone_into(&mut display_config.password);
    log::debug!(
        "Connecting to MySQL server with parameters: {:#?}",
        display_config
    );

    match tokio::time::timeout(
        Duration::from_secs(config.timeout.unwrap_or(DEFAULT_TIMEOUT)),
        MySqlConnectOptions::new()
            .host(&config.host)
            .username(&config.username)
            .password(&config.password)
            .port(config.port.unwrap_or(DEFAULT_PORT))
            .database("mysql")
            .connect(),
    )
    .await
    {
        Ok(connection) => connection.context("Failed to connect to MySQL"),
        Err(_) => Err(anyhow!("Timed out after 2 seconds")).context("Failed to connect to MySQL"),
    }
}
