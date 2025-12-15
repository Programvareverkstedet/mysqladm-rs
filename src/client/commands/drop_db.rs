use clap::Parser;
use clap_complete::ArgValueCompleter;
use dialoguer::Confirm;
use futures_util::SinkExt;
use tokio_stream::StreamExt;

use crate::{
    client::commands::{erroneous_server_response, print_authorization_owner_hint},
    core::{
        completion::mysql_database_completer,
        protocol::{
            ClientToServerMessageStream, DropDatabaseError, Request, Response,
            print_drop_databases_output_status, print_drop_databases_output_status_json,
            request_validation::ValidationError,
        },
        types::MySQLDatabase,
    },
};

#[derive(Parser, Debug, Clone)]
pub struct DropDbArgs {
    /// The MySQL database(s) to drop
    #[arg(num_args = 1.., value_name = "DB_NAME")]
    #[cfg_attr(not(feature = "suid-sgid-mode"), arg(add = ArgValueCompleter::new(mysql_database_completer)))]
    name: Vec<MySQLDatabase>,

    /// Print the information as JSON
    #[arg(short, long)]
    json: bool,

    /// Automatically confirm action without prompting
    #[arg(short, long)]
    yes: bool,
}

pub async fn drop_databases(
    args: DropDbArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    if args.name.is_empty() {
        anyhow::bail!("No database names provided");
    }

    if !args.yes {
        let confirmation = Confirm::new()
            .with_prompt(format!(
                "Are you sure you want to drop the databases?\n\n{}\n\nThis action cannot be undone",
                args.name
                    .iter()
                    .map(|d| format!("- {}", d))
                    .collect::<Vec<_>>()
                    .join("\n")
            ))
            .interact()?;

        if !confirmation {
            println!("Aborting drop operation.");
            return Ok(());
        }
    }

    let message = Request::DropDatabases(args.name.to_owned());
    server_connection.send(message).await?;

    let result = match server_connection.next().await {
        Some(Ok(Response::DropDatabases(result))) => result,
        response => return erroneous_server_response(response),
    };

    if args.json {
        print_drop_databases_output_status_json(&result);
    } else {
        print_drop_databases_output_status(&result);

        if result.iter().any(|(_, res)| {
            matches!(
                res,
                Err(DropDatabaseError::ValidationError(
                    ValidationError::AuthorizationError(_)
                ))
            )
        }) {
            print_authorization_owner_hint(&mut server_connection).await?
        }
    };

    server_connection.send(Request::Exit).await?;

    Ok(())
}
