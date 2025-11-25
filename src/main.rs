#[macro_use]
extern crate prettytable;

use anyhow::Context;
use clap::{CommandFactory, Parser, ValueEnum};
use clap_complete::{Shell, generate};
use clap_verbosity_flag::Verbosity;

use std::path::PathBuf;

use std::os::unix::net::UnixStream as StdUnixStream;
use tokio::net::UnixStream as TokioUnixStream;

use futures_util::StreamExt;

use crate::{
    core::{
        bootstrap::bootstrap_server_connection_and_drop_privileges,
        common::executable_is_suid_or_sgid,
        protocol::{Response, create_client_to_server_message_stream},
    },
    server::command::ServerArgs,
};

#[cfg(feature = "mysql-admutils-compatibility")]
use crate::client::mysql_admutils_compatibility::{mysql_dbadm, mysql_useradm};

mod server;

mod client;
mod core;

/// Database administration tool for non-admin users to manage their own MySQL databases and users.
///
/// This tool allows you to manage users and databases in MySQL.
///
/// You are only allowed to manage databases and users that are prefixed with
/// either your username, or a group that you are a member of.
#[derive(Parser, Debug)]
#[command(bin_name = "mysqladm", version, about, disable_help_subcommand = true)]
struct Args {
    #[command(subcommand)]
    command: Command,

    /// Path to the socket of the server, if it already exists.
    #[arg(
        short,
        long,
        value_name = "PATH",
        global = true,
        hide_short_help = true
    )]
    server_socket_path: Option<PathBuf>,

    /// Config file to use for the server.
    #[arg(
        short,
        long,
        value_name = "PATH",
        global = true,
        hide_short_help = true
    )]
    config: Option<PathBuf>,

    #[command(flatten)]
    verbose: Verbosity,
}

#[derive(Parser, Debug, Clone)]
enum Command {
    #[command(flatten)]
    Client(client::commands::ClientCommand),

    #[command(hide = true)]
    Server(server::command::ServerArgs),

    #[command(hide = true)]
    GenerateCompletions(GenerateCompletionArgs),
}

#[derive(Parser, Debug, Clone)]
struct GenerateCompletionArgs {
    #[arg(long, default_value = "bash")]
    shell: Shell,

    #[arg(long, default_value = "mysqladm")]
    command: ToplevelCommands,
}

#[cfg(feature = "mysql-admutils-compatibility")]
#[derive(ValueEnum, Debug, Clone)]
enum ToplevelCommands {
    Mysqladm,
    MysqlDbadm,
    MysqlUseradm,
}

/// **WARNING:** This function may be run with elevated privileges.
fn main() -> anyhow::Result<()> {
    #[cfg(feature = "mysql-admutils-compatibility")]
    if handle_mysql_admutils_command()?.is_some() {
        return Ok(());
    }

    let args: Args = Args::parse();

    if handle_server_command(&args)?.is_some() {
        return Ok(());
    }

    if handle_generate_completions_command(&args)?.is_some() {
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
            tokio_start_server(
                args.server_socket_path.to_owned(),
                args.config.to_owned(),
                args.verbose.to_owned(),
                command.to_owned(),
            )?;
            Ok(Some(()))
        }
        _ => Ok(None),
    }
}

/// **WARNING:** This function may be run with elevated privileges.
fn handle_generate_completions_command(args: &Args) -> anyhow::Result<Option<()>> {
    match args.command {
        Command::GenerateCompletions(ref completion_args) => {
            assert!(
                !executable_is_suid_or_sgid()?,
                "The executable should not be SUID or SGID when generating completions"
            );
            let mut cmd = match completion_args.command {
                ToplevelCommands::Mysqladm => Args::command(),
                #[cfg(feature = "mysql-admutils-compatibility")]
                ToplevelCommands::MysqlDbadm => mysql_dbadm::Args::command(),
                #[cfg(feature = "mysql-admutils-compatibility")]
                ToplevelCommands::MysqlUseradm => mysql_useradm::Args::command(),
            };

            let binary_name = cmd.get_bin_name().unwrap().to_owned();

            generate(
                completion_args.shell,
                &mut cmd,
                binary_name,
                &mut std::io::stdout(),
            );

            Ok(Some(()))
        }
        _ => Ok(None),
    }
}

/// Start a long-lived server using Tokio.
fn tokio_start_server(
    server_socket_path: Option<PathBuf>,
    config_path: Option<PathBuf>,
    verbosity: Verbosity,
    args: ServerArgs,
) -> anyhow::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("Failed to start Tokio runtime")?
        .block_on(async {
            server::command::handle_command(server_socket_path, config_path, verbosity, args).await
        })
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
                Command::GenerateCompletions(_) => unreachable!(),
            }
        })
}
