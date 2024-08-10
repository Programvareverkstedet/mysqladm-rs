#[macro_use]
extern crate prettytable;

use clap::Parser;

use std::path::PathBuf;

use std::os::unix::net::UnixStream as StdUnixStream;
use tokio::net::UnixStream as TokioUnixStream;

use crate::{
    core::{
        bootstrap::{bootstrap_server_connection_and_drop_privileges, drop_privs},
        protocol::create_client_to_server_message_stream,
    },
    server::command::ServerArgs,
};

#[cfg(feature = "mysql-admutils-compatibility")]
use crate::cli::mysql_admutils_compatibility::{mysql_dbadm, mysql_useradm};

mod server;

mod cli;
mod core;

#[cfg(feature = "tui")]
mod tui;

#[derive(Parser, Debug)]
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

    #[cfg(feature = "tui")]
    #[arg(short, long, alias = "tui", global = true)]
    interactive: bool,
}

// Database administration tool for non-admin users to manage their own MySQL databases and users.
//
// This tool allows you to manage users and databases in MySQL.
//
// You are only allowed to manage databases and users that are prefixed with
// either your username, or a group that you are a member of.
#[derive(Parser, Debug, Clone)]
#[command(version, about, disable_help_subcommand = true)]
enum Command {
    #[command(flatten)]
    Db(cli::database_command::DatabaseCommand),

    #[command(flatten)]
    User(cli::user_command::UserCommand),

    #[command(hide = true)]
    Server(server::command::ServerArgs),
}

// TODO: tag all functions that are run with elevated privileges with
//       comments emphasizing the need for caution.

fn main() -> anyhow::Result<()> {
    // TODO: find out if there are any security risks of running
    //       env_logger and clap with elevated privileges.

    env_logger::init();

    #[cfg(feature = "mysql-admutils-compatibility")]
    {
        let argv0 = std::env::args().next().and_then(|s| {
            PathBuf::from(s)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
        });

        match argv0.as_deref() {
            Some("mysql-dbadm") => return mysql_dbadm::main(),
            Some("mysql-useradm") => return mysql_useradm::main(),
            _ => { /* fall through */ }
        }
    }

    let args: Args = Args::parse();
    match args.command {
        Command::Server(ref command) => {
            drop_privs()?;
            tokio_start_server(args.server_socket_path, args.config, command.clone())?;
            return Ok(());
        }
        _ => { /* fall through */ }
    }

    let server_connection =
        bootstrap_server_connection_and_drop_privileges(args.server_socket_path, args.config)?;

    tokio_run_command(args.command, server_connection)?;

    Ok(())
}

fn tokio_start_server(
    server_socket_path: Option<PathBuf>,
    config_path: Option<PathBuf>,
    args: ServerArgs,
) -> anyhow::Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            server::command::handle_command(server_socket_path, config_path, args).await
        })
}

fn tokio_run_command(command: Command, server_connection: StdUnixStream) -> anyhow::Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let tokio_socket = TokioUnixStream::from_std(server_connection)?;
            let message_stream = create_client_to_server_message_stream(tokio_socket);
            match command {
                Command::User(user_args) => {
                    cli::user_command::handle_command(user_args, message_stream).await
                }
                Command::Db(db_args) => {
                    cli::database_command::handle_command(db_args, message_stream).await
                }
                Command::Server(_) => unreachable!(),
            }
        })
}
