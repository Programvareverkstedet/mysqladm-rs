use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    core::{
        protocol::request_validation::{DbOrUser, NameValidationError, OwnerValidationError},
        types::MySQLUser,
    },
    server::sql::user_operations::DatabaseUser,
};

pub type ListUsersRequest = Option<Vec<MySQLUser>>;

pub type ListUsersResponse = BTreeMap<MySQLUser, Result<DatabaseUser, ListUsersError>>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ListUsersError {
    SanitizationError(NameValidationError),
    OwnershipError(OwnerValidationError),
    UserDoesNotExist,
    MySqlError(String),
}

impl ListUsersError {
    pub fn to_error_message(&self, username: &MySQLUser) -> String {
        match self {
            ListUsersError::SanitizationError(err) => {
                err.to_error_message(username, DbOrUser::User)
            }
            ListUsersError::OwnershipError(err) => err.to_error_message(username, DbOrUser::User),
            ListUsersError::UserDoesNotExist => {
                format!("User '{}' does not exist.", username)
            }
            ListUsersError::MySqlError(err) => {
                format!("MySQL error: {}", err)
            }
        }
    }
}
