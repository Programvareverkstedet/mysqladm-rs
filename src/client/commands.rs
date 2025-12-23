mod check_auth;
mod create_db;
mod create_user;
mod drop_db;
mod drop_user;
mod edit_privs;
mod lock_user;
mod passwd_user;
mod show_db;
mod show_privs;
mod show_user;
mod unlock_user;

pub use check_auth::*;
pub use create_db::*;
pub use create_user::*;
pub use drop_db::*;
pub use drop_user::*;
pub use edit_privs::*;
pub use lock_user::*;
pub use passwd_user::*;
pub use show_db::*;
pub use show_privs::*;
pub use show_user::*;
pub use unlock_user::*;

use futures_util::SinkExt;
use itertools::Itertools;
use tokio_stream::StreamExt;

use crate::core::protocol::{ClientToServerMessageStream, Request, Response};

/// Handle an unexpected or erroneous response from the server.
///
/// This function checks the provided response and returns an appropriate error message.
/// It is typically used in `match` branches for expecting a specific response type from the server.
pub fn erroneous_server_response(
    response: Option<Result<Response, std::io::Error>>,
) -> anyhow::Result<()> {
    match response {
        Some(Ok(Response::Error(e))) => {
            anyhow::bail!("Server returned error: {e}");
        }
        Some(Err(e)) => {
            anyhow::bail!(e);
        }
        Some(response) => {
            anyhow::bail!("Unexpected response from server: {response:?}");
        }
        None => {
            anyhow::bail!("No response from server");
        }
    }
}

/// Print a hint about which name prefixes the user is authorized to manage
/// by querying the server for valid name prefixes.
///
/// This function should be used when an authorization error occurs,
/// to help the user understand which databases or users they are allowed to manage.
async fn print_authorization_owner_hint(
    server_connection: &mut ClientToServerMessageStream,
) -> anyhow::Result<()> {
    server_connection
        .send(Request::ListValidNamePrefixes)
        .await?;

    let response = match server_connection.next().await {
        Some(Ok(Response::ListValidNamePrefixes(prefixes))) => prefixes,
        response => return erroneous_server_response(response),
    };

    eprintln!(
        "Note: You are allowed to manage databases and users with the following prefixes:\n{}",
        response.into_iter().map(|p| format!(" - {p}")).join("\n")
    );

    Ok(())
}
