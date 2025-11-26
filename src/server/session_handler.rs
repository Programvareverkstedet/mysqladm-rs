use std::collections::BTreeSet;

use futures_util::{SinkExt, StreamExt};
use indoc::concatdoc;
use sqlx::{MySql, MySqlConnection, MySqlPool, pool::PoolConnection};
use tokio::net::UnixStream;

use crate::{
    core::{
        common::UnixUser,
        protocol::{
            Request, Response, ServerToClientMessageStream, SetPasswordError,
            create_server_to_client_message_stream,
        },
    },
    server::sql::{
        database_operations::{
            create_databases, drop_databases, list_all_databases_for_user, list_databases,
        },
        database_privilege_operations::{
            apply_privilege_diffs, get_all_database_privileges, get_databases_privilege_data,
        },
        user_operations::{
            create_database_users, drop_database_users, list_all_database_users_for_unix_user,
            list_database_users, lock_database_users, set_password_for_database_user,
            unlock_database_users,
        },
    },
};

// TODO: don't use database connection unless necessary.

pub async fn session_handler(
    socket: UnixStream,
    unix_user: &UnixUser,
    db_pool: MySqlPool,
) -> anyhow::Result<()> {
    let mut message_stream = create_server_to_client_message_stream(socket);

    log::debug!("Opening connection to database");

    let mut db_connection = match db_pool.acquire().await {
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
            return Err(err.into());
        }
    };

    log::debug!("Successfully connected to database");

    let result =
        session_handler_with_db_connection(message_stream, unix_user, &mut db_connection).await;

    close_or_ignore_db_connection(db_connection).await;

    result
}

// TODO: ensure proper db_connection hygiene for functions that invoke
//       this function

async fn session_handler_with_db_connection(
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
            Request::PasswdUser((db_user, _)) => {
                Request::PasswdUser((db_user.to_owned(), "<REDACTED>".to_string()))
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
            Request::PasswdUser((db_user, password)) => {
                let result =
                    set_password_for_database_user(&db_user, &password, unix_user, db_connection)
                        .await;
                Response::SetUserPassword(result)
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
            Response::SetUserPassword(Err(SetPasswordError::MySqlError(_))) => {
                Response::SetUserPassword(Err(SetPasswordError::MySqlError(
                    "<REDACTED>".to_string(),
                )))
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

async fn close_or_ignore_db_connection(db_connection: PoolConnection<MySql>) {
    if let Err(e) = db_connection.close().await {
        log::error!("Failed to close database connection: {}", e);
        log::error!("{}", e);
        log::error!("Ignoring...");
    }
}
