#[macro_use]
extern crate prettytable;

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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args: Args = Args::parse();
    let config = core::config::get_config(args.config_overrides)?;
    let connection = core::config::mysql_connection_from_config(config).await?;

    match args.command {
        Command::Db(command) => cli::database_command::handle_command(command, connection).await,
        Command::User(user_args) => cli::user_command::handle_command(user_args, connection).await,
    }
}
