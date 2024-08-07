use std::collections::BTreeMap;
use std::vec;

use anyhow::Context;
use clap::Parser;
use dialoguer::{Confirm, Password};
use prettytable::Table;
use serde_json::json;
use sqlx::{Connection, MySqlConnection};

use crate::core::{
    common::{close_database_connection, get_current_unix_user},
    database_operations::*,
    user_operations::*,
};

#[derive(Parser)]
pub struct UserArgs {
    #[clap(subcommand)]
    subcmd: UserCommand,
}

#[derive(Parser)]
pub enum UserCommand {
    /// Create one or more users
    #[command()]
    CreateUser(UserCreateArgs),

    /// Delete one or more users
    #[command()]
    DropUser(UserDeleteArgs),

    /// Change the MySQL password for a user
    #[command()]
    PasswdUser(UserPasswdArgs),

    /// Give information about one or more users
    ///
    /// If no username is provided, all users you have access will be shown.
    #[command()]
    ShowUser(UserShowArgs),
}

#[derive(Parser)]
pub struct UserCreateArgs {
    #[arg(num_args = 1..)]
    username: Vec<String>,

    /// Do not ask for a password, leave it unset
    #[clap(long)]
    no_password: bool,
}

#[derive(Parser)]
pub struct UserDeleteArgs {
    #[arg(num_args = 1..)]
    username: Vec<String>,
}

#[derive(Parser)]
pub struct UserPasswdArgs {
    username: String,

    #[clap(short, long)]
    password_file: Option<String>,
}

#[derive(Parser)]
pub struct UserShowArgs {
    #[arg(num_args = 0..)]
    username: Vec<String>,

    #[clap(short, long)]
    json: bool,
}

pub async fn handle_command(
    command: UserCommand,
    mut connection: MySqlConnection,
) -> anyhow::Result<()> {
    let result = connection
        .transaction(|txn| {
            Box::pin(async move {
                match command {
                    UserCommand::CreateUser(args) => create_users(args, txn).await,
                    UserCommand::DropUser(args) => drop_users(args, txn).await,
                    UserCommand::PasswdUser(args) => change_password_for_user(args, txn).await,
                    UserCommand::ShowUser(args) => show_users(args, txn).await,
                }
            })
        })
        .await;

    close_database_connection(connection).await;

    result
}

async fn create_users(
    args: UserCreateArgs,
    connection: &mut MySqlConnection,
) -> anyhow::Result<()> {
    if args.username.is_empty() {
        anyhow::bail!("No usernames provided");
    }

    for username in args.username {
        if let Err(e) = create_database_user(&username, connection).await {
            eprintln!("{}", e);
            eprintln!("Skipping...\n");
            continue;
        } else {
            println!("User '{}' created.", username);
        }

        if !args.no_password
            && Confirm::new()
                .with_prompt(format!(
                    "Do you want to set a password for user '{}'?",
                    username
                ))
                .interact()?
        {
            change_password_for_user(
                UserPasswdArgs {
                    username,
                    password_file: None,
                },
                connection,
            )
            .await?;
        }
        println!();
    }
    Ok(())
}

async fn drop_users(args: UserDeleteArgs, connection: &mut MySqlConnection) -> anyhow::Result<()> {
    if args.username.is_empty() {
        anyhow::bail!("No usernames provided");
    }

    for username in args.username {
        if let Err(e) = delete_database_user(&username, connection).await {
            eprintln!("{}", e);
            eprintln!("Skipping...");
        }
    }
    Ok(())
}

pub fn read_password_from_stdin_with_double_check(username: &str) -> anyhow::Result<String> {
    Password::new()
        .with_prompt(format!("New MySQL password for user '{}'", username))
        .with_confirmation(
            format!("Retype new MySQL password for user '{}'", username),
            "Passwords do not match",
        )
        .interact()
        .map_err(Into::into)
}

async fn change_password_for_user(
    args: UserPasswdArgs,
    connection: &mut MySqlConnection,
) -> anyhow::Result<()> {
    // NOTE: although this also is checked in `set_password_for_database_user`, we check it here
    //       to provide a more natural order of error messages.
    let unix_user = get_current_unix_user()?;
    validate_user_name(&args.username, &unix_user)?;

    let password = if let Some(password_file) = args.password_file {
        std::fs::read_to_string(password_file)
            .context("Failed to read password file")?
            .trim()
            .to_string()
    } else {
        read_password_from_stdin_with_double_check(&args.username)?
    };

    set_password_for_database_user(&args.username, &password, connection).await?;

    Ok(())
}

async fn show_users(args: UserShowArgs, connection: &mut MySqlConnection) -> anyhow::Result<()> {
    let unix_user = get_current_unix_user()?;

    let users = if args.username.is_empty() {
        get_all_database_users_for_unix_user(&unix_user, connection).await?
    } else {
        let mut result = vec![];
        for username in args.username {
            if let Err(e) = validate_user_name(&username, &unix_user) {
                eprintln!("{}", e);
                eprintln!("Skipping...");
                continue;
            }

            let user = get_database_user_for_user(&username, connection).await?;
            if let Some(user) = user {
                result.push(user);
            } else {
                eprintln!("User not found: {}", username);
            }
        }
        result
    };

    let mut user_databases: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for user in users.iter() {
        user_databases.insert(
            user.user.clone(),
            get_databases_where_user_has_privileges(&user.user, connection).await?,
        );
    }

    if args.json {
        let users_json = users
            .into_iter()
            .map(|user| {
                json!({
                    "user": user.user,
                    "has_password": user.has_password,
                    "databases": user_databases.get(&user.user).unwrap_or(&vec![]),
                })
            })
            .collect::<serde_json::Value>();
        println!(
            "{}",
            serde_json::to_string_pretty(&users_json)
                .context("Failed to serialize users to JSON")?
        );
    } else if users.is_empty() {
        println!("No users found.");
    } else {
        let mut table = Table::new();
        table.add_row(row![
            "User",
            "Password is set",
            "Databases where user has privileges"
        ]);
        for user in users {
            table.add_row(row![
                user.user,
                user.has_password,
                user_databases.get(&user.user).unwrap_or(&vec![]).join("\n")
            ]);
        }
        table.printstd();
    }

    Ok(())
}
