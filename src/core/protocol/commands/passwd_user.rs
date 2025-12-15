use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::core::{
    protocol::request_validation::AuthorizationError,
    types::{DbOrUser, MySQLUser},
};

pub type SetUserPasswordRequest = (MySQLUser, String);

pub type SetUserPasswordResponse = Result<(), SetPasswordError>;

#[derive(Error, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SetPasswordError {
    #[error("Authorization error: {0}")]
    AuthorizationError(#[from] AuthorizationError),

    #[error("User does not exist")]
    UserDoesNotExist,

    #[error("MySQL error: {0}")]
    MySqlError(String),
}

pub fn print_set_password_output_status(output: &SetUserPasswordResponse, username: &MySQLUser) {
    match output {
        Ok(()) => {
            println!("Password for user '{}' set successfully.", username);
        }
        Err(err) => {
            println!("{}", err.to_error_message(username));
            println!("Skipping...");
        }
    }
}

impl SetPasswordError {
    pub fn to_error_message(&self, username: &MySQLUser) -> String {
        match self {
            SetPasswordError::AuthorizationError(err) => {
                err.to_error_message(DbOrUser::User(username.clone()))
            }
            SetPasswordError::UserDoesNotExist => {
                format!("User '{}' does not exist.", username)
            }
            SetPasswordError::MySqlError(err) => {
                format!("MySQL error: {}", err)
            }
        }
    }

    #[allow(dead_code)]
    pub fn error_type(&self) -> String {
        match self {
            SetPasswordError::AuthorizationError(err) => err.error_type(),
            SetPasswordError::UserDoesNotExist => "user-does-not-exist".to_string(),
            SetPasswordError::MySqlError(_) => "mysql-error".to_string(),
        }
    }
}
