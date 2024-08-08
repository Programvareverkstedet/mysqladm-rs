use std::{fs, path::PathBuf, time::Duration};

use anyhow::{anyhow, Context};
use clap::Parser;
use serde::{Deserialize, Serialize};
use sqlx::{mysql::MySqlConnectOptions, ConnectOptions, MySqlConnection};

// NOTE: this might look empty now, and the extra wrapping for the mysql
//       config seems unnecessary, but it will be useful later when we
//       add more configuration options.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
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

const DEFAULT_PORT: u16 = 3306;
const DEFAULT_TIMEOUT: u64 = 2;

#[derive(Parser)]
pub struct GlobalConfigArgs {
    /// Path to the configuration file.
    #[arg(
        short,
        long,
        value_name = "PATH",
        global = true,
        hide_short_help = true,
        default_value = "/etc/mysqladm/config.toml"
    )]
    config_file: String,

    /// Hostname of the MySQL server.
    #[arg(long, value_name = "HOST", global = true, hide_short_help = true)]
    mysql_host: Option<String>,

    /// Port of the MySQL server.
    #[arg(long, value_name = "PORT", global = true, hide_short_help = true)]
    mysql_port: Option<u16>,

    /// Username to use for the MySQL connection.
    #[arg(long, value_name = "USER", global = true, hide_short_help = true)]
    mysql_user: Option<String>,

    /// Path to a file containing the MySQL password.
    #[arg(long, value_name = "PATH", global = true, hide_short_help = true)]
    mysql_password_file: Option<String>,

    /// Seconds to wait for the MySQL connection to be established.
    #[arg(long, value_name = "SECONDS", global = true, hide_short_help = true)]
    mysql_connect_timeout: Option<u64>,
}

/// Use the arguments and whichever configuration file which might or might not
/// be found and default values to determine the configuration for the program.
pub fn get_config(args: GlobalConfigArgs) -> anyhow::Result<Config> {
    let config_path = PathBuf::from(args.config_file);

    let config: Config = fs::read_to_string(&config_path)
        .context(format!(
            "Failed to read config file from {:?}",
            &config_path
        ))
        .and_then(|c| toml::from_str(&c).context("Failed to parse config file"))
        .context(format!(
            "Failed to parse config file from {:?}",
            &config_path
        ))?;

    let mysql = &config.mysql;

    let password = if let Some(path) = args.mysql_password_file {
        fs::read_to_string(path)
            .context("Failed to read MySQL password file")
            .map(|s| s.trim().to_owned())?
    } else {
        mysql.password.to_owned()
    };

    let mysql_config = MysqlConfig {
        host: args.mysql_host.unwrap_or(mysql.host.to_owned()),
        port: args.mysql_port.or(mysql.port),
        username: args.mysql_user.unwrap_or(mysql.username.to_owned()),
        password,
        timeout: args.mysql_connect_timeout.or(mysql.timeout),
    };

    Ok(Config {
        mysql: mysql_config,
    })
}

/// Use the provided configuration to establish a connection to a MySQL server.
pub async fn create_mysql_connection_from_config(
    config: MysqlConfig,
) -> anyhow::Result<MySqlConnection> {
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
