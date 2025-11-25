use clap::Parser;
use futures_util::SinkExt;
use tokio_stream::StreamExt;

use crate::{
    client::commands::erroneous_server_response,
    core::protocol::{
        ClientToServerMessageStream, MySQLDatabase, Request, Response,
        print_drop_databases_output_status, print_drop_databases_output_status_json,
    },
};

#[derive(Parser, Debug, Clone)]
pub struct DropDbArgs {
    /// The name of the database(s) to drop
    #[arg(num_args = 1..)]
    name: Vec<MySQLDatabase>,

    /// Print the information as JSON
    #[arg(short, long)]
    json: bool,
}

pub async fn drop_databases(
    args: DropDbArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    if args.name.is_empty() {
        anyhow::bail!("No database names provided");
    }

    let message = Request::DropDatabases(args.name.to_owned());
    server_connection.send(message).await?;

    let result = match server_connection.next().await {
        Some(Ok(Response::DropDatabases(result))) => result,
        response => return erroneous_server_response(response),
    };

    server_connection.send(Request::Exit).await?;

    if args.json {
        print_drop_databases_output_status_json(&result);
    } else {
        print_drop_databases_output_status(&result);
    };

    Ok(())
}
