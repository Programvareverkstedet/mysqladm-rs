use clap::Parser;
use futures_util::SinkExt;
use tokio_stream::StreamExt;

use crate::{
    client::commands::erroneous_server_response,
    core::{
        protocol::{
            ClientToServerMessageStream, Request, Response, print_lock_users_output_status,
            print_lock_users_output_status_json,
        },
        types::MySQLUser,
    },
};

#[derive(Parser, Debug, Clone)]
pub struct LockUserArgs {
    #[arg(num_args = 1..)]
    username: Vec<MySQLUser>,

    /// Print the information as JSON
    #[arg(short, long)]
    json: bool,
}

pub async fn lock_users(
    args: LockUserArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    if args.username.is_empty() {
        anyhow::bail!("No usernames provided");
    }

    let message = Request::LockUsers(args.username.to_owned());

    if let Err(err) = server_connection.send(message).await {
        server_connection.close().await.ok();
        anyhow::bail!(err);
    }

    let result = match server_connection.next().await {
        Some(Ok(Response::LockUsers(result))) => result,
        response => return erroneous_server_response(response),
    };

    server_connection.send(Request::Exit).await?;

    if args.json {
        print_lock_users_output_status_json(&result);
    } else {
        print_lock_users_output_status(&result);
    }

    Ok(())
}
