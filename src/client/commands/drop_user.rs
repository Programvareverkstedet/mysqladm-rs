use clap::Parser;
use clap_complete::ArgValueCompleter;
use dialoguer::Confirm;
use futures_util::SinkExt;
use tokio_stream::StreamExt;

use crate::{
    client::commands::{erroneous_server_response, print_authorization_owner_hint},
    core::{
        completion::mysql_user_completer,
        protocol::{
            ClientToServerMessageStream, DropUserError, Request, Response,
            print_drop_users_output_status, print_drop_users_output_status_json,
            request_validation::ValidationError,
        },
        types::MySQLUser,
    },
};

#[derive(Parser, Debug, Clone)]
pub struct DropUserArgs {
    /// The `MySQL` user(s) to drop
    #[arg(num_args = 1.., value_name = "USER_NAME")]
    #[cfg_attr(not(feature = "suid-sgid-mode"), arg(add = ArgValueCompleter::new(mysql_user_completer)))]
    username: Vec<MySQLUser>,

    /// Print the information as JSON
    #[arg(short, long)]
    json: bool,

    /// Automatically confirm action without prompting
    #[arg(short, long)]
    yes: bool,
}

pub async fn drop_users(
    args: DropUserArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    if args.username.is_empty() {
        anyhow::bail!("No usernames provided");
    }

    if !args.yes {
        let confirmation = Confirm::new()
            .with_prompt(format!(
                "Are you sure you want to drop the users?\n\n{}\n\nThis action cannot be undone",
                args.username
                    .iter()
                    .map(|d| format!("- {d}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ))
            .interact()?;

        if !confirmation {
            // TODO: should we return with an error code here?
            println!("Aborting drop operation.");
            server_connection.send(Request::Exit).await?;
            return Ok(());
        }
    }

    let message = Request::DropUsers(args.username.clone());

    if let Err(err) = server_connection.send(message).await {
        server_connection.close().await.ok();
        anyhow::bail!(err);
    }

    let result = match server_connection.next().await {
        Some(Ok(Response::DropUsers(result))) => result,
        response => return erroneous_server_response(response),
    };

    if args.json {
        print_drop_users_output_status_json(&result);
    } else {
        print_drop_users_output_status(&result);

        if result.iter().any(|(_, res)| {
            matches!(
                res,
                Err(DropUserError::ValidationError(
                    ValidationError::AuthorizationError(_)
                ))
            )
        }) {
            print_authorization_owner_hint(&mut server_connection).await?;
        }
    }

    server_connection.send(Request::Exit).await?;

    if result.values().any(std::result::Result::is_err) {
        std::process::exit(1);
    }

    Ok(())
}
