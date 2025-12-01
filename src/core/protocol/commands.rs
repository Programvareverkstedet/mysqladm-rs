mod check_authorization;
mod complete_database_name;
mod complete_user_name;
mod create_databases;
mod create_users;
mod drop_databases;
mod drop_users;
mod list_all_databases;
mod list_all_privileges;
mod list_all_users;
mod list_databases;
mod list_privileges;
mod list_users;
mod lock_users;
mod modify_privileges;
mod passwd_user;
mod unlock_users;

pub use check_authorization::*;
pub use complete_database_name::*;
pub use complete_user_name::*;
pub use create_databases::*;
pub use create_users::*;
pub use drop_databases::*;
pub use drop_users::*;
pub use list_all_databases::*;
pub use list_all_privileges::*;
pub use list_all_users::*;
pub use list_databases::*;
pub use list_privileges::*;
pub use list_users::*;
pub use lock_users::*;
pub use modify_privileges::*;
pub use passwd_user::*;
pub use unlock_users::*;

use serde::{Deserialize, Serialize};
use tokio::net::UnixStream;
use tokio_serde::{Framed as SerdeFramed, formats::Bincode};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

pub type ServerToClientMessageStream = SerdeFramed<
    Framed<UnixStream, LengthDelimitedCodec>,
    Request,
    Response,
    Bincode<Request, Response>,
>;

pub type ClientToServerMessageStream = SerdeFramed<
    Framed<UnixStream, LengthDelimitedCodec>,
    Response,
    Request,
    Bincode<Response, Request>,
>;

pub fn create_server_to_client_message_stream(socket: UnixStream) -> ServerToClientMessageStream {
    let length_delimited = Framed::new(socket, LengthDelimitedCodec::new());
    tokio_serde::Framed::new(length_delimited, Bincode::default())
}

pub fn create_client_to_server_message_stream(socket: UnixStream) -> ClientToServerMessageStream {
    let length_delimited = Framed::new(socket, LengthDelimitedCodec::new());
    tokio_serde::Framed::new(length_delimited, Bincode::default())
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Request {
    CheckAuthorization(CheckAuthorizationRequest),

    CompleteDatabaseName(CompleteDatabaseNameRequest),
    CompleteUserName(CompleteUserNameRequest),

    CreateDatabases(CreateDatabasesRequest),
    DropDatabases(DropDatabasesRequest),
    ListDatabases(ListDatabasesRequest),
    ListPrivileges(ListPrivilegesRequest),
    ModifyPrivileges(ModifyPrivilegesRequest),

    CreateUsers(CreateUsersRequest),
    DropUsers(DropUsersRequest),
    PasswdUser(SetUserPasswordRequest),
    ListUsers(ListUsersRequest),
    LockUsers(LockUsersRequest),
    UnlockUsers(UnlockUsersRequest),

    // Commit,
    Exit,
}

// TODO: include a generic "message" that will display a message to the user?

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Response {
    CheckAuthorization(CheckAuthorizationResponse),

    CompleteDatabaseName(CompleteDatabaseNameResponse),
    CompleteUserName(CompleteUserNameResponse),

    // Specific data for specific commands
    CreateDatabases(CreateDatabasesResponse),
    DropDatabases(DropDatabasesResponse),
    ListDatabases(ListDatabasesResponse),
    ListAllDatabases(ListAllDatabasesResponse),
    ListPrivileges(ListPrivilegesResponse),
    ListAllPrivileges(ListAllPrivilegesResponse),
    ModifyPrivileges(ModifyPrivilegesResponse),

    CreateUsers(CreateUsersResponse),
    DropUsers(DropUsersResponse),
    SetUserPassword(SetUserPasswordResponse),
    ListUsers(ListUsersResponse),
    ListAllUsers(ListAllUsersResponse),
    LockUsers(LockUsersResponse),
    UnlockUsers(UnlockUsersResponse),

    // Generic responses
    Ready,
    Error(String),
}
