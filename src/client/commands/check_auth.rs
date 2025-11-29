use crate::{
    client::commands::erroneous_server_response,
    core::{
        protocol::{
            ClientToServerMessageStream, Request, Response,
            print_check_authorization_output_status, print_check_authorization_output_status_json,
        },
        types::DbOrUser,
    },
};
use clap::Parser;
use futures_util::SinkExt;
use tokio_stream::StreamExt;

#[derive(Parser, Debug, Clone)]
pub struct CheckAuthArgs {
    /// The name of the database(s) or user(s) to check authorization for
    #[arg(num_args = 1..)]
    name: Vec<String>,

    /// Assume the names are users, not databases
    #[arg(short, long)]
    users: bool,

    /// Print the information as JSON
    #[arg(short, long)]
    json: bool,
}

pub async fn check_authorization(
    args: CheckAuthArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    if args.name.is_empty() {
        anyhow::bail!("No database/user names provided");
    }

    let payload = args
        .name
        .into_iter()
        .map(|name| {
            if args.users {
                DbOrUser::User(name.into())
            } else {
                DbOrUser::Database(name.into())
            }
        })
        .collect::<Vec<_>>();

    let message = Request::CheckAuthorization(payload);
    server_connection.send(message).await?;

    let result = match server_connection.next().await {
        Some(Ok(Response::CheckAuthorization(response))) => response,
        response => return erroneous_server_response(response),
    };

    server_connection.send(Request::Exit).await?;

    if args.json {
        print_check_authorization_output_status_json(&result);
    } else {
        print_check_authorization_output_status(&result);
    }

    Ok(())
}
