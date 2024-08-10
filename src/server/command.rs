use std::os::fd::FromRawFd;
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;

use std::os::unix::net::UnixStream as StdUnixStream;
use tokio::net::UnixStream as TokioUnixStream;

use crate::core::bootstrap::authenticated_unix_socket;
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

    // if let Err(e) = &result {
    //     eprintln!("{}", e);
    // }

    match args.subcmd {
        ServerCommand::Listen => listen_for_incoming_connections(socket_path, config).await,
        ServerCommand::SocketActivate => socket_activate(config).await,
    }
}

async fn socket_activate(config: ServerConfig) -> anyhow::Result<()> {
    // TODO: allow getting socket path from other socket activation sources
    let mut conn = get_socket_from_systemd().await?;
    let uid = authenticated_unix_socket::server_authenticate(&mut conn).await?;
    let unix_user = UnixUser::from_uid(uid.into())?;
    handle_requests_for_single_session(conn, &unix_user, &config).await?;

    Ok(())
}

async fn get_socket_from_systemd() -> anyhow::Result<TokioUnixStream> {
    let fd = std::env::var("LISTEN_FDS")
        .context("LISTEN_FDS not set, not running under systemd?")?
        .parse::<i32>()
        .context("Failed to parse LISTEN_FDS")?;

    if fd != 1 {
        return Err(anyhow::anyhow!("Unexpected LISTEN_FDS value: {}", fd));
    }

    let std_unix_stream = unsafe { StdUnixStream::from_raw_fd(fd) };
    let socket = TokioUnixStream::from_std(std_unix_stream)?;
    Ok(socket)
}
