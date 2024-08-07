use std::vec;

use anyhow::Context;
use clap::Parser;
use dialoguer::{Confirm, Password};
use sqlx::MySqlConnection;

use crate::core::{common::close_database_connection, user_operations::validate_user_name};

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
}

pub async fn handle_command(command: UserCommand, mut conn: MySqlConnection) -> anyhow::Result<()> {
    let result = match command {
        UserCommand::CreateUser(args) => create_users(args, &mut conn).await,
        UserCommand::DropUser(args) => drop_users(args, &mut conn).await,
        UserCommand::PasswdUser(args) => change_password_for_user(args, &mut conn).await,
        UserCommand::ShowUser(args) => show_users(args, &mut conn).await,
    };

    close_database_connection(conn).await;

    result
}

async fn create_users(args: UserCreateArgs, conn: &mut MySqlConnection) -> anyhow::Result<()> {
    if args.username.is_empty() {
        anyhow::bail!("No usernames provided");
    }

    for username in args.username {
        if let Err(e) = crate::core::user_operations::create_database_user(&username, conn).await {
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
                conn,
            )
            .await?;
        }
        println!("");
    }
    Ok(())
}

async fn drop_users(args: UserDeleteArgs, conn: &mut MySqlConnection) -> anyhow::Result<()> {
    if args.username.is_empty() {
        anyhow::bail!("No usernames provided");
    }

    for username in args.username {
        if let Err(e) = crate::core::user_operations::delete_database_user(&username, conn).await {
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
    conn: &mut MySqlConnection,
) -> anyhow::Result<()> {
    // NOTE: although this also is checked in `set_password_for_database_user`, we check it here
    //       to provide a more natural order of error messages.
    let unix_user = crate::core::common::get_current_unix_user()?;
    validate_user_name(&args.username, &unix_user)?;

    let password = if let Some(password_file) = args.password_file {
        std::fs::read_to_string(password_file)
            .context("Failed to read password file")?
            .trim()
            .to_string()
    } else {
        read_password_from_stdin_with_double_check(&args.username)?
    };

    crate::core::user_operations::set_password_for_database_user(&args.username, &password, conn)
        .await?;

    Ok(())
}

async fn show_users(args: UserShowArgs, conn: &mut MySqlConnection) -> anyhow::Result<()> {
    let unix_user = crate::core::common::get_current_unix_user()?;

    let users = if args.username.is_empty() {
        crate::core::user_operations::get_all_database_users_for_unix_user(&unix_user, conn).await?
    } else {
        let mut result = vec![];
        for username in args.username {
            if let Err(e) = validate_user_name(&username, &unix_user) {
                eprintln!("{}", e);
                eprintln!("Skipping...");
                continue;
            }

            let user =
                crate::core::user_operations::get_database_user_for_user(&username, conn).await?;
            if let Some(user) = user {
                result.push(user);
            } else {
                eprintln!("User not found: {}", username);
            }
        }
        result
    };

    for user in users {
        println!(
            "User '{}': {}",
            &user.user,
            if !(user.authentication_string.is_empty() && user.password.is_empty()) {
                "password set."
            } else {
                "no password set."
            }
        );
    }

    Ok(())
}
