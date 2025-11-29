use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::core::{
    database_privileges::{DatabasePrivilegeRow, DatabasePrivilegeRowDiff, DatabasePrivilegesDiff},
    protocol::request_validation::{NameValidationError, OwnerValidationError},
    types::{DbOrUser, MySQLDatabase, MySQLUser},
};

pub type ModifyPrivilegesRequest = BTreeSet<DatabasePrivilegesDiff>;

pub type ModifyPrivilegesResponse =
    BTreeMap<(MySQLDatabase, MySQLUser), Result<(), ModifyDatabasePrivilegesError>>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ModifyDatabasePrivilegesError {
    DatabaseSanitizationError(NameValidationError),
    DatabaseOwnershipError(OwnerValidationError),
    UserSanitizationError(NameValidationError),
    UserOwnershipError(OwnerValidationError),
    DatabaseDoesNotExist,
    DiffDoesNotApply(DiffDoesNotApplyError),
    MySqlError(String),
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DiffDoesNotApplyError {
    RowAlreadyExists(MySQLDatabase, MySQLUser),
    RowDoesNotExist(MySQLDatabase, MySQLUser),
    RowPrivilegeChangeDoesNotApply(DatabasePrivilegeRowDiff, DatabasePrivilegeRow),
}

pub fn print_modify_database_privileges_output_status(output: &ModifyPrivilegesResponse) {
    for ((database_name, username), result) in output {
        match result {
            Ok(()) => {
                println!(
                    "Privileges for user '{}' on database '{}' modified successfully.",
                    username, database_name
                );
            }
            Err(err) => {
                println!("{}", err.to_error_message(database_name, username));
                println!("Skipping...");
            }
        }
        println!();
    }
}

impl ModifyDatabasePrivilegesError {
    pub fn to_error_message(&self, database_name: &MySQLDatabase, username: &MySQLUser) -> String {
        match self {
            ModifyDatabasePrivilegesError::DatabaseSanitizationError(err) => {
                err.to_error_message(DbOrUser::Database(database_name.clone()))
            }
            ModifyDatabasePrivilegesError::DatabaseOwnershipError(err) => {
                err.to_error_message(DbOrUser::Database(database_name.clone()))
            }
            ModifyDatabasePrivilegesError::UserSanitizationError(err) => {
                err.to_error_message(DbOrUser::User(username.clone()))
            }
            ModifyDatabasePrivilegesError::UserOwnershipError(err) => {
                err.to_error_message(DbOrUser::User(username.clone()))
            }
            ModifyDatabasePrivilegesError::DatabaseDoesNotExist => {
                format!("Database '{}' does not exist.", database_name)
            }
            ModifyDatabasePrivilegesError::DiffDoesNotApply(diff) => {
                format!(
                    "Could not apply privilege change:\n{}",
                    diff.to_error_message()
                )
            }
            ModifyDatabasePrivilegesError::MySqlError(err) => {
                format!("MySQL error: {}", err)
            }
        }
    }
}

impl DiffDoesNotApplyError {
    pub fn to_error_message(&self) -> String {
        match self {
            DiffDoesNotApplyError::RowAlreadyExists(database_name, username) => {
                format!(
                    "Privileges for user '{}' on database '{}' already exist.",
                    username, database_name
                )
            }
            DiffDoesNotApplyError::RowDoesNotExist(database_name, username) => {
                format!(
                    "Privileges for user '{}' on database '{}' do not exist.",
                    username, database_name
                )
            }
            DiffDoesNotApplyError::RowPrivilegeChangeDoesNotApply(diff, row) => {
                format!(
                    "Could not apply privilege change {:?} to row {:?}",
                    diff, row
                )
            }
        }
    }
}
