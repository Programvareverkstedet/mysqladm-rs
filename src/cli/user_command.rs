use anyhow::Context;
use clap::Parser;
use dialoguer::{Confirm, Password};
use futures_util::{SinkExt, StreamExt};

use crate::core::protocol::{
    print_create_users_output_status, print_drop_users_output_status,
    print_lock_users_output_status, print_set_password_output_status,
    print_unlock_users_output_status, ClientToServerMessageStream, ListUsersError, Request,
    Response,
};

use super::common::erroneous_server_response;

#[derive(Parser, Debug, Clone)]
pub struct UserArgs {
    #[clap(subcommand)]
    subcmd: UserCommand,
}

#[allow(clippy::enum_variant_names)]
#[derive(Parser, Debug, Clone)]
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

    /// Print information about one or more users
    ///
    /// If no username is provided, all users you have access will be shown.
    #[command()]
    ShowUser(UserShowArgs),

    /// Lock account for one or more users
    #[command()]
    LockUser(UserLockArgs),

    /// Unlock account for one or more users
    #[command()]
    UnlockUser(UserUnlockArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct UserCreateArgs {
    #[arg(num_args = 1..)]
    username: Vec<String>,

    /// Do not ask for a password, leave it unset
    #[clap(long)]
    no_password: bool,
}

#[derive(Parser, Debug, Clone)]
pub struct UserDeleteArgs {
    #[arg(num_args = 1..)]
    username: Vec<String>,
}

#[derive(Parser, Debug, Clone)]
pub struct UserPasswdArgs {
    username: String,

    #[clap(short, long)]
    password_file: Option<String>,
}

#[derive(Parser, Debug, Clone)]
pub struct UserShowArgs {
    #[arg(num_args = 0..)]
    username: Vec<String>,

    #[clap(short, long)]
    json: bool,
}

#[derive(Parser, Debug, Clone)]
pub struct UserLockArgs {
    #[arg(num_args = 1..)]
    username: Vec<String>,
}

#[derive(Parser, Debug, Clone)]
pub struct UserUnlockArgs {
    #[arg(num_args = 1..)]
    username: Vec<String>,
}

pub async fn handle_command(
    command: UserCommand,
    server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    match command {
        UserCommand::CreateUser(args) => create_users(args, server_connection).await,
        UserCommand::DropUser(args) => drop_users(args, server_connection).await,
        UserCommand::PasswdUser(args) => passwd_user(args, server_connection).await,
        UserCommand::ShowUser(args) => show_users(args, server_connection).await,
        UserCommand::LockUser(args) => lock_users(args, server_connection).await,
        UserCommand::UnlockUser(args) => unlock_users(args, server_connection).await,
    }
}

async fn create_users(
    args: UserCreateArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    if args.username.is_empty() {
        anyhow::bail!("No usernames provided");
    }

    let message = Request::CreateUsers(args.username.clone());
    if let Err(err) = server_connection.send(message).await {
        server_connection.close().await.ok();
        anyhow::bail!(anyhow::Error::from(err).context("Failed to communicate with server"));
    }

    let result = match server_connection.next().await {
        Some(Ok(Response::CreateUsers(result))) => result,
        response => return erroneous_server_response(response),
    };

    print_create_users_output_status(&result);

    let successfully_created_users = result
        .iter()
        .filter_map(|(username, result)| result.as_ref().ok().map(|_| username))
        .collect::<Vec<_>>();

    for username in successfully_created_users {
        if !args.no_password
            && Confirm::new()
                .with_prompt(format!(
                    "Do you want to set a password for user '{}'?",
                    username
                ))
                .default(false)
                .interact()?
        {
            let password = read_password_from_stdin_with_double_check(username)?;
            let message = Request::PasswdUser(username.clone(), password);

            if let Err(err) = server_connection.send(message).await {
                server_connection.close().await.ok();
                anyhow::bail!(err);
            }

            match server_connection.next().await {
                Some(Ok(Response::PasswdUser(result))) => {
                    print_set_password_output_status(&result, username)
                }
                response => return erroneous_server_response(response),
            }

            println!();
        }
    }

    server_connection.send(Request::Exit).await?;

    Ok(())
}

async fn drop_users(
    args: UserDeleteArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    if args.username.is_empty() {
        anyhow::bail!("No usernames provided");
    }

    let message = Request::DropUsers(args.username.clone());

    if let Err(err) = server_connection.send(message).await {
        server_connection.close().await.ok();
        anyhow::bail!(err);
    }

    let result = match server_connection.next().await {
        Some(Ok(Response::DropUsers(result))) => result,
        response => return erroneous_server_response(response),
    };

    server_connection.send(Request::Exit).await?;

    print_drop_users_output_status(&result);

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

async fn passwd_user(
    args: UserPasswdArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    // TODO: create a "user" exists check" command
    let message = Request::ListUsers(Some(vec![args.username.clone()]));
    if let Err(err) = server_connection.send(message).await {
        server_connection.close().await.ok();
        anyhow::bail!(err);
    }
    let response = match server_connection.next().await {
        Some(Ok(Response::ListUsers(users))) => users,
        response => return erroneous_server_response(response),
    };
    match response
        .get(&args.username)
        .unwrap_or(&Err(ListUsersError::UserDoesNotExist))
    {
        Ok(_) => {}
        Err(err) => {
            server_connection.send(Request::Exit).await?;
            server_connection.close().await.ok();
            anyhow::bail!("{}", err.to_error_message(&args.username));
        }
    }

    let password = if let Some(password_file) = args.password_file {
        std::fs::read_to_string(password_file)
            .context("Failed to read password file")?
            .trim()
            .to_string()
    } else {
        read_password_from_stdin_with_double_check(&args.username)?
    };

    let message = Request::PasswdUser(args.username.clone(), password);

    if let Err(err) = server_connection.send(message).await {
        server_connection.close().await.ok();
        anyhow::bail!(err);
    }

    let result = match server_connection.next().await {
        Some(Ok(Response::PasswdUser(result))) => result,
        response => return erroneous_server_response(response),
    };

    server_connection.send(Request::Exit).await?;

    print_set_password_output_status(&result, &args.username);

    Ok(())
}

async fn show_users(
    args: UserShowArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    let message = if args.username.is_empty() {
        Request::ListUsers(None)
    } else {
        Request::ListUsers(Some(args.username.clone()))
    };

    if let Err(err) = server_connection.send(message).await {
        server_connection.close().await.ok();
        anyhow::bail!(err);
    }

    let users = match server_connection.next().await {
        Some(Ok(Response::ListUsers(users))) => users
            .into_iter()
            .filter_map(|(username, result)| match result {
                Ok(user) => Some(user),
                Err(err) => {
                    eprintln!("{}", err.to_error_message(&username));
                    eprintln!("Skipping...");
                    None
                }
            })
            .collect::<Vec<_>>(),
        Some(Ok(Response::ListAllUsers(users))) => match users {
            Ok(users) => users,
            Err(err) => {
                server_connection.send(Request::Exit).await?;
                return Err(
                    anyhow::anyhow!(err.to_error_message()).context("Failed to list all users")
                );
            }
        },
        response => return erroneous_server_response(response),
    };

    server_connection.send(Request::Exit).await?;

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&users).context("Failed to serialize users to JSON")?
        );
    } else if users.is_empty() {
        println!("No users to show.");
    } else {
        let mut table = prettytable::Table::new();
        table.add_row(row![
            "User",
            "Password is set",
            "Locked",
            "Databases where user has privileges"
        ]);
        for user in users {
            table.add_row(row![
                user.user,
                user.has_password,
                user.is_locked,
                user.databases.join("\n")
            ]);
        }
        table.printstd();
    }

    Ok(())
}

async fn lock_users(
    args: UserLockArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    if args.username.is_empty() {
        anyhow::bail!("No usernames provided");
    }

    let message = Request::LockUsers(args.username.clone());

    if let Err(err) = server_connection.send(message).await {
        server_connection.close().await.ok();
        anyhow::bail!(err);
    }

    let result = match server_connection.next().await {
        Some(Ok(Response::LockUsers(result))) => result,
        response => return erroneous_server_response(response),
    };

    server_connection.send(Request::Exit).await?;

    print_lock_users_output_status(&result);

    Ok(())
}

async fn unlock_users(
    args: UserUnlockArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    if args.username.is_empty() {
        anyhow::bail!("No usernames provided");
    }

    let message = Request::UnlockUsers(args.username.clone());

    if let Err(err) = server_connection.send(message).await {
        server_connection.close().await.ok();
        anyhow::bail!(err);
    }

    let result = match server_connection.next().await {
        Some(Ok(Response::UnlockUsers(result))) => result,
        response => return erroneous_server_response(response),
    };

    server_connection.send(Request::Exit).await?;

    print_unlock_users_output_status(&result);

    Ok(())
}
