use std::{fs, path::PathBuf};

use anyhow::Context;
use clap::Parser;
use serde::{Deserialize, Serialize};
use sqlx::{mysql::MySqlConnectOptions, ConnectOptions, MySqlConnection};

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
}

#[derive(Parser)]
pub struct ConfigOverrideArgs {
    #[arg(
      long,
      value_name = "PATH",
      global = true,
      help_heading = Some("Configuration overrides"),
      hide_short_help = true,
      alias = "config",
      alias = "conf",
    )]
    config_file: Option<String>,

    #[arg(
      long,
      value_name = "HOST",
      global = true,
      help_heading = Some("Configuration overrides"),
      hide_short_help = true,
    )]
    mysql_host: Option<String>,

    #[arg(
      long,
      value_name = "PORT",
      global = true,
      help_heading = Some("Configuration overrides"),
      hide_short_help = true,
    )]
    mysql_port: Option<u16>,

    #[arg(
      long,
      value_name = "USER",
      global = true,
      help_heading = Some("Configuration overrides"),
      hide_short_help = true,
    )]
    mysql_user: Option<String>,

    #[arg(
      long,
      value_name = "PATH",
      global = true,
      help_heading = Some("Configuration overrides"),
      hide_short_help = true,
    )]
    mysql_password_file: Option<String>,
}

pub fn get_config(args: ConfigOverrideArgs) -> anyhow::Result<Config> {
    let config_path = args
        .config_file
        .unwrap_or("/etc/mysqladm/config.toml".to_string());
    let config_path = PathBuf::from(config_path);

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
    };

    Ok(Config {
        mysql: mysql_config,
    })
}

/// TODO: Add timeout.
pub async fn mysql_connection_from_config(config: Config) -> anyhow::Result<MySqlConnection> {
    MySqlConnectOptions::new()
        .host(&config.mysql.host)
        .username(&config.mysql.username)
        .password(&config.mysql.password)
        .port(config.mysql.port.unwrap_or(3306))
        .database("mysql")
        .connect()
        .await
        .context("Failed to connect to MySQL")
}
