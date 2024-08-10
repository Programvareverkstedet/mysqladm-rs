use std::{fs, path::PathBuf};

use anyhow::Context;
use nix::libc::{exit, EXIT_SUCCESS};
use std::os::unix::net::UnixStream as StdUnixStream;
use tokio::net::UnixStream as TokioUnixStream;

use crate::{
    core::{
        bootstrap::authenticated_unix_socket::client_authenticate,
        common::{UnixUser, DEFAULT_CONFIG_PATH, DEFAULT_SOCKET_PATH},
    },
    server::{config::read_config_form_path, server_loop::handle_requests_for_single_session},
};

pub mod authenticated_unix_socket;

// TODO: this function is security critical, it should be integration tested
//       in isolation.
/// Drop privileges to the real user and group of the process.
/// If the process is not running with elevated privileges, this function
/// is a no-op.
pub fn drop_privs() -> anyhow::Result<()> {
    log::debug!("Dropping privileges");
    let real_uid = nix::unistd::getuid();
    let real_gid = nix::unistd::getgid();

    nix::unistd::setuid(real_uid).context("Failed to drop privileges")?;
    nix::unistd::setgid(real_gid).context("Failed to drop privileges")?;

    debug_assert_eq!(nix::unistd::getuid(), real_uid);
    debug_assert_eq!(nix::unistd::getgid(), real_gid);

    log::debug!("Privileges dropped successfully");
    Ok(())
}

/// This function is used to bootstrap the connection to the server.
/// This can happen in two ways:
/// 1. If a socket path is provided, or exists in the default location,
///    the function will connect to the socket and authenticate with the
///    server to ensure that the server knows the uid of the client.
/// 2. If a config path is provided, or exists in the default location,
///    and the config is readable, the function will assume it is either
///    setuid or setgid, and will fork a child process to run the server
///    with the provided config. The server will exit silently by itself
///    when it is done, and this function will only return for the client
///    with the socket for the server.
/// If neither of these options are available, the function will fail.
pub fn bootstrap_server_connection_and_drop_privileges(
    server_socket_path: Option<PathBuf>,
    config_path: Option<PathBuf>,
) -> anyhow::Result<StdUnixStream> {
    if server_socket_path.is_some() && config_path.is_some() {
        anyhow::bail!("Cannot provide both a socket path and a config path");
    }

    log::debug!("Starting the server connection bootstrap process");

    let (socket, do_authenticate) = bootstrap_server_connection(server_socket_path, config_path)?;

    drop_privs()?;

    let result: anyhow::Result<StdUnixStream> = if do_authenticate {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let mut socket = TokioUnixStream::from_std(socket)?;
                client_authenticate(&mut socket, None).await?;
                Ok(socket.into_std()?)
            })
    } else {
        Ok(socket)
    };

    result
}

/// Inner function for [`bootstrap_server_connection_and_drop_privileges`].
/// See that function for more information.
fn bootstrap_server_connection(
    socket_path: Option<PathBuf>,
    config_path: Option<PathBuf>,
) -> anyhow::Result<(StdUnixStream, bool)> {
    // TODO: ensure this is both readable and writable
    if let Some(socket_path) = socket_path {
        log::debug!("Connecting to socket at {:?}", socket_path);
        return match StdUnixStream::connect(socket_path) {
            Ok(socket) => Ok((socket, true)),
            Err(e) => match e.kind() {
                std::io::ErrorKind::NotFound => Err(anyhow::anyhow!("Socket not found")),
                std::io::ErrorKind::PermissionDenied => Err(anyhow::anyhow!("Permission denied")),
                _ => Err(anyhow::anyhow!("Failed to connect to socket: {}", e)),
            },
        };
    }
    if let Some(config_path) = config_path {
        // ensure config exists and is readable
        if fs::metadata(&config_path).is_err() {
            return Err(anyhow::anyhow!("Config file not found or not readable"));
        }

        log::debug!("Starting server with config at {:?}", config_path);
        return invoke_server_with_config(config_path).map(|socket| (socket, false));
    }

    if fs::metadata(DEFAULT_SOCKET_PATH).is_ok() {
        return match StdUnixStream::connect(DEFAULT_SOCKET_PATH) {
            Ok(socket) => Ok((socket, true)),
            Err(e) => match e.kind() {
                std::io::ErrorKind::NotFound => Err(anyhow::anyhow!("Socket not found")),
                std::io::ErrorKind::PermissionDenied => Err(anyhow::anyhow!("Permission denied")),
                _ => Err(anyhow::anyhow!("Failed to connect to socket: {}", e)),
            },
        };
    }

    let config_path = PathBuf::from(DEFAULT_CONFIG_PATH);
    if fs::metadata(&config_path).is_ok() {
        return invoke_server_with_config(config_path).map(|socket| (socket, false));
    }

    anyhow::bail!("No socket path or config path provided, and no default socket or config found");
}

// TODO: we should somehow ensure that the forked process is killed on completion,
//       just in case the client does not behave properly.
/// Fork a child process to run the server with the provided config.
/// The server will exit silently by itself when it is done, and this function
/// will only return for the client with the socket for the server.
fn invoke_server_with_config(config_path: PathBuf) -> anyhow::Result<StdUnixStream> {
    let (server_socket, client_socket) = StdUnixStream::pair()?;
    let unix_user = UnixUser::from_uid(nix::unistd::getuid().as_raw())?;

    match (unsafe { nix::unistd::fork() }).context("Failed to fork")? {
        nix::unistd::ForkResult::Parent { child } => {
            log::debug!("Forked child process with PID {}", child);
            Ok(client_socket)
        }
        nix::unistd::ForkResult::Child => {
            log::debug!("Running server in child process");

            match run_forked_server(config_path, server_socket, unix_user) {
                Err(e) => Err(e),
                Ok(_) => unreachable!(),
            }
        }
    }
}

/// Run the server in the forked child process.
/// This function will not return, but will exit the process with a success code.
fn run_forked_server(
    config_path: PathBuf,
    server_socket: StdUnixStream,
    unix_user: UnixUser,
) -> anyhow::Result<()> {
    let config = read_config_form_path(Some(config_path))?;

    let result: anyhow::Result<()> = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let socket = TokioUnixStream::from_std(server_socket)?;
            handle_requests_for_single_session(socket, &unix_user, &config).await?;
            Ok(())
        });

    result?;

    unsafe {
        exit(EXIT_SUCCESS);
    }
}
