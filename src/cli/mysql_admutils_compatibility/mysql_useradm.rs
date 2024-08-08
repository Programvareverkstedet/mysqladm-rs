use clap::Parser;
use sqlx::MySqlConnection;

use crate::{
    cli::{mysql_admutils_compatibility::common::filter_db_or_user_names, user_command},
    core::{
        common::{close_database_connection, get_current_unix_user, DbOrUser},
        config::{get_config, mysql_connection_from_config, GlobalConfigArgs},
        user_operations::*,
    },
};

#[derive(Parser)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    config_overrides: GlobalConfigArgs,
}

/// Create, delete or change password for the USER(s),
/// as determined by the COMMAND.
///
/// This is a compatibility layer for the mysql-useradm command.
/// Please consider using the newer mysqladm command instead.
#[derive(Parser)]
#[command(version, about, disable_help_subcommand = true, verbatim_doc_comment)]
pub enum Command {
    /// create the USER(s).
    Create(CreateArgs),

    /// delete the USER(s).
    Delete(DeleteArgs),

    /// change the MySQL password for the USER(s).
    Passwd(PasswdArgs),

    /// give information about the USERS(s), or, if
    /// none are given, all the users you have.
    Show(ShowArgs),
}

#[derive(Parser)]
pub struct CreateArgs {
    /// The name of the USER(s) to create.
    #[arg(num_args = 1..)]
    name: Vec<String>,
}

#[derive(Parser)]
pub struct DeleteArgs {
    /// The name of the USER(s) to delete.
    #[arg(num_args = 1..)]
    name: Vec<String>,
}

#[derive(Parser)]
pub struct PasswdArgs {
    /// The name of the USER(s) to change the password for.
    #[arg(num_args = 1..)]
    name: Vec<String>,
}

#[derive(Parser)]
pub struct ShowArgs {
    /// The name of the USER(s) to show.
    #[arg(num_args = 0..)]
    name: Vec<String>,
}

pub async fn main() -> anyhow::Result<()> {
    let args: Args = Args::parse();

    let command = match args.command {
        Some(command) => command,
        None => {
            println!(
                "Try `{} --help' for more information.",
                std::env::args()
                    .next()
                    .unwrap_or("mysql-useradm".to_string())
            );
            return Ok(());
        }
    };

    let config = get_config(args.config_overrides)?;
    let mut connection = mysql_connection_from_config(config).await?;

    match command {
        Command::Create(args) => {
            let filtered_names = filter_db_or_user_names(args.name, DbOrUser::User)?;
            for name in filtered_names {
                create_database_user(&name, &mut connection).await?;
            }
        }
        Command::Delete(args) => {
            let filtered_names = filter_db_or_user_names(args.name, DbOrUser::User)?;
            for name in filtered_names {
                delete_database_user(&name, &mut connection).await?;
            }
        }
        Command::Passwd(args) => passwd(args, &mut connection).await?,
        Command::Show(args) => show(args, &mut connection).await?,
    }

    close_database_connection(connection).await;

    Ok(())
}

async fn passwd(args: PasswdArgs, connection: &mut MySqlConnection) -> anyhow::Result<()> {
    let filtered_names = filter_db_or_user_names(args.name, DbOrUser::User)?;

    // NOTE: this gets doubly checked during the call to `set_password_for_database_user`.
    //       This is moving the check before asking the user for the password,
    //       to avoid having them figure out that the user does not exist after they
    //       have entered the password twice.
    let mut better_filtered_names = Vec::with_capacity(filtered_names.len());
    for name in filtered_names.into_iter() {
        if !user_exists(&name, connection).await? {
            println!(
                "{}: User '{}' does not exist. You must create it first.",
                std::env::args()
                    .next()
                    .unwrap_or("mysql-useradm".to_string()),
                name,
            );
        } else {
            better_filtered_names.push(name);
        }
    }

    for name in better_filtered_names {
        let password = user_command::read_password_from_stdin_with_double_check(&name)?;
        set_password_for_database_user(&name, &password, connection).await?;
        println!("Password updated for user '{}'.", name);
    }

    Ok(())
}

async fn show(args: ShowArgs, connection: &mut MySqlConnection) -> anyhow::Result<()> {
    let users = if args.name.is_empty() {
        let unix_user = get_current_unix_user()?;
        get_all_database_users_for_unix_user(&unix_user, connection).await?
    } else {
        let filtered_usernames = filter_db_or_user_names(args.name, DbOrUser::User)?;
        let mut result = Vec::with_capacity(filtered_usernames.len());
        for username in filtered_usernames.iter() {
            // TODO: fetch all users in one query
            if let Some(user) = get_database_user_for_user(username, connection).await? {
                result.push(user)
            }
        }
        result
    };

    for user in users {
        if user.has_password {
            println!("User '{}': password set.", user.user);
        } else {
            println!("User '{}': no password set.", user.user);
        }
    }

    Ok(())
}
