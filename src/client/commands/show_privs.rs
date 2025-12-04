use clap::Parser;
use clap_complete::ArgValueCompleter;
use futures_util::SinkExt;
use itertools::Itertools;
use tokio_stream::StreamExt;

use crate::{
    client::commands::erroneous_server_response,
    core::{
        completion::mysql_database_completer,
        protocol::{
            ClientToServerMessageStream, Request, Response, print_list_privileges_output_status,
            print_list_privileges_output_status_json,
        },
        types::MySQLDatabase,
    },
};

#[derive(Parser, Debug, Clone)]
pub struct ShowPrivsArgs {
    /// The MySQL database(s) to show privileges for
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

pub async fn show_database_privileges(
    args: ShowPrivsArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    let message = if args.name.is_empty() {
        Request::ListPrivileges(None)
    } else {
        Request::ListPrivileges(Some(args.name.to_owned()))
    };
    server_connection.send(message).await?;

    let privilege_data = match server_connection.next().await {
        Some(Ok(Response::ListPrivileges(databases))) => databases,
        Some(Ok(Response::ListAllPrivileges(privilege_rows))) => match privilege_rows {
            Ok(list) => list
                .into_iter()
                .map(|row| (row.db.clone(), row))
                .into_group_map()
                .into_iter()
                .map(|(db, rows)| (db, Ok(rows)))
                .collect(),
            Err(err) => {
                server_connection.send(Request::Exit).await?;
                return Err(anyhow::anyhow!(err.to_error_message())
                    .context("Failed to list database privileges"));
            }
        },
        response => return erroneous_server_response(response),
    };

    server_connection.send(Request::Exit).await?;

    if args.json {
        print_list_privileges_output_status_json(&privilege_data);
    } else {
        print_list_privileges_output_status(&privilege_data);
    }

    if args.fail && privilege_data.values().any(|res| res.is_err()) {
        std::process::exit(1);
    }

    Ok(())
}
