use clap::Parser;
use clap_complete::ArgValueCompleter;
use futures_util::SinkExt;
use prettytable::{Cell, Row, Table};
use tokio_stream::StreamExt;

use crate::{
    client::commands::erroneous_server_response,
    core::{
        common::yn,
        completion::mysql_database_completer,
        database_privileges::{DATABASE_PRIVILEGE_FIELDS, db_priv_field_human_readable_name},
        protocol::{ClientToServerMessageStream, Request, Response},
        types::MySQLDatabase,
    },
};

#[derive(Parser, Debug, Clone)]
pub struct ShowPrivsArgs {
    /// The MySQL database(s) to show privileges for
    #[arg(num_args = 0.., add = ArgValueCompleter::new(mysql_database_completer))]
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

    let mut contained_errors = false;
    let privilege_data = match server_connection.next().await {
        Some(Ok(Response::ListPrivileges(databases))) => databases
            .into_iter()
            .filter_map(|(database_name, result)| match result {
                Ok(privileges) => Some(privileges),
                Err(err) => {
                    contained_errors = true;
                    eprintln!("{}", err.to_error_message(&database_name));
                    eprintln!("Skipping...");
                    println!();
                    None
                }
            })
            .flatten()
            .collect::<Vec<_>>(),
        Some(Ok(Response::ListAllPrivileges(privilege_rows))) => match privilege_rows {
            Ok(list) => list,
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
        println!("{}", serde_json::to_string_pretty(&privilege_data)?);
    } else if privilege_data.is_empty() {
        println!("No database privileges to show.");
    } else {
        let mut table = Table::new();
        table.add_row(Row::new(
            DATABASE_PRIVILEGE_FIELDS
                .into_iter()
                .map(db_priv_field_human_readable_name)
                .map(|name| Cell::new(&name))
                .collect(),
        ));

        for row in privilege_data {
            table.add_row(row![
                row.db,
                row.user,
                c->yn(row.select_priv),
                c->yn(row.insert_priv),
                c->yn(row.update_priv),
                c->yn(row.delete_priv),
                c->yn(row.create_priv),
                c->yn(row.drop_priv),
                c->yn(row.alter_priv),
                c->yn(row.index_priv),
                c->yn(row.create_tmp_table_priv),
                c->yn(row.lock_tables_priv),
                c->yn(row.references_priv),
            ]);
        }
        table.printstd();
    }

    if args.fail && contained_errors {
        std::process::exit(1);
    }

    Ok(())
}
