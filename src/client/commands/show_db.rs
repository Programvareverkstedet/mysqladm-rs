use clap::Parser;
use clap_complete::ArgValueCompleter;
use futures_util::SinkExt;
use prettytable::{Cell, Row, Table};
use tokio_stream::StreamExt;

use crate::{
    client::commands::erroneous_server_response,
    core::{
        completion::mysql_database_completer,
        protocol::{ClientToServerMessageStream, Request, Response},
        types::MySQLDatabase,
    },
};

#[derive(Parser, Debug, Clone)]
pub struct ShowDbArgs {
    /// The MySQL database(s) to show
    #[arg(num_args = 0.., add = ArgValueCompleter::new(mysql_database_completer))]
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

    // TODO: collect errors for json output.

    let mut contained_errors = false;
    let database_list = match server_connection.next().await {
        Some(Ok(Response::ListDatabases(databases))) => databases
            .into_iter()
            .filter_map(|(database_name, result)| match result {
                Ok(database_row) => Some(database_row),
                Err(err) => {
                    contained_errors = true;
                    eprintln!("{}", err.to_error_message(&database_name));
                    eprintln!("Skipping...");
                    println!();
                    None
                }
            })
            .collect::<Vec<_>>(),
        Some(Ok(Response::ListAllDatabases(database_list))) => match database_list {
            Ok(list) => list,
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
        println!("{}", serde_json::to_string_pretty(&database_list)?);
    } else if database_list.is_empty() {
        println!("No databases to show.");
    } else {
        let mut table = Table::new();
        table.add_row(Row::new(vec![Cell::new("Database")]));
        for db in database_list {
            table.add_row(row![db.database]);
        }
        table.printstd();
    }

    if args.fail && contained_errors {
        std::process::exit(1);
    }

    Ok(())
}
