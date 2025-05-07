use std::{collections::BTreeSet, fs, path::PathBuf};

use futures_util::{SinkExt, StreamExt};
use indoc::concatdoc;
use tokio::net::{UnixListener, UnixStream};

use sqlx::MySqlConnection;
use sqlx::prelude::*;

use crate::core::protocol::SetPasswordError;
use crate::server::sql::database_operations::list_databases;
use crate::{
    core::{
        common::{DEFAULT_SOCKET_PATH, UnixUser},
        protocol::request_response::{
            Request, Response, ServerToClientMessageStream, create_server_to_client_message_stream,
        },
    },
    server::{
        config::{ServerConfig, create_mysql_connection_from_config},
        sql::{
            database_operations::{create_databases, drop_databases, list_all_databases_for_user},
            database_privilege_operations::{
                apply_privilege_diffs, get_all_database_privileges, get_databases_privilege_data,
            },
            user_operations::{
                create_database_users, drop_database_users, list_all_database_users_for_unix_user,
                list_database_users, lock_database_users, set_password_for_database_user,
                unlock_database_users,
            },
        },
    },
};

// TODO: consider using a connection pool

pub async fn listen_for_incoming_connections(
    socket_path: Option<PathBuf>,
    config: ServerConfig,
    // db_connection: &mut MySqlConnection,
) -> anyhow::Result<()> {
    let socket_path = socket_path.unwrap_or(PathBuf::from(DEFAULT_SOCKET_PATH));

    let parent_directory = socket_path.parent().unwrap();
    if !parent_directory.exists() {
        log::debug!("Creating directory {:?}", parent_directory);
        fs::create_dir_all(parent_directory)?;
    }

    log::info!("Listening on socket {:?}", socket_path);

    match fs::remove_file(socket_path.as_path()) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }

    let listener = UnixListener::bind(socket_path)?;

    sd_notify::notify(false, &[sd_notify::NotifyState::Ready]).ok();

    while let Ok((conn, _addr)) = listener.accept().await {
        let uid = match conn.peer_cred() {
            Ok(cred) => cred.uid(),
            Err(e) => {
                log::error!("Failed to get peer credentials from socket: {}", e);
                let mut message_stream = create_server_to_client_message_stream(conn);
                message_stream
                    .send(Response::Error(
                        (concatdoc! {
                            "Server failed to get peer credentials from socket\n",
                            "Please check the server logs or contact the system administrators"
                        })
                        .to_string(),
                    ))
                    .await
                    .ok();
                continue;
            }
        };

        log::debug!("Accepted connection from uid {}", uid);

        let unix_user = match UnixUser::from_uid(uid) {
            Ok(user) => user,
            Err(e) => {
                log::error!("Failed to get username from uid: {}", e);
                let mut message_stream = create_server_to_client_message_stream(conn);
                message_stream
                    .send(Response::Error(
                        (concatdoc! {
                            "Server failed to get user data from the system\n",
                            "Please check the server logs or contact the system administrators"
                        })
                        .to_string(),
                    ))
                    .await
                    .ok();
                continue;
            }
        };

        log::info!("Accepted connection from {}", unix_user.username);

        match handle_requests_for_single_session(conn, &unix_user, &config).await {
            Ok(()) => {}
            Err(e) => {
                log::error!("Failed to run server: {}", e);
            }
        }
    }

    Ok(())
}

pub async fn handle_requests_for_single_session(
    socket: UnixStream,
    unix_user: &UnixUser,
    config: &ServerConfig,
) -> anyhow::Result<()> {
    let mut message_stream = create_server_to_client_message_stream(socket);
    let mut db_connection = match create_mysql_connection_from_config(&config.mysql).await {
        Ok(connection) => connection,
        Err(err) => {
            message_stream
                .send(Response::Error(
                    (concatdoc! {
                        "Server failed to connect to database\n",
                        "Please check the server logs or contact the system administrators"
                    })
                    .to_string(),
                ))
                .await?;
            message_stream.flush().await?;
            return Err(err);
        }
    };

    log::debug!("Successfully connected to database");

    let result = handle_requests_for_single_session_with_db_connection(
        message_stream,
        unix_user,
        &mut db_connection,
    )
    .await;

    if let Err(e) = db_connection.close().await {
        log::error!("Failed to close database connection: {}", e);
        log::error!("{}", e);
        log::error!("Ignoring...");
    }

    result
}

// TODO: ensure proper db_connection hygiene for functions that invoke
//       this function

pub async fn handle_requests_for_single_session_with_db_connection(
    mut stream: ServerToClientMessageStream,
    unix_user: &UnixUser,
    db_connection: &mut MySqlConnection,
) -> anyhow::Result<()> {
    stream.send(Response::Ready).await?;
    loop {
        // TODO: better error handling
        let request = match stream.next().await {
            Some(Ok(request)) => request,
            Some(Err(e)) => return Err(e.into()),
            None => {
                log::warn!("Client disconnected without sending an exit message");
                break;
            }
        };

        // TODO: don't clone the request
        let request_to_display = match &request {
            Request::PasswdUser(db_user, _) => {
                Request::PasswdUser(db_user.to_owned(), "<REDACTED>".to_string())
            }
            request => request.to_owned(),
        };
        log::info!("Received request: {:#?}", request_to_display);

        let response = match request {
            Request::CreateDatabases(databases_names) => {
                let result = create_databases(databases_names, unix_user, db_connection).await;
                Response::CreateDatabases(result)
            }
            Request::DropDatabases(databases_names) => {
                let result = drop_databases(databases_names, unix_user, db_connection).await;
                Response::DropDatabases(result)
            }
            Request::ListDatabases(database_names) => match database_names {
                Some(database_names) => {
                    let result = list_databases(database_names, unix_user, db_connection).await;
                    Response::ListDatabases(result)
                }
                None => {
                    let result = list_all_databases_for_user(unix_user, db_connection).await;
                    Response::ListAllDatabases(result)
                }
            },
            Request::ListPrivileges(database_names) => match database_names {
                Some(database_names) => {
                    let privilege_data =
                        get_databases_privilege_data(database_names, unix_user, db_connection)
                            .await;
                    Response::ListPrivileges(privilege_data)
                }
                None => {
                    let privilege_data =
                        get_all_database_privileges(unix_user, db_connection).await;
                    Response::ListAllPrivileges(privilege_data)
                }
            },
            Request::ModifyPrivileges(database_privilege_diffs) => {
                let result = apply_privilege_diffs(
                    BTreeSet::from_iter(database_privilege_diffs),
                    unix_user,
                    db_connection,
                )
                .await;
                Response::ModifyPrivileges(result)
            }
            Request::CreateUsers(db_users) => {
                let result = create_database_users(db_users, unix_user, db_connection).await;
                Response::CreateUsers(result)
            }
            Request::DropUsers(db_users) => {
                let result = drop_database_users(db_users, unix_user, db_connection).await;
                Response::DropUsers(result)
            }
            Request::PasswdUser(db_user, password) => {
                let result =
                    set_password_for_database_user(&db_user, &password, unix_user, db_connection)
                        .await;
                Response::PasswdUser(result)
            }
            Request::ListUsers(db_users) => match db_users {
                Some(db_users) => {
                    let result = list_database_users(db_users, unix_user, db_connection).await;
                    Response::ListUsers(result)
                }
                None => {
                    let result =
                        list_all_database_users_for_unix_user(unix_user, db_connection).await;
                    Response::ListAllUsers(result)
                }
            },
            Request::LockUsers(db_users) => {
                let result = lock_database_users(db_users, unix_user, db_connection).await;
                Response::LockUsers(result)
            }
            Request::UnlockUsers(db_users) => {
                let result = unlock_database_users(db_users, unix_user, db_connection).await;
                Response::UnlockUsers(result)
            }
            Request::Exit => {
                break;
            }
        };

        // TODO: don't clone the response
        let response_to_display = match &response {
            Response::PasswdUser(Err(SetPasswordError::MySqlError(_))) => {
                Response::PasswdUser(Err(SetPasswordError::MySqlError("<REDACTED>".to_string())))
            }
            response => response.to_owned(),
        };
        log::info!("Response: {:#?}", response_to_display);

        stream.send(response).await?;
        stream.flush().await?;
        log::debug!("Successfully processed request");
    }

    Ok(())
}
