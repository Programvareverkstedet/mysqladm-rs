use anyhow::Context;
use clap::Parser;
use futures_util::SinkExt;
use tokio_stream::StreamExt;

use crate::{
    client::commands::erroneous_server_response,
    core::{
        protocol::{ClientToServerMessageStream, Request, Response},
        types::MySQLUser,
    },
};

#[derive(Parser, Debug, Clone)]
pub struct ShowUserArgs {
    /// The MySQL user(s) to show
    #[arg(num_args = 0..)]
    username: Vec<MySQLUser>,

    /// Print the information as JSON
    #[arg(short, long)]
    json: bool,

    /// Return a non-zero exit code if any of the results were erroneous
    #[arg(short, long)]
    fail: bool,
}

pub async fn show_users(
    args: ShowUserArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    let message = if args.username.is_empty() {
        Request::ListUsers(None)
    } else {
        Request::ListUsers(Some(args.username.to_owned()))
    };

    if let Err(err) = server_connection.send(message).await {
        server_connection.close().await.ok();
        anyhow::bail!(err);
    }

    let mut contained_errors = false;
    let users = match server_connection.next().await {
        Some(Ok(Response::ListUsers(users))) => users
            .into_iter()
            .filter_map(|(username, result)| match result {
                Ok(user) => Some(user),
                Err(err) => {
                    contained_errors = true;
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

    if args.fail && contained_errors {
        std::process::exit(1);
    }

    Ok(())
}
