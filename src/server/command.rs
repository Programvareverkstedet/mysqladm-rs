use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use clap_verbosity_flag::Verbosity;
use systemd_journal_logger::JournalLog;

use crate::{core::common::DEFAULT_CONFIG_PATH, server::supervisor::Supervisor};

#[derive(Parser, Debug, Clone)]
pub struct ServerArgs {
    #[command(subcommand)]
    subcmd: ServerCommand,

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
    } else {
        env_logger::Builder::new()
            .filter_level(verbosity.log_level_filter())
            .init();

        log::info!("Running in standalone mode");
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
