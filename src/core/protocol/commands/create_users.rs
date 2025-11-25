use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::core::{
    protocol::request_validation::{DbOrUser, NameValidationError, OwnerValidationError},
    types::MySQLUser,
};

pub type CreateUsersRequest = Vec<MySQLUser>;

pub type CreateUsersResponse = BTreeMap<MySQLUser, Result<(), CreateUserError>>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CreateUserError {
    SanitizationError(NameValidationError),
    OwnershipError(OwnerValidationError),
    UserAlreadyExists,
    MySqlError(String),
}

pub fn print_create_users_output_status(output: &CreateUsersResponse) {
    for (username, result) in output {
        match result {
            Ok(()) => {
                println!("User '{}' created successfully.", username);
            }
            Err(err) => {
                println!("{}", err.to_error_message(username));
                println!("Skipping...");
            }
        }
        println!();
    }
}

pub fn print_create_users_output_status_json(output: &CreateUsersResponse) {
    let value = output
        .iter()
        .map(|(name, result)| match result {
            Ok(()) => (name.to_string(), json!({ "status": "success" })),
            Err(err) => (
                name.to_string(),
                json!({
                  "status": "error",
                  "error": err.to_error_message(name),
                }),
            ),
        })
        .collect::<serde_json::Map<_, _>>();
    println!(
        "{}",
        serde_json::to_string_pretty(&value)
            .unwrap_or("Failed to serialize result to JSON".to_string())
    );
}

impl CreateUserError {
    pub fn to_error_message(&self, username: &MySQLUser) -> String {
        match self {
            CreateUserError::SanitizationError(err) => {
                err.to_error_message(username, DbOrUser::User)
            }
            CreateUserError::OwnershipError(err) => err.to_error_message(username, DbOrUser::User),
            CreateUserError::UserAlreadyExists => {
                format!("User '{}' already exists.", username)
            }
            CreateUserError::MySqlError(err) => {
                format!("MySQL error: {}", err)
            }
        }
    }
}
