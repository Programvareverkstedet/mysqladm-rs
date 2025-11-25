use clap::Parser;
use dialoguer::Confirm;
use futures_util::SinkExt;
use tokio_stream::StreamExt;

use crate::{
    client::commands::{erroneous_server_response, read_password_from_stdin_with_double_check},
    core::{
        protocol::{
            ClientToServerMessageStream, Request, Response, print_create_users_output_status,
            print_create_users_output_status_json, print_set_password_output_status,
        },
        types::MySQLUser,
    },
};

#[derive(Parser, Debug, Clone)]
pub struct CreateUserArgs {
    #[arg(num_args = 1..)]
    username: Vec<MySQLUser>,

    /// Do not ask for a password, leave it unset
    #[clap(long)]
    no_password: bool,

    /// Print the information as JSON
    ///
    /// Note that this implies `--no-password`, since the command will become non-interactive.
    #[arg(short, long)]
    json: bool,
}

pub async fn create_users(
    args: CreateUserArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    if args.username.is_empty() {
        anyhow::bail!("No usernames provided");
    }

    let message = Request::CreateUsers(args.username.to_owned());
    if let Err(err) = server_connection.send(message).await {
        server_connection.close().await.ok();
        anyhow::bail!(anyhow::Error::from(err).context("Failed to communicate with server"));
    }

    let result = match server_connection.next().await {
        Some(Ok(Response::CreateUsers(result))) => result,
        response => return erroneous_server_response(response),
    };

    if args.json {
        print_create_users_output_status_json(&result);
    } else {
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
                let message = Request::PasswdUser(username.to_owned(), password);

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
    }

    server_connection.send(Request::Exit).await?;

    Ok(())
}
