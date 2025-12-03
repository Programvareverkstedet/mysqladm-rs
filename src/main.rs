#[macro_use]
extern crate prettytable;

use anyhow::Context;
use clap::{CommandFactory, Parser, crate_version};
use clap_complete::CompleteEnv;
use clap_verbosity_flag::{InfoLevel, Verbosity};

use std::path::PathBuf;

use std::os::unix::net::UnixStream as StdUnixStream;
use tokio::net::UnixStream as TokioUnixStream;

use futures_util::StreamExt;

use crate::{
    core::{
        bootstrap::bootstrap_server_connection_and_drop_privileges,
        common::{ASCII_BANNER, KIND_REGARDS, executable_is_suid_or_sgid},
        protocol::{Response, create_client_to_server_message_stream},
    },
    server::{command::ServerArgs, landlock::landlock_restrict_server},
};

#[cfg(feature = "mysql-admutils-compatibility")]
use crate::client::mysql_admutils_compatibility::{mysql_dbadm, mysql_useradm};

mod server;

mod client;
mod core;

const fn long_version() -> &'static str {
    macro_rules! feature {
        ($title:expr, $flag:expr) => {
            if cfg!(feature = $flag) {
                concat!($title, ": enabled")
            } else {
                concat!($title, ": disabled")
            }
        };
    }
    const_format::concatcp!(
        crate_version!(),
        "\n",
        "build profile: ",
        env!("BUILD_PROFILE"),
        "\n",
        "commit: ",
        env!("GIT_COMMIT"),
        "\n\n",
        "[features]\n",
        feature!("SUID/SGID mode", "suid-sgid-mode"),
        "\n",
        feature!(
            "mysql-admutils compatibility",
            "mysql-admutils-compatibility"
        ),
        "\n",
    )
}

const LONG_VERSION: &str = long_version();

/// Database administration tool for non-admin users to manage their own MySQL databases and users.
///
/// This tool allows you to manage users and databases in MySQL.
///
/// You are only allowed to manage databases and users that are prefixed with
/// either your username, or a group that you are a member of.
#[derive(Parser, Debug)]
#[command(
  bin_name = "muscl",
  author = "Programvareverkstedet <projects@pvv.ntnu.no>",
  version,
  about,
  disable_help_subcommand = true,
  before_long_help = ASCII_BANNER,
  after_long_help = KIND_REGARDS,
  long_version = LONG_VERSION,
)]
struct Args {
    #[command(subcommand)]
    command: Command,

    /// Path to the socket of the server, if it already exists.
    #[arg(
        short,
        long,
        value_name = "PATH",
        value_hint = clap::ValueHint::FilePath,
        global = true,
        hide_short_help = true
    )]
    server_socket_path: Option<PathBuf>,

    /// Config file to use for the server.
    #[arg(
        short,
        long,
        value_name = "PATH",
        value_hint = clap::ValueHint::FilePath,
        global = true,
        hide_short_help = true
    )]
    config: Option<PathBuf>,

    #[command(flatten)]
    verbose: Verbosity<InfoLevel>,
}

#[derive(Parser, Debug, Clone)]
enum Command {
    #[command(flatten)]
    Client(client::commands::ClientCommand),

    /// Run the server
    #[command(hide = true)]
    Server(server::command::ServerArgs),
}

/// **WARNING:** This function may be run with elevated privileges.
fn main() -> anyhow::Result<()> {
    if handle_dynamic_completion()?.is_some() {
        return Ok(());
    }

    #[cfg(feature = "mysql-admutils-compatibility")]
    if handle_mysql_admutils_command()?.is_some() {
        return Ok(());
    }

    let args: Args = Args::parse();

    if handle_server_command(&args)?.is_some() {
        return Ok(());
    }

    let connection = bootstrap_server_connection_and_drop_privileges(
        args.server_socket_path,
        args.config,
        args.verbose,
    )?;

    tokio_run_command(args.command, connection)?;

    Ok(())
}

/// **WARNING:** This function may be run with elevated privileges.
fn handle_dynamic_completion() -> anyhow::Result<Option<()>> {
    if std::env::var_os("COMPLETE").is_some() {
        #[cfg(feature = "suid-sgid-mode")]
        if executable_is_suid_or_sgid()? {
            use crate::core::bootstrap::drop_privs;
            drop_privs()?
        }

        let argv0 = std::env::args()
            .next()
            .and_then(|s| {
                PathBuf::from(s)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
            })
            .ok_or(anyhow::anyhow!(
                "Could not determine executable name for completion"
            ))?;

        let command = match argv0.as_str() {
            "muscl" => Args::command(),
            "mysql-dbadm" => mysql_dbadm::Command::command(),
            "mysql-useradm" => mysql_useradm::Command::command(),
            command => anyhow::bail!("Unknown executable name: `{}`", command),
        };

        CompleteEnv::with_factory(move || command.clone()).complete();

        Ok(Some(()))
    } else {
        Ok(None)
    }
}

/// **WARNING:** This function may be run with elevated privileges.
fn handle_mysql_admutils_command() -> anyhow::Result<Option<()>> {
    let argv0 = std::env::args().next().and_then(|s| {
        PathBuf::from(s)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
    });

    match argv0.as_deref() {
        Some("mysql-dbadm") => mysql_dbadm::main().map(Some),
        Some("mysql-useradm") => mysql_useradm::main().map(Some),
        _ => Ok(None),
    }
}

/// **WARNING:** This function may be run with elevated privileges.
fn handle_server_command(args: &Args) -> anyhow::Result<Option<()>> {
    match args.command {
        Command::Server(ref command) => {
            assert!(
                !executable_is_suid_or_sgid()?,
                "The executable should not be SUID or SGID when running the server manually"
            );

            if !command.disable_landlock {
                landlock_restrict_server(args.config.as_deref())
                    .context("Failed to apply Landlock restrictions to the server process")?;
            }

            tokio_start_server(
                args.config.to_owned(),
                args.verbose.to_owned(),
                command.to_owned(),
            )?;
            Ok(Some(()))
        }
        _ => Ok(None),
    }
}

const MIN_TOKIO_WORKER_THREADS: usize = 4;

/// Start a long-lived server using Tokio.
fn tokio_start_server(
    config_path: Option<PathBuf>,
    verbosity: Verbosity<InfoLevel>,
    args: ServerArgs,
) -> anyhow::Result<()> {
    let worker_thread_count = std::cmp::max(num_cpus::get(), MIN_TOKIO_WORKER_THREADS);

    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(worker_thread_count)
        .enable_all()
        .build()
        .context("Failed to start Tokio runtime")?
        .block_on(server::command::handle_command(
            config_path,
            verbosity,
            args,
        ))
}

/// Run the given commmand (from the client side) using Tokio.
///
/// **WARNING:** This function may be run with elevated privileges.
fn tokio_run_command(command: Command, server_connection: StdUnixStream) -> anyhow::Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("Failed to start Tokio runtime")?
        .block_on(async {
            let tokio_socket = TokioUnixStream::from_std(server_connection)?;
            let mut message_stream = create_client_to_server_message_stream(tokio_socket);

            while let Some(Ok(message)) = message_stream.next().await {
                match message {
                    Response::Error(err) => {
                        anyhow::bail!("{}", err);
                    }
                    Response::Ready => break,
                    message => {
                        eprintln!("Unexpected message from server: {:?}", message);
                    }
                }
            }

            match command {
                Command::Client(client_args) => {
                    client::commands::handle_command(client_args, message_stream).await
                }
                Command::Server(_) => unreachable!(),
            }
        })
}
