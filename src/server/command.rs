use std::os::fd::FromRawFd;
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use clap_verbosity_flag::Verbosity;
use futures::SinkExt;
use indoc::concatdoc;
use systemd_journal_logger::JournalLog;

use std::os::unix::net::UnixStream as StdUnixStream;
use tokio::net::UnixStream as TokioUnixStream;

use crate::core::common::UnixUser;
use crate::core::protocol::{create_server_to_client_message_stream, Response};
use crate::server::config::read_config_from_path_with_arg_overrides;
use crate::server::server_loop::listen_for_incoming_connections;
use crate::server::{
    config::{ServerConfig, ServerConfigArgs},
    server_loop::handle_requests_for_single_session,
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
        ServerCommand::Listen => listen_for_incoming_connections(socket_path, config).await,
        ServerCommand::SocketActivate => {
            if !args.systemd {
                anyhow::bail!(concat!(
                    "The `--systemd` flag must be used with the `socket-activate` command.\n",
                    "This command currently only supports socket activation under systemd."
                ));
            }

            socket_activate(config).await
        }
    }
}

fn start_watchdog_thread_if_enabled() {
    let mut micro_seconds: u64 = 0;
    let watchdog_enabled = sd_notify::watchdog_enabled(true, &mut micro_seconds);

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

async fn socket_activate(config: ServerConfig) -> anyhow::Result<()> {
    let conn = get_socket_from_systemd().await?;

    let uid = match conn.peer_cred() {
        Ok(cred) => cred.uid(),
        Err(e) => {
            log::error!("Failed to get peer credentials from socket: {}", e);
            let mut message_stream = create_server_to_client_message_stream(conn);
            message_stream
                .send(Response::Error(
                    (concatdoc! {
                        "Server failed to get peer credentials from socket\n",
                        "Please check the server logs or contact the system administrators"
                    })
                    .to_string(),
                ))
                .await
                .ok();
            anyhow::bail!("Failed to get peer credentials from socket");
        }
    };

    log::debug!("Accepted connection from uid {}", uid);

    let unix_user = match UnixUser::from_uid(uid) {
        Ok(user) => user,
        Err(e) => {
            log::error!("Failed to get username from uid: {}", e);
            let mut message_stream = create_server_to_client_message_stream(conn);
            message_stream
                .send(Response::Error(
                    (concatdoc! {
                        "Server failed to get user data from the system\n",
                        "Please check the server logs or contact the system administrators"
                    })
                    .to_string(),
                ))
                .await
                .ok();
            anyhow::bail!("Failed to get username from uid");
        }
    };

    log::info!("Accepted connection from {}", unix_user.username);

    sd_notify::notify(false, &[sd_notify::NotifyState::Ready]).ok();

    handle_requests_for_single_session(conn, &unix_user, &config).await?;

    Ok(())
}

async fn get_socket_from_systemd() -> anyhow::Result<TokioUnixStream> {
    let fd = sd_notify::listen_fds()
        .context("Failed to get file descriptors from systemd")?
        .next()
        .context("No file descriptors received from systemd")?;

    debug_assert!(fd == 3, "Unexpected file descriptor from systemd: {}", fd);

    log::debug!(
        "Received file descriptor from systemd with id: '{}', assuming socket",
        fd
    );

    let std_unix_stream = unsafe { StdUnixStream::from_raw_fd(fd) };
    let socket = TokioUnixStream::from_std(std_unix_stream)?;
    Ok(socket)
}
