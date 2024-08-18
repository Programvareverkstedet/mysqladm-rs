use std::os::fd::FromRawFd;
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;

use std::os::unix::net::UnixStream as StdUnixStream;
use tokio::net::UnixStream as TokioUnixStream;

use crate::core::common::UnixUser;
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
}

#[derive(Parser, Debug, Clone)]
pub enum ServerCommand {
    #[command()]
    Listen,

    #[command()]
    SocketActivate,
}

pub async fn handle_command(
    socket_path: Option<PathBuf>,
    config_path: Option<PathBuf>,
    args: ServerArgs,
) -> anyhow::Result<()> {
    let config = read_config_from_path_with_arg_overrides(config_path, args.config_overrides)?;

    match args.subcmd {
        ServerCommand::Listen => listen_for_incoming_connections(socket_path, config).await,
        ServerCommand::SocketActivate => socket_activate(config).await,
    }
}

async fn socket_activate(config: ServerConfig) -> anyhow::Result<()> {
    // TODO: allow getting socket path from other socket activation sources
    let conn = get_socket_from_systemd().await?;
    let uid = conn.peer_cred()?.uid();
    let unix_user = UnixUser::from_uid(uid)?;

    log::info!("Accepted connection from {}", unix_user.username);

    sd_notify::notify(true, &[sd_notify::NotifyState::Ready]).ok();

    handle_requests_for_single_session(conn, &unix_user, &config).await?;

    Ok(())
}

async fn get_socket_from_systemd() -> anyhow::Result<TokioUnixStream> {
    let fd = sd_notify::listen_fds()
        .context("Failed to get file descriptors from systemd")?
        .next()
        .context("No file descriptors received from systemd")?;

    log::debug!("Received file descriptor from systemd: {}", fd);

    let std_unix_stream = unsafe { StdUnixStream::from_raw_fd(fd) };
    let socket = TokioUnixStream::from_std(std_unix_stream)?;
    Ok(socket)
}
