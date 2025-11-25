use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use tokio::net::UnixStream;
use tokio_serde::{Framed as SerdeFramed, formats::Bincode};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use crate::core::{
    database_privileges::DatabasePrivilegesDiff,
    protocol::*,
    types::{MySQLDatabase, MySQLUser},
};

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
    CreateDatabases(Vec<MySQLDatabase>),
    DropDatabases(Vec<MySQLDatabase>),
    ListDatabases(Option<Vec<MySQLDatabase>>),
    ListPrivileges(Option<Vec<MySQLDatabase>>),
    ModifyPrivileges(BTreeSet<DatabasePrivilegesDiff>),

    CreateUsers(Vec<MySQLUser>),
    DropUsers(Vec<MySQLUser>),
    PasswdUser(MySQLUser, String),
    ListUsers(Option<Vec<MySQLUser>>),
    LockUsers(Vec<MySQLUser>),
    UnlockUsers(Vec<MySQLUser>),

    // Commit,
    Exit,
}

// TODO: include a generic "message" that will display a message to the user?

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Response {
    // Specific data for specific commands
    CreateDatabases(CreateDatabasesOutput),
    DropDatabases(DropDatabasesOutput),
    ListDatabases(ListDatabasesOutput),
    ListAllDatabases(ListAllDatabasesOutput),
    ListPrivileges(GetDatabasesPrivilegeData),
    ListAllPrivileges(GetAllDatabasesPrivilegeData),
    ModifyPrivileges(ModifyDatabasePrivilegesOutput),

    CreateUsers(CreateUsersOutput),
    DropUsers(DropUsersOutput),
    PasswdUser(SetPasswordOutput),
    ListUsers(ListUsersOutput),
    ListAllUsers(ListAllUsersOutput),
    LockUsers(LockUsersOutput),
    UnlockUsers(UnlockUsersOutput),

    // Generic responses
    Ready,
    Error(String),
}
