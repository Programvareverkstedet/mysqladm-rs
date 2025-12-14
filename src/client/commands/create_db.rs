use clap::Parser;
use futures_util::SinkExt;
use tokio_stream::StreamExt;

use crate::{
    client::commands::erroneous_server_response,
    core::{
        protocol::{
            ClientToServerMessageStream, Request, Response, print_create_databases_output_status,
            print_create_databases_output_status_json,
        },
        types::MySQLDatabase,
    },
};

#[derive(Parser, Debug, Clone)]
pub struct CreateDbArgs {
    /// The MySQL database(s) to create
    #[arg(num_args = 1.., value_name = "DB_NAME")]
    name: Vec<MySQLDatabase>,

    /// Print the information as JSON
    #[arg(short, long)]
    json: bool,
}

pub async fn create_databases(
    args: CreateDbArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    if args.name.is_empty() {
        anyhow::bail!("No database names provided");
    }

    let message = Request::CreateDatabases(args.name.to_owned());
    server_connection.send(message).await?;

    let result = match server_connection.next().await {
        Some(Ok(Response::CreateDatabases(result))) => result,
        response => return erroneous_server_response(response),
    };

    server_connection.send(Request::Exit).await?;

    if args.json {
        print_create_databases_output_status_json(&result);
    } else {
        print_create_databases_output_status(&result);
    }

    Ok(())
}
