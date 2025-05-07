use std::{fs, path::PathBuf, time::Duration};

use anyhow::{Context, anyhow};
use clap::Parser;
use serde::{Deserialize, Serialize};
use sqlx::{ConnectOptions, MySqlConnection, mysql::MySqlConnectOptions};

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
    pub socket_path: Option<PathBuf>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub password_file: Option<PathBuf>,
    pub timeout: Option<u64>,
}

#[derive(Parser, Debug, Clone)]
pub struct ServerConfigArgs {
    /// Path to the socket of the MySQL server.
    #[arg(long, value_name = "PATH", global = true)]
    socket_path: Option<PathBuf>,

    /// Hostname of the MySQL server.
    #[arg(
        long,
        value_name = "HOST",
        global = true,
        conflicts_with = "socket_path"
    )]
    mysql_host: Option<String>,

    /// Port of the MySQL server.
    #[arg(
        long,
        value_name = "PORT",
        global = true,
        conflicts_with = "socket_path"
    )]
    mysql_port: Option<u16>,

    /// Username to use for the MySQL connection.
    #[arg(long, value_name = "USER", global = true)]
    mysql_user: Option<String>,

    /// Path to a file containing the MySQL password.
    #[arg(long, value_name = "PATH", global = true)]
    mysql_password_file: Option<PathBuf>,

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
    let config = read_config_from_path(config_path)?;

    let mysql = config.mysql;

    let password = if let Some(path) = &args.mysql_password_file {
        Some(
            fs::read_to_string(path)
                .context("Failed to read MySQL password file")
                .map(|s| s.trim().to_owned())?,
        )
    } else if let Some(path) = &mysql.password_file {
        Some(
            fs::read_to_string(path)
                .context("Failed to read MySQL password file")
                .map(|s| s.trim().to_owned())?,
        )
    } else {
        mysql.password.to_owned()
    };

    Ok(ServerConfig {
        mysql: MysqlConfig {
            socket_path: args.socket_path.or(mysql.socket_path),
            host: args.mysql_host.or(mysql.host),
            port: args.mysql_port.or(mysql.port),
            username: args.mysql_user.or(mysql.username.to_owned()),
            password,
            password_file: args.mysql_password_file.or(mysql.password_file),
            timeout: args.mysql_connect_timeout.or(mysql.timeout),
        },
    })
}

pub fn read_config_from_path(config_path: Option<PathBuf>) -> anyhow::Result<ServerConfig> {
    let config_path = config_path.unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH));

    log::debug!("Reading config file at {:?}", &config_path);

    fs::read_to_string(&config_path)
        .context(format!("Failed to read config file at {:?}", &config_path))
        .and_then(|c| toml::from_str(&c).context("Failed to parse config file"))
        .context(format!("Failed to parse config file at {:?}", &config_path))
}

fn log_config(config: &MysqlConfig) {
    let mut display_config = config.to_owned();
    display_config.password = display_config
        .password
        .as_ref()
        .map(|_| "<REDACTED>".to_owned());
    log::debug!(
        "Connecting to MySQL server with parameters: {:#?}",
        display_config
    );
}

/// Use the provided configuration to establish a connection to a MySQL server.
pub async fn create_mysql_connection_from_config(
    config: &MysqlConfig,
) -> anyhow::Result<MySqlConnection> {
    log_config(config);

    let mut mysql_options = MySqlConnectOptions::new()
        .database("mysql")
        .log_statements(log::LevelFilter::Trace);

    if let Some(username) = &config.username {
        mysql_options = mysql_options.username(username);
    }

    if let Some(password) = &config.password {
        mysql_options = mysql_options.password(password);
    }

    if let Some(socket_path) = &config.socket_path {
        mysql_options = mysql_options.socket(socket_path);
    } else if let Some(host) = &config.host {
        mysql_options = mysql_options.host(host);
        mysql_options = mysql_options.port(config.port.unwrap_or(DEFAULT_PORT));
    } else {
        anyhow::bail!("No MySQL host or socket path provided");
    }

    match tokio::time::timeout(
        Duration::from_secs(config.timeout.unwrap_or(DEFAULT_TIMEOUT)),
        mysql_options.connect(),
    )
    .await
    {
        Ok(connection) => connection.context("Failed to connect to the database"),
        Err(_) => {
            Err(anyhow!("Timed out after 2 seconds")).context("Failed to connect to the database")
        }
    }
}
