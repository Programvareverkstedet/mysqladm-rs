use std::{fs, path::PathBuf};

use anyhow::Context;
use clap_verbosity_flag::Verbosity;
use nix::libc::{EXIT_SUCCESS, exit};
use std::os::unix::net::UnixStream as StdUnixStream;
use tokio::net::UnixStream as TokioUnixStream;

use crate::{
    core::common::{
        DEFAULT_CONFIG_PATH, DEFAULT_SOCKET_PATH, UnixUser, executable_is_suid_or_sgid,
    },
    server::{config::read_config_from_path, session_handler},
};

/// Determine whether we will make a connection to an external server
/// or start an internal server with elevated privileges.
///
/// If neither is feasible, an error is returned.
fn will_connect_to_external_server(
    server_socket_path: Option<&PathBuf>,
    // This parameter is only used in suid-sgid-mode
    #[allow(unused_variables)] config_path: Option<&PathBuf>,
) -> anyhow::Result<bool> {
    if server_socket_path.is_some() {
        return Ok(true);
    }

    #[cfg(feature = "suid-sgid-mode")]
    if config_path.is_some() {
        return Ok(false);
    }

    if fs::metadata(DEFAULT_SOCKET_PATH).is_ok() {
        return Ok(true);
    }

    #[cfg(feature = "suid-sgid-mode")]
    if fs::metadata(DEFAULT_CONFIG_PATH).is_ok() {
        return Ok(false);
    }

    #[cfg(feature = "suid-sgid-mode")]
    anyhow::bail!("No socket path or config path provided, and no default socket or config found");

    #[cfg(not(feature = "suid-sgid-mode"))]
    anyhow::bail!("No socket path provided, and no default socket found");
}

/// This function is used to bootstrap the connection to the server.
/// This can happen in two ways:
///
/// 1. If a socket path is provided, or exists in the default location,
///    the function will connect to the socket and authenticate with the
///    server to ensure that the server knows the uid of the client.
///
/// 2. If a config path is provided, or exists in the default location,
///    and the config is readable, the function will assume it is either
///    setuid or setgid, and will fork a child process to run the server
///    with the provided config. The server will exit silently by itself
///    when it is done, and this function will only return for the client
///    with the socket for the server.
///
/// If neither of these options are available, the function will fail.
///
/// Note that this function is also responsible for setting up logging,
/// because in the case of an internal server, we need to drop privileges
/// before we can initialize logging.
///
/// **WARNING:** This function may be run with elevated privileges.
pub fn bootstrap_server_connection_and_drop_privileges(
    server_socket_path: Option<PathBuf>,
    config: Option<PathBuf>,
    verbose: Verbosity,
) -> anyhow::Result<StdUnixStream> {
    if will_connect_to_external_server(server_socket_path.as_ref(), config.as_ref())? {
        assert!(
            !executable_is_suid_or_sgid()?,
            "The executable should not be SUID or SGID when connecting to an external server"
        );

        env_logger::Builder::new()
            .filter_level(verbose.log_level_filter())
            .init();

        connect_to_external_server(server_socket_path)
    } else if cfg!(feature = "suid-sgid-mode") {
        // NOTE: We need to be really careful with the code up until this point,
        //       as we might be running with elevated privileges.
        let server_connection = bootstrap_internal_server_and_drop_privs(config)?;

        env_logger::Builder::new()
            .filter_level(verbose.log_level_filter())
            .init();

        Ok(server_connection)
    } else {
        anyhow::bail!("SUID/SGID support is not enabled, cannot start internal server");
    }
}

fn connect_to_external_server(
    server_socket_path: Option<PathBuf>,
) -> anyhow::Result<StdUnixStream> {
    // TODO: ensure this is both readable and writable
    if let Some(socket_path) = server_socket_path {
        log::debug!("Connecting to socket at {:?}", socket_path);
        return match StdUnixStream::connect(socket_path) {
            Ok(socket) => Ok(socket),
            Err(e) => match e.kind() {
                std::io::ErrorKind::NotFound => Err(anyhow::anyhow!("Socket not found")),
                std::io::ErrorKind::PermissionDenied => Err(anyhow::anyhow!("Permission denied")),
                _ => Err(anyhow::anyhow!("Failed to connect to socket: {}", e)),
            },
        };
    }

    if fs::metadata(DEFAULT_SOCKET_PATH).is_ok() {
        log::debug!("Connecting to default socket at {:?}", DEFAULT_SOCKET_PATH);
        return match StdUnixStream::connect(DEFAULT_SOCKET_PATH) {
            Ok(socket) => Ok(socket),
            Err(e) => match e.kind() {
                std::io::ErrorKind::NotFound => Err(anyhow::anyhow!("Socket not found")),
                std::io::ErrorKind::PermissionDenied => Err(anyhow::anyhow!("Permission denied")),
                _ => Err(anyhow::anyhow!("Failed to connect to socket: {}", e)),
            },
        };
    }

    anyhow::bail!("No socket path provided, and no default socket found");
}

// TODO: this function is security critical, it should be integration tested
//       in isolation.
/// Drop privileges to the real user and group of the process.
/// If the process is not running with elevated privileges, this function
/// is a no-op.
fn drop_privs() -> anyhow::Result<()> {
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

fn bootstrap_internal_server_and_drop_privs(
    config_path: Option<PathBuf>,
) -> anyhow::Result<StdUnixStream> {
    if let Some(config_path) = config_path {
        if !executable_is_suid_or_sgid()? {
            anyhow::bail!("Executable is not SUID/SGID - refusing to start internal sever");
        }

        // ensure config exists and is readable
        if fs::metadata(&config_path).is_err() {
            return Err(anyhow::anyhow!("Config file not found or not readable"));
        }

        log::debug!("Starting server with config at {:?}", config_path);
        let socket = invoke_server_with_config(config_path)?;
        drop_privs()?;
        return Ok(socket);
    };

    let config_path = PathBuf::from(DEFAULT_CONFIG_PATH);
    if fs::metadata(&config_path).is_ok() {
        if !executable_is_suid_or_sgid()? {
            anyhow::bail!("Executable is not SUID/SGID - refusing to start internal sever");
        }
        log::debug!("Starting server with default config at {:?}", config_path);
        let socket = invoke_server_with_config(config_path)?;
        drop_privs()?;
        return Ok(socket);
    };

    anyhow::bail!("No config path provided, and no default config found");
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
    let config = read_config_from_path(Some(config_path))?;

    let result: anyhow::Result<()> = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let socket = TokioUnixStream::from_std(server_socket)?;
            session_handler::session_handler(socket, &unix_user, &config).await?;
            Ok(())
        });

    result?;

    unsafe {
        exit(EXIT_SUCCESS);
    }
}
