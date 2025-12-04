use clap::Parser;
use clap_complete::ArgValueCompleter;
use futures_util::SinkExt;
use tokio_stream::StreamExt;

use crate::{
    client::commands::erroneous_server_response,
    core::{
        completion::mysql_user_completer,
        protocol::{
            ClientToServerMessageStream, Request, Response, print_list_users_output_status,
            print_list_users_output_status_json,
        },
        types::MySQLUser,
    },
};

#[derive(Parser, Debug, Clone)]
pub struct ShowUserArgs {
    /// The MySQL user(s) to show
    #[arg(num_args = 0..)]
    #[cfg_attr(not(feature = "suid-sgid-mode"), arg(add = ArgValueCompleter::new(mysql_user_completer)))]
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

    let users = match server_connection.next().await {
        Some(Ok(Response::ListUsers(users))) => users,
        Some(Ok(Response::ListAllUsers(users))) => match users {
            Ok(users) => users
                .into_iter()
                .map(|user| (user.user.clone(), Ok(user)))
                .collect(),
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
        print_list_users_output_status_json(&users);
    } else {
        print_list_users_output_status(&users);
    }

    if args.fail && users.values().any(|result| result.is_err()) {
        std::process::exit(1);
    }

    Ok(())
}
