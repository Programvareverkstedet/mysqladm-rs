use clap::Parser;
use futures_util::{SinkExt, StreamExt};

use crate::core::protocol::{
    ClientToServerMessageStream, Request, Response
};

use super::common::erroneous_server_response;

#[allow(clippy::enum_variant_names)]
#[derive(Parser, Debug, Clone)]
pub enum OtherCommand {
    /// Check if the tool is set up correctly, and the server is running.
    #[command()]
    Status(StatusArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct StatusArgs {
    /// Print the information as JSON
    #[arg(short, long)]
    json: bool,
}

pub async fn handle_command(
    command: OtherCommand,
    server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    match command {
        OtherCommand::Status(args) => status(args, server_connection).await,
    }
}

/// TODO: this should be moved all the way out to the main function, so that
///       we can teste the server connection before it fails to be established.
async fn status(
    args: StatusArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    if let Err(err) = server_connection.send(Request::Ping).await {
        server_connection.close().await.ok();
        anyhow::bail!(err);
    }

    match server_connection.next().await {
        Some(Ok(Response::Pong)) => (),
        response => return erroneous_server_response(response),
    };

    server_connection.send(Request::Exit).await?;

    if args.json {
    //     print_drop_users_output_status_json(&result);
    } else {
    //     print_drop_users_output_status(&result);
    }

    Ok(())
}
