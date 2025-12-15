use clap::Parser;
use clap_complete::ArgValueCompleter;
use futures_util::SinkExt;
use tokio_stream::StreamExt;

use crate::{
    client::commands::{erroneous_server_response, print_authorization_owner_hint},
    core::{
        completion::mysql_user_completer,
        protocol::{
            ClientToServerMessageStream, LockUserError, Request, Response,
            print_lock_users_output_status, print_lock_users_output_status_json,
            request_validation::ValidationError,
        },
        types::MySQLUser,
    },
};

#[derive(Parser, Debug, Clone)]
pub struct LockUserArgs {
    /// The MySQL user(s) to loc
    #[arg(num_args = 1.., value_name = "USER_NAME")]
    #[cfg_attr(not(feature = "suid-sgid-mode"), arg(add = ArgValueCompleter::new(mysql_user_completer)))]
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

    if args.json {
        print_lock_users_output_status_json(&result);
    } else {
        print_lock_users_output_status(&result);

        if result.iter().any(|(_, res)| {
            matches!(
                res,
                Err(LockUserError::ValidationError(
                    ValidationError::AuthorizationError(_)
                ))
            )
        }) {
            print_authorization_owner_hint(&mut server_connection).await?
        }
    }

    server_connection.send(Request::Exit).await?;

    Ok(())
}
