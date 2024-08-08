#[macro_use]
extern crate prettytable;

use core::common::CommandStatus;
#[cfg(feature = "mysql-admutils-compatibility")]
use std::path::PathBuf;

#[cfg(feature = "mysql-admutils-compatibility")]
use crate::cli::mysql_admutils_compatibility::{mysql_dbadm, mysql_useradm};

use clap::Parser;

mod cli;
mod core;

#[cfg(feature = "tui")]
mod tui;

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Command,

    #[command(flatten)]
    config_overrides: core::config::GlobalConfigArgs,

    #[cfg(feature = "tui")]
    #[arg(short, long, alias = "tui", global = true)]
    interactive: bool,
}

/// Database administration tool for non-admin users to manage their own MySQL databases and users.
///
/// This tool allows you to manage users and databases in MySQL.
///
/// You are only allowed to manage databases and users that are prefixed with
/// either your username, or a group that you are a member of.
#[derive(Parser)]
#[command(version, about, disable_help_subcommand = true)]
enum Command {
    #[command(flatten)]
    Db(cli::database_command::DatabaseCommand),

    #[command(flatten)]
    User(cli::user_command::UserCommand),
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    #[cfg(feature = "mysql-admutils-compatibility")]
    {
        let argv0 = std::env::args().next().and_then(|s| {
            PathBuf::from(s)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
        });

        match argv0.as_deref() {
            Some("mysql-dbadm") => return mysql_dbadm::main().await,
            Some("mysql-useradm") => return mysql_useradm::main().await,
            _ => { /* fall through */ }
        }
    }

    let args: Args = Args::parse();
    let config = core::config::get_config(args.config_overrides)?;
    let connection = core::config::create_mysql_connection_from_config(config.mysql).await?;

    let result = match args.command {
        Command::Db(command) => cli::database_command::handle_command(command, connection).await,
        Command::User(user_args) => cli::user_command::handle_command(user_args, connection).await,
    };

    match result {
        Ok(CommandStatus::SuccessfullyModified) => {
            println!("Modifications committed successfully");
            Ok(())
        }
        Ok(CommandStatus::PartiallySuccessfullyModified) => {
            println!("Some modifications committed successfully");
            Ok(())
        }
        Ok(CommandStatus::NoModificationsNeeded) => {
            println!("No modifications made");
            Ok(())
        }
        Ok(CommandStatus::NoModificationsIntended) => {
            /* Don't report anything */
            Ok(())
        }
        Ok(CommandStatus::Cancelled) => {
            println!("Command cancelled successfully");
            Ok(())
        }
        Err(e) => Err(e),
    }
}
