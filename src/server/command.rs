use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use clap_verbosity_flag::Verbosity;
use systemd_journal_logger::JournalLog;

use crate::server::{
    config::{ServerConfigArgs, read_config_from_path_with_arg_overrides},
    server_loop::{
        listen_for_incoming_connections_with_socket_path,
        listen_for_incoming_connections_with_systemd_socket,
    },
};

#[derive(Parser, Debug, Clone)]
pub struct ServerArgs {
    #[command(subcommand)]
    subcmd: ServerCommand,

    #[command(flatten)]
    config_overrides: ServerConfigArgs,

    #[arg(long)]
    systemd: bool,
}

#[derive(Parser, Debug, Clone)]
pub enum ServerCommand {
    #[command()]
    Listen,

    #[command()]
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

pub async fn handle_command(
    socket_path: Option<PathBuf>,
    config_path: Option<PathBuf>,
    verbosity: Verbosity,
    args: ServerArgs,
) -> anyhow::Result<()> {
    let mut auto_detected_systemd_mode = false;
    let systemd_mode = args.systemd || {
        if let Ok(true) = sd_notify::booted() {
            auto_detected_systemd_mode = true;
            true
        } else {
            false
        }
    };
    if systemd_mode {
        JournalLog::new()
            .context("Failed to initialize journald logging")?
            .install()
            .context("Failed to install journald logger")?;

        log::set_max_level(verbosity.log_level_filter());

        if verbosity.log_level_filter() >= log::LevelFilter::Trace {
            log::warn!("{}", LOG_LEVEL_WARNING.trim());
        }

        if auto_detected_systemd_mode {
            log::info!("Running in systemd mode, auto-detected");
        } else {
            log::info!("Running in systemd mode");
        }

        start_watchdog_thread_if_enabled();
    } else {
        env_logger::Builder::new()
            .filter_level(verbosity.log_level_filter())
            .init();

        log::info!("Running in standalone mode");
    }

    let config = read_config_from_path_with_arg_overrides(config_path, args.config_overrides)?;

    match args.subcmd {
        ServerCommand::Listen => {
            listen_for_incoming_connections_with_socket_path(socket_path, config).await
        }
        ServerCommand::SocketActivate => {
            if !args.systemd {
                anyhow::bail!(concat!(
                    "The `--systemd` flag must be used with the `socket-activate` command.\n",
                    "This command currently only supports socket activation under systemd."
                ));
            }

            listen_for_incoming_connections_with_systemd_socket(config).await
        }
    }
}

fn start_watchdog_thread_if_enabled() {
    let mut micro_seconds: u64 = 0;
    let watchdog_enabled = sd_notify::watchdog_enabled(false, &mut micro_seconds);

    if watchdog_enabled {
        micro_seconds = micro_seconds.max(2_000_000).div_ceil(2);

        tokio::spawn(async move {
            log::debug!(
                "Starting systemd watchdog thread with {} millisecond interval",
                micro_seconds.div_ceil(1000)
            );
            loop {
                tokio::time::sleep(tokio::time::Duration::from_micros(micro_seconds)).await;
                if let Err(err) = sd_notify::notify(false, &[sd_notify::NotifyState::Watchdog]) {
                    log::warn!("Failed to notify systemd watchdog: {}", err);
                } else {
                    log::trace!("Ping sent to systemd watchdog");
                }
            }
        });
    } else {
        log::debug!("Systemd watchdog not enabled, skipping watchdog thread");
    }
}
