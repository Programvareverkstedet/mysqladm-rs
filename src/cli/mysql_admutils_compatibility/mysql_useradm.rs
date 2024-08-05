use clap::Parser;
use sqlx::MySqlConnection;

use crate::{
    cli::{
        mysql_admutils_compatibility::common::{filter_db_or_user_names, DbOrUser},
        user_command,
    },
    core::{
        common::get_current_unix_user,
        config::{get_config, mysql_connection_from_config, GlobalConfigArgs},
        user_operations::{
            create_database_user, delete_database_user, get_all_database_users_for_unix_user,
            password_is_set_for_database_user, set_password_for_database_user, user_exists,
        },
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
        Command::Passwd(args) => passwd(args, connection).await?,
        Command::Show(args) => show(args, connection).await?,
    }

    Ok(())
}

async fn passwd(args: PasswdArgs, mut connection: MySqlConnection) -> anyhow::Result<()> {
    let filtered_names = filter_db_or_user_names(args.name, DbOrUser::User)?;

    // NOTE: this gets doubly checked during the call to `set_password_for_database_user`.
    //       This is moving the check before asking the user for the password,
    //       to avoid having them figure out that the user does not exist after they
    //       have entered the password twice.
    let mut better_filtered_names = Vec::with_capacity(filtered_names.len());
    for name in filtered_names.into_iter() {
        if !user_exists(&name, &mut connection).await? {
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
        set_password_for_database_user(&name, &password, &mut connection).await?;
        println!("Password updated for user '{}'.", name);
    }

    Ok(())
}

async fn show(args: ShowArgs, mut connection: MySqlConnection) -> anyhow::Result<()> {
    let users = if args.name.is_empty() {
        let unix_user = get_current_unix_user()?;
        get_all_database_users_for_unix_user(&unix_user, &mut connection)
            .await?
            .into_iter()
            .map(|u| u.user)
            .collect()
    } else {
        filter_db_or_user_names(args.name, DbOrUser::User)?
    };

    for user in users {
        let password_is_set = password_is_set_for_database_user(&user, &mut connection).await?;

        match password_is_set {
            Some(true) => println!("User '{}': password set.", user),
            Some(false) => println!("User '{}': no password set.", user),
            None => {}
        }
    }

    Ok(())
}
