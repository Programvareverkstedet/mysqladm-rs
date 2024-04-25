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
    config_overrides: core::config::ConfigOverrideArgs,

    #[cfg(feature = "tui")]
    #[arg(short, long, alias = "tui", global = true)]
    interactive: bool,
}

/// Database administration tool designed for non-admin users to manage their own MySQL databases and users.
/// Use `--help` for advanced usage.
///
/// This tool allows you to manage users and databases in MySQL that are prefixed with your username.
#[derive(Parser)]
#[command(version, about, disable_help_subcommand = true)]
enum Command {
    /// Create, drop or show/edit permissions for DATABASE(s),
    #[command(alias = "database")]
    Db(cli::database_command::DatabaseArgs),

    // Database(cli::database_command::DatabaseArgs),
    /// Create, drop, change password for, or show your USER(s),
    #[command(name = "user")]
    User(cli::user_command::UserArgs),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args: Args = Args::parse();
    let config = core::config::get_config(args.config_overrides)?;
    let connection = core::config::mysql_connection_from_config(config).await?;

    match args.command {
        Command::Db(database_args) => {
            cli::database_command::handle_command(database_args, connection).await
        }
        Command::User(user_args) => cli::user_command::handle_command(user_args, connection).await,
    }
}
