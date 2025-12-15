use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};
use tracing_subscriber::prelude::*;

use crate::{
    core::common::{ASCII_BANNER, DEFAULT_CONFIG_PATH, KIND_REGARDS},
    server::supervisor::Supervisor,
};

#[derive(Parser, Debug, Clone)]
pub struct ServerArgs {
    #[command(subcommand)]
    pub subcmd: ServerCommand,

    /// Enable systemd mode
    #[cfg(target_os = "linux")]
    #[arg(long)]
    pub systemd: bool,

    /// Disable Landlock sandboxing.
    ///
    /// This is useful if you are planning to reload the server's configuration.
    #[arg(long)]
    pub disable_landlock: bool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ServerCommand {
    /// Start the server and listen for incoming connections on the unix socket
    /// specified in the configuration file.
    Listen,

    /// Start the server using systemd socket activation.
    SocketActivate,
}

const LOG_LEVEL_WARNING: &str = r#"
===================================================
== WARNING: LOG LEVEL IS SET TO 'TRACE'!         ==
== THIS WILL CAUSE THE SERVER TO LOG SQL QUERIES ==
== THAT MAY CONTAIN SENSITIVE INFORMATION LIKE   ==
== PASSWORDS AND AUTHENTICATION TOKENS.          ==
== THIS IS INTENDED FOR DEBUGGING PURPOSES ONLY  ==
== AND SHOULD *NEVER* BE USED IN PRODUCTION.     ==
===================================================
"#;

pub fn trace_server_prelude() {
    let message = [ASCII_BANNER, "", KIND_REGARDS, ""].join("\n");
    tracing::info!(message);
}

pub async fn handle_command(
    config_path: Option<PathBuf>,
    verbosity: Verbosity<InfoLevel>,
    args: ServerArgs,
) -> anyhow::Result<()> {
    let mut auto_detected_systemd_mode = false;

    #[cfg(target_os = "linux")]
    let systemd_mode = args.systemd || {
        if let Ok(true) = sd_notify::booted() {
            auto_detected_systemd_mode = true;
            true
        } else {
            false
        }
    };

    #[cfg(not(target_os = "linux"))]
    let systemd_mode = false;

    if systemd_mode {
        #[cfg(target_os = "linux")]
        {
            let subscriber = tracing_subscriber::Registry::default()
                .with(verbosity.tracing_level_filter())
                .with(tracing_journald::layer()?);

            tracing::subscriber::set_global_default(subscriber)
                .context("Failed to set global default tracing subscriber")?;

            trace_server_prelude();

            if verbosity.tracing_level_filter() >= tracing::Level::TRACE {
                tracing::warn!("{}", LOG_LEVEL_WARNING.trim());
            }

            if auto_detected_systemd_mode {
                tracing::debug!("Running in systemd mode, auto-detected");
            } else {
                tracing::debug!("Running in systemd mode");
            }
        }
    } else {
        let subscriber = tracing_subscriber::Registry::default()
            .with(verbosity.tracing_level_filter())
            .with(
                tracing_subscriber::fmt::layer()
                    .with_line_number(cfg!(debug_assertions))
                    .with_target(cfg!(debug_assertions))
                    .with_thread_ids(false)
                    .with_thread_names(false),
            );

        tracing::subscriber::set_global_default(subscriber)
            .context("Failed to set global default tracing subscriber")?;

        trace_server_prelude();

        tracing::debug!("Running in standalone mode");
    }

    let config_path = config_path.unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH));

    match args.subcmd {
        ServerCommand::Listen => {
            Supervisor::new(config_path, systemd_mode)
                .await?
                .run()
                .await
        }
        ServerCommand::SocketActivate => {
            if !args.systemd {
                anyhow::bail!(concat!(
                    "The `--systemd` flag must be used with the `socket-activate` command.\n",
                    "This command currently only supports socket activation under systemd."
                ));
            }

            Supervisor::new(config_path, systemd_mode)
                .await?
                .run()
                .await
        }
    }
}
