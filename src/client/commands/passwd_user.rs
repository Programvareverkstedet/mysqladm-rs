use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use clap_complete::ArgValueCompleter;
use dialoguer::Password;
use futures_util::SinkExt;
use tokio_stream::StreamExt;

use crate::{
    client::commands::erroneous_server_response,
    core::{
        completion::mysql_user_completer,
        protocol::{
            ClientToServerMessageStream, ListUsersError, Request, Response,
            print_set_password_output_status,
        },
        types::MySQLUser,
    },
};

#[derive(Parser, Debug, Clone)]
pub struct PasswdUserArgs {
    /// The MySQL user whose password is to be changed
    #[arg(add = ArgValueCompleter::new(mysql_user_completer))]
    username: MySQLUser,

    /// Read the new password from a file instead of prompting for it
    #[clap(short, long, value_name = "PATH", conflicts_with = "stdin")]
    password_file: Option<PathBuf>,

    /// Read the new password from stdin instead of prompting for it
    #[clap(short = 'i', long, conflicts_with = "password_file")]
    stdin: bool,

    /// Print the information as JSON
    #[arg(short, long)]
    json: bool,
}

pub fn read_password_from_stdin_with_double_check(username: &MySQLUser) -> anyhow::Result<String> {
    Password::new()
        .with_prompt(format!("New MySQL password for user '{}'", username))
        .with_confirmation(
            format!("Retype new MySQL password for user '{}'", username),
            "Passwords do not match",
        )
        .interact()
        .map_err(Into::into)
}

pub async fn passwd_user(
    args: PasswdUserArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    // TODO: create a "user" exists check" command
    let message = Request::ListUsers(Some(vec![args.username.to_owned()]));
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
    } else if args.stdin {
        let mut buffer = String::new();
        std::io::stdin()
            .read_line(&mut buffer)
            .context("Failed to read password from stdin")?;
        buffer.trim().to_string()
    } else {
        read_password_from_stdin_with_double_check(&args.username)?
    };

    let message = Request::PasswdUser((args.username.to_owned(), password));

    if let Err(err) = server_connection.send(message).await {
        server_connection.close().await.ok();
        anyhow::bail!(err);
    }

    let result = match server_connection.next().await {
        Some(Ok(Response::SetUserPassword(result))) => result,
        response => return erroneous_server_response(response),
    };

    server_connection.send(Request::Exit).await?;

    print_set_password_output_status(&result, &args.username);

    Ok(())
}
