use std::vec;

use anyhow::Context;
use clap::Parser;
use sqlx::MySqlConnection;

use crate::core::user_operations::validate_ownership_of_user_name;

#[derive(Parser)]
pub struct UserArgs {
    #[clap(subcommand)]
    subcmd: UserCommand,
}

#[derive(Parser)]
enum UserCommand {
    /// Create the USER(s).
    #[command(alias = "add", alias = "c")]
    Create(UserCreateArgs),

    /// Delete the USER(s).
    #[command(alias = "remove", alias = "delete", alias = "rm", alias = "d")]
    Drop(UserDeleteArgs),

    /// Change the MySQL password for the USER.
    #[command(alias = "password", alias = "p")]
    Passwd(UserPasswdArgs),

    /// Give information about the USER(s), or if no USER is given, all USERs you have access to.
    #[command(alias = "list", alias = "ls", alias = "s")]
    Show(UserShowArgs),
}

#[derive(Parser)]
struct UserCreateArgs {
    #[arg(num_args = 1..)]
    username: Vec<String>,
}

#[derive(Parser)]
struct UserDeleteArgs {
    #[arg(num_args = 1..)]
    username: Vec<String>,
}

#[derive(Parser)]
struct UserPasswdArgs {
    username: String,

    #[clap(short, long)]
    password_file: Option<String>,
}

#[derive(Parser)]
struct UserShowArgs {
    #[arg(num_args = 0..)]
    username: Vec<String>,
}

pub async fn handle_command(args: UserArgs, conn: MySqlConnection) -> anyhow::Result<()> {
    match args.subcmd {
        UserCommand::Create(args) => create_users(args, conn).await,
        UserCommand::Drop(args) => drop_users(args, conn).await,
        UserCommand::Passwd(args) => change_password_for_user(args, conn).await,
        UserCommand::Show(args) => show_users(args, conn).await,
    }
}

// TODO: provide a better error message when the user already exists
async fn create_users(args: UserCreateArgs, mut conn: MySqlConnection) -> anyhow::Result<()> {
    if args.username.is_empty() {
        anyhow::bail!("No usernames provided");
    }

    for username in args.username {
        if let Err(e) =
            crate::core::user_operations::create_database_user(&username, &mut conn).await
        {
            eprintln!("{}", e);
            eprintln!("Skipping...");
        }
    }
    Ok(())
}

// TODO: provide a better error message when the user does not exist
async fn drop_users(args: UserDeleteArgs, mut conn: MySqlConnection) -> anyhow::Result<()> {
    if args.username.is_empty() {
        anyhow::bail!("No usernames provided");
    }

    for username in args.username {
        if let Err(e) =
            crate::core::user_operations::delete_database_user(&username, &mut conn).await
        {
            eprintln!("{}", e);
            eprintln!("Skipping...");
        }
    }
    Ok(())
}

async fn change_password_for_user(
    args: UserPasswdArgs,
    mut conn: MySqlConnection,
) -> anyhow::Result<()> {
    // NOTE: although this also is checked in `set_password_for_database_user`, we check it here
    //       to provide a more natural order of error messages.
    let unix_user = crate::core::common::get_current_unix_user()?;
    validate_ownership_of_user_name(&args.username, &unix_user)?;

    let password = if let Some(password_file) = args.password_file {
        std::fs::read_to_string(password_file)
            .context("Failed to read password file")?
            .trim()
            .to_string()
    } else {
        let pass1 = rpassword::prompt_password("Enter new password: ")
            .context("Failed to read password")?;

        let pass2 = rpassword::prompt_password("Re-enter new password: ")
            .context("Failed to read password")?;

        if pass1 != pass2 {
            anyhow::bail!("Passwords do not match");
        }

        pass1
    };

    crate::core::user_operations::set_password_for_database_user(
        &args.username,
        &password,
        &mut conn,
    )
    .await?;

    Ok(())
}

async fn show_users(args: UserShowArgs, mut conn: MySqlConnection) -> anyhow::Result<()> {
    let user = crate::core::common::get_current_unix_user()?;

    let users = if args.username.is_empty() {
        crate::core::user_operations::get_all_database_users_for_user(&user, &mut conn).await?
    } else {
        let mut result = vec![];
        for username in args.username {
            if let Err(e) = validate_ownership_of_user_name(&username, &user) {
                eprintln!("{}", e);
                eprintln!("Skipping...");
                continue;
            }

            let user =
                crate::core::user_operations::get_database_user_for_user(&username, &mut conn)
                    .await?;
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
