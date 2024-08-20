use std::{
    collections::BTreeSet,
    fmt::{Display, Formatter},
    ops::{Deref, DerefMut},
    str::FromStr,
};

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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MySQLUser(String);

impl FromStr for MySQLUser {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(MySQLUser(s.to_string()))
    }
}

impl Deref for MySQLUser {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for MySQLUser {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Display for MySQLUser {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for MySQLUser {
    fn from(s: &str) -> Self {
        MySQLUser(s.to_string())
    }
}

impl From<String> for MySQLUser {
    fn from(s: String) -> Self {
        MySQLUser(s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MySQLDatabase(String);

impl FromStr for MySQLDatabase {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(MySQLDatabase(s.to_string()))
    }
}

impl Deref for MySQLDatabase {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for MySQLDatabase {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Display for MySQLDatabase {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for MySQLDatabase {
    fn from(s: &str) -> Self {
        MySQLDatabase(s.to_string())
    }
}

impl From<String> for MySQLDatabase {
    fn from(s: String) -> Self {
        MySQLDatabase(s)
    }
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
