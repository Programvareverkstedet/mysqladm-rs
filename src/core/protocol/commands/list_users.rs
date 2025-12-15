use std::collections::BTreeMap;

use prettytable::Table;
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

use crate::{
    core::{
        protocol::request_validation::AuthorizationError,
        types::{DbOrUser, MySQLUser},
    },
    server::sql::user_operations::DatabaseUser,
};

pub type ListUsersRequest = Option<Vec<MySQLUser>>;

pub type ListUsersResponse = BTreeMap<MySQLUser, Result<DatabaseUser, ListUsersError>>;

#[derive(Error, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ListUsersError {
    #[error("Authorization error: {0}")]
    AuthorizationError(#[from] AuthorizationError),

    #[error("User does not exist")]
    UserDoesNotExist,

    #[error("MySQL error: {0}")]
    MySqlError(String),
}

pub fn print_list_users_output_status(output: &ListUsersResponse) {
    let mut final_user_list: Vec<&DatabaseUser> = Vec::new();
    for (db_name, db_result) in output {
        match db_result {
            Ok(db_row) => final_user_list.push(db_row),
            Err(err) => {
                eprintln!("{}", err.to_error_message(db_name));
                eprintln!("Skipping...");
            }
        }
    }

    if final_user_list.is_empty() {
        println!("No users to show.");
    } else {
        let mut table = Table::new();
        table.add_row(row![
            "User",
            "Password is set",
            "Locked",
            "Databases where user has privileges"
        ]);
        for user in final_user_list {
            table.add_row(row![
                user.user,
                user.has_password,
                user.is_locked,
                user.databases.join("\n")
            ]);
        }
        table.printstd();
    }
}

pub fn print_list_users_output_status_json(output: &ListUsersResponse) {
    let value = output
        .iter()
        .map(|(name, result)| match result {
            Ok(row) => (
                name.to_string(),
                json!({
                  "status": "success",
                  "value": {
                    "user": row.user,
                    "has_password": row.has_password,
                    "is_locked": row.is_locked,
                    "databases": row.databases,
                  }
                }),
            ),
            Err(err) => (
                name.to_string(),
                json!({
                  "status": "error",
                  "type": err.error_type(),
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

impl ListUsersError {
    pub fn to_error_message(&self, username: &MySQLUser) -> String {
        match self {
            ListUsersError::AuthorizationError(err) => {
                err.to_error_message(DbOrUser::User(username.clone()))
            }
            ListUsersError::UserDoesNotExist => {
                format!("User '{}' does not exist.", username)
            }
            ListUsersError::MySqlError(err) => {
                format!("MySQL error: {}", err)
            }
        }
    }

    pub fn error_type(&self) -> String {
        match self {
            ListUsersError::AuthorizationError(err) => err.error_type(),
            ListUsersError::UserDoesNotExist => "user-does-not-exist".to_string(),
            ListUsersError::MySqlError(_) => "mysql-error".to_string(),
        }
    }
}
