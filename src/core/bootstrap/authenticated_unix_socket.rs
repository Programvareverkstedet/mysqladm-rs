//! This module provides a way to authenticate a client uid to a server over a Unix socket.
//! This is needed so that the server can trust the client's uid, which it depends on to
//! make modifications for that user in the database. It is crucial that the server can trust
//! that the client is the unix user it claims to be.
//!
//! This works by having the client respond to a challenge on a socket that is verifiably owned
//! by the client. In more detailed steps, the following should happen:
//!
//! 1. Before initializing it's request, the client should open an "authentication" socket with permissions 644
//!    and owned by the uid of the current user.
//! 2. The client opens a request to the server on the "normal" socket where the server is listening,
//!    In this request, the client should include the following:
//!      - The address of it's authentication socket
//!      - The uid of the user currently using the client
//!      - A challenge string that has been randomly generated
//! 3. The server validates the following:
//!      - The address of the auth socket is valid
//!      - The owner of the auth socket address is the same as the uid
//! 4. Server connects to the auth socket address and receives another challenge string.
//!    The server should close the connection after receiving the challenge string.
//! 5. Server verifies that the challenge is the same as the one it originally received.
//!    It responds to the client with an "Authenticated" message if the challenge matches,
//!    or an error message if it does not.
//! 6. Client closes the authentication socket. Normal socket is used for communication.
//!
//! Note that the server can at any point in the process send an error message to the client
//! over it's initial connection, and the client should respond by closing the authentication
//! socket, it's connection to the normal socket, and reporting the error to the user.
//!
//! Also note that it is essential that the client does not send any sensitive information
//! over it's authentication socket, since it is readable by any user on the system.

// TODO: rewrite this so that it can be used with a normal std::os::unix::net::UnixStream

use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use async_bincode::{tokio::AsyncBincodeStream, AsyncDestination};
use derive_more::derive::{Display, Error};
use futures::{SinkExt, StreamExt};
use nix::{sys::stat, unistd::Uid};
use rand::distributions::Alphanumeric;
use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::net::{UnixListener, UnixStream};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClientRequest {
    Initialize {
        uid: u32,
        challenge: u64,
        auth_socket: String,
    },
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Display, Error)]
pub enum ServerResponse {
    Authenticated,
    ChallengeDidNotMatch,
    ServerError(ServerError),
}

// TODO: wrap more data into the errors

#[derive(Debug, Display, PartialEq, Serialize, Deserialize, Clone, Error)]
pub enum ServerError {
    InvalidRequest,
    UnableToReadPermissionsFromAuthSocket,
    CouldNotConnectToAuthSocket,
    AuthSocketClosedEarly,
    UidMismatch,
    ChallengeMismatch,
    InvalidChallenge,
}

#[derive(Debug, PartialEq, Display, Error)]
pub enum ClientError {
    UnableToConnectToServer,
    UnableToOpenAuthSocket,
    UnableToConfigureAuthSocket,
    AuthSocketClosedEarly,
    UnableToCloseAuthSocket,
    AuthenticationError,
    UnableToParseServerResponse,
    NoServerResponse,
    ServerError(ServerError),
}

async fn create_auth_socket(socket_addr: &PathBuf) -> Result<UnixListener, ClientError> {
    let auth_socket =
        UnixListener::bind(socket_addr).map_err(|_err| ClientError::UnableToOpenAuthSocket)?;

    stat::fchmod(
        auth_socket.as_raw_fd(),
        stat::Mode::S_IRUSR | stat::Mode::S_IWUSR | stat::Mode::S_IRGRP,
    )
    .map_err(|_err| ClientError::UnableToConfigureAuthSocket)?;

    Ok(auth_socket)
}

type ClientToServerStream<'a> =
    AsyncBincodeStream<&'a mut UnixStream, ServerResponse, ClientRequest, AsyncDestination>;
type ServerToClientStream<'a> =
    AsyncBincodeStream<&'a mut UnixStream, ClientRequest, ServerResponse, AsyncDestination>;

// TODO: make the challenge constant size and use socket directly, this is overkill
type AuthStream<'a> = AsyncBincodeStream<&'a mut UnixStream, u64, u64, AsyncDestination>;

// TODO: add timeout

// TODO: respect $XDG_RUNTIME_DIR and $TMPDIR

const AUTH_SOCKET_NAME: &str = "mysqladm-rs-cli-auth.sock";

pub async fn client_authenticate(
    normal_socket: &mut UnixStream,
    auth_socket_dir: Option<PathBuf>,
) -> Result<(), ClientError> {
    let random_prefix: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(16)
        .map(char::from)
        .collect();

    let socket_name = format!("{}-{}", random_prefix, AUTH_SOCKET_NAME);

    let auth_socket_address = auth_socket_dir
        .unwrap_or(std::env::temp_dir())
        .join(socket_name);

    client_authenticate_with_auth_socket_address(normal_socket, &auth_socket_address).await
}

async fn client_authenticate_with_auth_socket_address(
    normal_socket: &mut UnixStream,
    auth_socket_address: &PathBuf,
) -> Result<(), ClientError> {
    let auth_socket = create_auth_socket(auth_socket_address).await?;

    let result =
        client_authenticate_with_auth_socket(normal_socket, auth_socket, auth_socket_address).await;

    std::fs::remove_file(auth_socket_address)
        .map_err(|_err| ClientError::UnableToCloseAuthSocket)?;

    result
}

async fn client_authenticate_with_auth_socket(
    normal_socket: &mut UnixStream,
    auth_socket: UnixListener,
    auth_socket_address: &Path,
) -> Result<(), ClientError> {
    let challenge = rand::random::<u64>();
    let uid = nix::unistd::getuid();

    let mut normal_socket: ClientToServerStream =
        AsyncBincodeStream::from(normal_socket).for_async();

    let challenge_replier_cancellation_token = CancellationToken::new();
    let challenge_replier_cancellation_token_clone = challenge_replier_cancellation_token.clone();
    let challenge_replier_handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                socket = auth_socket.accept() =>
                  {
                  match socket {
                      Ok((mut conn, _addr)) => {
                          let mut stream: AuthStream = AsyncBincodeStream::from(&mut conn).for_async();
                          stream.send(challenge).await.ok();
                          stream.close().await.ok();
                      }
                      Err(_err) => return Err(ClientError::AuthSocketClosedEarly),
                  }
              }

                _ = challenge_replier_cancellation_token_clone.cancelled() => {
                    break Ok(());
                }
            }
        }
    });

    let client_hello = ClientRequest::Initialize {
        uid: uid.into(),
        challenge,
        auth_socket: auth_socket_address
            .to_str()
            .ok_or(ClientError::UnableToConfigureAuthSocket)?
            .to_owned(),
    };

    normal_socket
        .send(client_hello)
        .await
        .map_err(|err| match *err {
            bincode::ErrorKind::Io(_err) => ClientError::UnableToConnectToServer,
            _ => ClientError::NoServerResponse,
        })?;

    match normal_socket.next().await {
        Some(Ok(ServerResponse::Authenticated)) => {}
        Some(Ok(ServerResponse::ChallengeDidNotMatch)) => {
            return Err(ClientError::AuthenticationError)
        }
        Some(Ok(ServerResponse::ServerError(err))) => return Err(ClientError::ServerError(err)),
        Some(Err(err)) => match *err {
            bincode::ErrorKind::Io(_err) => return Err(ClientError::NoServerResponse),
            _ => return Err(ClientError::UnableToParseServerResponse),
        },
        None => return Err(ClientError::NoServerResponse),
    }

    challenge_replier_cancellation_token.cancel();
    challenge_replier_handle.await.ok();

    Ok(())
}

macro_rules! report_server_error_and_return {
    ($normal_socket:expr, $err:expr) => {{
        $normal_socket
            .send(ServerResponse::ServerError($err))
            .await
            .ok();
        return Err($err);
    }};
}

pub async fn server_authenticate(normal_socket: &mut UnixStream) -> Result<Uid, ServerError> {
    _server_authenticate(normal_socket, None).await
}

pub async fn _server_authenticate(
    normal_socket: &mut UnixStream,
    unix_user_uid: Option<u32>,
) -> Result<Uid, ServerError> {
    let mut normal_socket: ServerToClientStream =
        AsyncBincodeStream::from(normal_socket).for_async();

    let (uid, challenge, auth_socket) = match normal_socket.next().await {
        Some(Ok(ClientRequest::Initialize {
            uid,
            challenge,
            auth_socket,
        })) => (uid, challenge, auth_socket),
        // TODO: more granular errros
        _ => report_server_error_and_return!(normal_socket, ServerError::InvalidRequest),
    };

    let auth_socket_uid = match unix_user_uid {
        Some(uid) => uid,
        None => match stat::stat(auth_socket.as_str()) {
            Ok(stat) => stat.st_uid,
            Err(_err) => report_server_error_and_return!(
                normal_socket,
                ServerError::UnableToReadPermissionsFromAuthSocket
            ),
        },
    };

    if uid != auth_socket_uid {
        report_server_error_and_return!(normal_socket, ServerError::UidMismatch);
    }

    let mut authenticated_unix_socket = match UnixStream::connect(auth_socket).await {
        Ok(socket) => socket,
        Err(_err) => {
            report_server_error_and_return!(normal_socket, ServerError::CouldNotConnectToAuthSocket)
        }
    };
    let mut authenticated_unix_socket: AuthStream =
        AsyncBincodeStream::from(&mut authenticated_unix_socket).for_async();

    let challenge_2 = match authenticated_unix_socket.next().await {
        Some(Ok(challenge)) => challenge,
        Some(Err(_)) => {
            report_server_error_and_return!(normal_socket, ServerError::InvalidChallenge)
        }
        None => report_server_error_and_return!(normal_socket, ServerError::AuthSocketClosedEarly),
    };

    authenticated_unix_socket.close().await.ok();

    if challenge != challenge_2 {
        normal_socket
            .send(ServerResponse::ChallengeDidNotMatch)
            .await
            .ok();
        return Err(ServerError::ChallengeMismatch);
    }

    normal_socket.send(ServerResponse::Authenticated).await.ok();

    Ok(Uid::from_raw(uid))
}

#[cfg(test)]
mod test {
    use super::*;

    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_valid_authentication() {
        let (mut client, mut server) = UnixStream::pair().unwrap();

        let client_handle =
            tokio::spawn(async move { client_authenticate(&mut client, None).await });

        let server_handle = tokio::spawn(async move { server_authenticate(&mut server).await });

        client_handle.await.unwrap().unwrap();
        server_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn test_ensure_auth_socket_does_not_exist() {
        let (mut client, mut server) = UnixStream::pair().unwrap();

        let client_handle = tokio::spawn(async move {
            client_authenticate_with_auth_socket_address(
                &mut client,
                &PathBuf::from("/tmp/test_auth_socket_does_not_exist.sock"),
            )
            .await
        });

        let server_handle = tokio::spawn(async move { server_authenticate(&mut server).await });

        client_handle.await.unwrap().unwrap();
        server_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn test_uid_mismatch() {
        let (mut client, mut server) = UnixStream::pair().unwrap();

        let client_handle = tokio::spawn(async move {
            let err = client_authenticate(&mut client, None).await;
            assert_eq!(err, Err(ClientError::ServerError(ServerError::UidMismatch)));
        });

        let server_handle = tokio::spawn(async move {
            let uid: u32 = nix::unistd::getuid().into();
            let err = _server_authenticate(&mut server, Some(uid + 1)).await;
            assert_eq!(err, Err(ServerError::UidMismatch));
        });

        client_handle.await.unwrap();
        server_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_snooping_connection() {
        let (mut client, mut server) = UnixStream::pair().unwrap();

        let socket_path = std::env::temp_dir().join("socket_to_snoop.sock");
        let socket_path_clone = socket_path.clone();
        let client_handle = tokio::spawn(async move {
            client_authenticate_with_auth_socket_address(&mut client, &socket_path_clone).await
        });

        for i in 0..100 {
            if socket_path.exists() {
                break;
            }
            sleep(Duration::from_millis(10)).await;

            if i == 99 {
                panic!("Socket not created after 1 second, assuming test failure");
            }
        }

        let mut snooper = UnixStream::connect(socket_path.clone()).await.unwrap();
        let mut snooper: AuthStream = AsyncBincodeStream::from(&mut snooper).for_async();
        let message = snooper.next().await.unwrap().unwrap();

        let mut other_snooper = UnixStream::connect(socket_path.clone()).await.unwrap();
        let mut other_snooper: AuthStream =
            AsyncBincodeStream::from(&mut other_snooper).for_async();
        let other_message = other_snooper.next().await.unwrap().unwrap();

        assert_eq!(message, other_message);

        let third_snooper_handle = tokio::spawn(async move {
            let mut third_snooper = UnixStream::connect(socket_path.clone()).await.unwrap();
            let mut third_snooper: AuthStream =
                AsyncBincodeStream::from(&mut third_snooper).for_async();
            // NOTE: Should hang
            third_snooper.send(1234).await.unwrap()
        });

        sleep(Duration::from_millis(10)).await;

        let server_handle = tokio::spawn(async move { server_authenticate(&mut server).await });

        client_handle.await.unwrap().unwrap();
        server_handle.await.unwrap().unwrap();

        third_snooper_handle.abort();
    }

    #[tokio::test]
    async fn test_dead_server() {
        let (mut client, server) = UnixStream::pair().unwrap();
        std::mem::drop(server);

        let client_handle = tokio::spawn(async move {
            let err = client_authenticate(&mut client, None).await;
            assert_eq!(err, Err(ClientError::UnableToConnectToServer));
        });

        client_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_no_response_from_server() {
        let (mut client, server) = UnixStream::pair().unwrap();

        let client_handle = tokio::spawn(async move {
            let err = client_authenticate(&mut client, None).await;
            assert_eq!(err, Err(ClientError::NoServerResponse));
        });

        sleep(Duration::from_millis(200)).await;

        std::mem::drop(server);

        client_handle.await.unwrap();
    }

    // TODO: Test challenge mismatch
    // TODO: Test invoking server with junk data
}
