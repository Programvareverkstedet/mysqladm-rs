use clap::Parser;
use clap_complete::ArgValueCompleter;
use futures_util::SinkExt;
use tokio_stream::StreamExt;

use crate::{
    client::commands::erroneous_server_response,
    core::{
        completion::mysql_database_completer,
        protocol::{
            ClientToServerMessageStream, Request, Response, print_list_databases_output_status,
            print_list_databases_output_status_json,
        },
        types::MySQLDatabase,
    },
};

#[derive(Parser, Debug, Clone)]
pub struct ShowDbArgs {
    /// The MySQL database(s) to show
    #[arg(num_args = 0..)]
    #[cfg_attr(not(feature = "suid-sgid-mode"), arg(add = ArgValueCompleter::new(mysql_database_completer)))]
    name: Vec<MySQLDatabase>,

    /// Print the information as JSON
    #[arg(short, long)]
    json: bool,

    /// Return a non-zero exit code if any of the results were erroneous
    #[arg(short, long)]
    fail: bool,
}

pub async fn show_databases(
    args: ShowDbArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    let message = if args.name.is_empty() {
        Request::ListDatabases(None)
    } else {
        Request::ListDatabases(Some(args.name.to_owned()))
    };

    server_connection.send(message).await?;

    let databases = match server_connection.next().await {
        Some(Ok(Response::ListDatabases(databases))) => databases,
        Some(Ok(Response::ListAllDatabases(database_list))) => match database_list {
            Ok(list) => list
                .into_iter()
                .map(|db| (db.database.clone(), Ok(db)))
                .collect(),
            Err(err) => {
                server_connection.send(Request::Exit).await?;
                return Err(
                    anyhow::anyhow!(err.to_error_message()).context("Failed to list databases")
                );
            }
        },
        response => return erroneous_server_response(response),
    };

    server_connection.send(Request::Exit).await?;

    if args.json {
        print_list_databases_output_status_json(&databases);
    } else {
        print_list_databases_output_status(&databases);
    }

    if args.fail && databases.values().any(|res| res.is_err()) {
        std::process::exit(1);
    }

    Ok(())
}
