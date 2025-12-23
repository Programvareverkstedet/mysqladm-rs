use clap_complete::CompletionCandidate;
use clap_verbosity_flag::Verbosity;
use futures_util::SinkExt;
use tokio::net::UnixStream as TokioUnixStream;
use tokio_stream::StreamExt;

use crate::{
    client::commands::erroneous_server_response,
    core::{
        bootstrap::bootstrap_server_connection_and_drop_privileges,
        protocol::{Request, Response, create_client_to_server_message_stream},
    },
};

#[must_use]
pub fn prefix_completer(current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => match runtime.block_on(prefix_completer_(current)) {
            Ok(completions) => completions,
            Err(err) => {
                eprintln!("Error getting prefix completions: {err}");
                Vec::new()
            }
        },
        Err(err) => {
            eprintln!("Error starting Tokio runtime: {err}");
            Vec::new()
        }
    }
}

/// Connect to the server to get `MySQL` user completions.
async fn prefix_completer_(_current: &std::ffi::OsStr) -> anyhow::Result<Vec<CompletionCandidate>> {
    let server_connection =
        bootstrap_server_connection_and_drop_privileges(None, None, Verbosity::new(0, 1))?;

    let tokio_socket = TokioUnixStream::from_std(server_connection)?;
    let mut server_connection = create_client_to_server_message_stream(tokio_socket);

    while let Some(Ok(message)) = server_connection.next().await {
        match message {
            Response::Error(err) => {
                anyhow::bail!("{err}");
            }
            Response::Ready => break,
            message => {
                eprintln!("Unexpected message from server: {message:?}");
            }
        }
    }

    let message = Request::ListValidNamePrefixes;

    if let Err(err) = server_connection.send(message).await {
        server_connection.close().await.ok();
        anyhow::bail!(anyhow::Error::from(err).context("Failed to communicate with server"));
    }

    let result = match server_connection.next().await {
        Some(Ok(Response::ListValidNamePrefixes(prefixes))) => prefixes,
        response => return erroneous_server_response(response).map(|()| vec![]),
    };

    server_connection.send(Request::Exit).await?;

    let result = result
        .into_iter()
        .map(|prefix| prefix + "_")
        .map(CompletionCandidate::new)
        .collect();

    Ok(result)
}
