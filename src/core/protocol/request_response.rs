use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use tokio::net::UnixStream;
use tokio_serde::{formats::Bincode, Framed as SerdeFramed};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use crate::core::{database_privileges::DatabasePrivilegesDiff, protocol::*};

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
    CreateDatabases(Vec<String>),
    DropDatabases(Vec<String>),
    ListDatabases,
    ListPrivileges(Option<Vec<String>>),
    ModifyPrivileges(BTreeSet<DatabasePrivilegesDiff>),

    CreateUsers(Vec<String>),
    DropUsers(Vec<String>),
    PasswdUser(String, String),
    ListUsers(Option<Vec<String>>),
    LockUsers(Vec<String>),
    UnlockUsers(Vec<String>),

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
