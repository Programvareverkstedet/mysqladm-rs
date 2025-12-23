use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use clap_complete::ArgValueCompleter;
use dialoguer::Password;
use futures_util::SinkExt;
use tokio_stream::StreamExt;

use crate::{
    client::commands::{erroneous_server_response, print_authorization_owner_hint},
    core::{
        completion::mysql_user_completer,
        protocol::{
            ClientToServerMessageStream, ListUsersError, Request, Response, SetPasswordError,
            SetUserPasswordRequest, print_set_password_output_status,
            request_validation::ValidationError,
        },
        types::MySQLUser,
    },
};

#[derive(Parser, Debug, Clone)]
pub struct PasswdUserArgs {
    /// The `MySQL` user whose password is to be changed
    #[cfg_attr(not(feature = "suid-sgid-mode"), arg(add = ArgValueCompleter::new(mysql_user_completer)))]
    #[arg(value_name = "USER_NAME")]
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

    /// Set the password to expire on the given date (YYYY-MM-DD)
    #[arg(short, long, value_name = "DATE", conflicts_with = "no-expire")]
    expire_on: Option<chrono::NaiveDate>,

    /// Set the password to never expire
    #[arg(short, long, conflicts_with = "expire_on")]
    no_expire: bool,

    /// Clear the password for the user instead of setting a new one
    #[arg(short, long, conflicts_with_all = &["password_file", "stdin", "expire_on", "no-expire"])]
    clear: bool,
}

pub fn interactive_password_dialogue_with_double_check(username: &MySQLUser) -> anyhow::Result<String> {
    Password::new()
        .with_prompt(format!("New MySQL password for user '{username}'"))
        .with_confirmation(
            format!("Retype new MySQL password for user '{username}'"),
            "Passwords do not match",
        )
        .interact()
        .map_err(Into::into)
}

pub fn interactive_password_expiry_dialogue(username: &MySQLUser) -> anyhow::Result<Option<chrono::NaiveDate>> {
    let input = dialoguer::Input::<String>::new()
        .with_prompt(format!(
            "Enter the password expiry date for user '{username}' (YYYY-MM-DD)"
        ))
        .allow_empty(true)
        .validate_with(|input: &String| {
            chrono::NaiveDate::parse_from_str(input, "%Y-%m-%d")
                .map(|_| ())
                .map_err(|_| "Invalid date format. Please use YYYY-MM-DD".to_string())
        })
        .interact_text()?;

    if input.trim().is_empty() {
        return Ok(None);
    }

    let date = chrono::NaiveDate::parse_from_str(&input, "%Y-%m-%d")
        .map_err(|e| anyhow::anyhow!("Failed to parse date: {}", e))?;

    Ok(Some(date))
}

pub async fn passwd_user(
    args: PasswdUserArgs,
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

    let password: Option<String> = if let Some(password_file) = args.password_file {
        Some(
            std::fs::read_to_string(password_file)
                .context("Failed to read password file")?
                .trim()
                .to_string(),
        )
    } else if args.stdin {
        let mut buffer = String::new();
        std::io::stdin()
            .read_line(&mut buffer)
            .context("Failed to read password from stdin")?;
        Some(buffer.trim().to_string())
    } else if args.clear {
        None
    } else {
        Some(interactive_password_dialogue_with_double_check(&args.username)?)
    };

    let expiry_date = if args.no_expire {
        None
    } else if let Some(date) = args.expire_on {
        Some(date)
    } else {
        interactive_password_expiry_dialogue(&args.username)?
    };

    let message = Request::PasswdUser(SetUserPasswordRequest {
        user: args.username.clone(),
        new_password: password,
        expiry: expiry_date,
    });

    if let Err(err) = server_connection.send(message).await {
        server_connection.close().await.ok();
        anyhow::bail!(err);
    }

    let result = match server_connection.next().await {
        Some(Ok(Response::SetUserPassword(result))) => result,
        response => return erroneous_server_response(response),
    };

    print_set_password_output_status(&result, &args.username);

    if matches!(
        result,
        Err(SetPasswordError::ValidationError(
            ValidationError::AuthorizationError(_)
        ))
    ) {
        print_authorization_owner_hint(&mut server_connection).await?;
    }

    server_connection.send(Request::Exit).await?;

    if result.is_err() {
        std::process::exit(1);
    }

    Ok(())
}
