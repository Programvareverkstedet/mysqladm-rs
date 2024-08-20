use std::collections::BTreeMap;

use indoc::indoc;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    core::{common::UnixUser, database_privileges::DatabasePrivilegeRowDiff},
    server::sql::{
        database_operations::DatabaseRow, database_privilege_operations::DatabasePrivilegeRow,
        user_operations::DatabaseUser,
    },
};

use super::{MySQLDatabase, MySQLUser};

/// This enum is used to differentiate between database and user operations.
/// Their output are very similar, but there are slight differences in the words used.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum DbOrUser {
    Database,
    User,
}

impl DbOrUser {
    pub fn lowercased(&self) -> &'static str {
        match self {
            DbOrUser::Database => "database",
            DbOrUser::User => "user",
        }
    }

    pub fn capitalized(&self) -> &'static str {
        match self {
            DbOrUser::Database => "Database",
            DbOrUser::User => "User",
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum NameValidationError {
    EmptyString,
    InvalidCharacters,
    TooLong,
}

impl NameValidationError {
    pub fn to_error_message(self, name: &str, db_or_user: DbOrUser) -> String {
        match self {
            NameValidationError::EmptyString => {
                format!("{} name cannot be empty.", db_or_user.capitalized()).to_owned()
            }
            NameValidationError::TooLong => format!(
                "{} is too long. Maximum length is 64 characters.",
                db_or_user.capitalized()
            )
            .to_owned(),
            NameValidationError::InvalidCharacters => format!(
                indoc! {r#"
                  Invalid characters in {} name: '{}'

                  Only A-Z, a-z, 0-9, _ (underscore) and - (dash) are permitted.
                "#},
                db_or_user.lowercased(),
                name
            )
            .to_owned(),
        }
    }
}

impl OwnerValidationError {
    pub fn to_error_message(self, name: &str, db_or_user: DbOrUser) -> String {
        let user = UnixUser::from_enviroment();

        let UnixUser { username, groups } = user.unwrap_or(UnixUser {
            username: "???".to_string(),
            groups: vec![],
        });

        match self {
            OwnerValidationError::NoMatch => format!(
                indoc! {r#"
                  Invalid {} name prefix: '{}' does not match your username or any of your groups.
                  Are you sure you are allowed to create {} names with this prefix?
                  The format should be: <prefix>_<{} name>

                  Allowed prefixes:
                    - {}
                  {}
                "#},
                db_or_user.lowercased(),
                name,
                db_or_user.lowercased(),
                db_or_user.lowercased(),
                username,
                groups
                    .iter()
                    .map(|g| format!("  - {}", g))
                    .sorted()
                    .join("\n"),
            )
            .to_owned(),
            OwnerValidationError::StringEmpty => format!(
                "'{}' is not a valid {} name.",
                name,
                db_or_user.lowercased()
            )
            .to_string(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum OwnerValidationError {
    // The name is valid, but none of the given prefixes matched the name
    NoMatch,

    // The name is empty, which is invalid
    StringEmpty,
}

pub type CreateDatabasesOutput = BTreeMap<MySQLDatabase, Result<(), CreateDatabaseError>>;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CreateDatabaseError {
    SanitizationError(NameValidationError),
    OwnershipError(OwnerValidationError),
    DatabaseAlreadyExists,
    MySqlError(String),
}

pub fn print_create_databases_output_status(output: &CreateDatabasesOutput) {
    for (database_name, result) in output {
        match result {
            Ok(()) => {
                println!("Database '{}' created successfully.", database_name);
            }
            Err(err) => {
                println!("{}", err.to_error_message(database_name));
                println!("Skipping...");
            }
        }
        println!();
    }
}

pub fn print_create_databases_output_status_json(output: &CreateDatabasesOutput) {
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

impl CreateDatabaseError {
    pub fn to_error_message(&self, database_name: &MySQLDatabase) -> String {
        match self {
            CreateDatabaseError::SanitizationError(err) => {
                err.to_error_message(database_name, DbOrUser::Database)
            }
            CreateDatabaseError::OwnershipError(err) => {
                err.to_error_message(database_name, DbOrUser::Database)
            }
            CreateDatabaseError::DatabaseAlreadyExists => {
                format!("Database {} already exists.", database_name)
            }
            CreateDatabaseError::MySqlError(err) => {
                format!("MySQL error: {}", err)
            }
        }
    }
}

pub type DropDatabasesOutput = BTreeMap<MySQLDatabase, Result<(), DropDatabaseError>>;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DropDatabaseError {
    SanitizationError(NameValidationError),
    OwnershipError(OwnerValidationError),
    DatabaseDoesNotExist,
    MySqlError(String),
}

pub fn print_drop_databases_output_status(output: &DropDatabasesOutput) {
    for (database_name, result) in output {
        match result {
            Ok(()) => {
                println!(
                    "Database '{}' dropped successfully.",
                    database_name.as_str()
                );
            }
            Err(err) => {
                println!("{}", err.to_error_message(database_name));
                println!("Skipping...");
            }
        }
        println!();
    }
}

pub fn print_drop_databases_output_status_json(output: &DropDatabasesOutput) {
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

impl DropDatabaseError {
    pub fn to_error_message(&self, database_name: &MySQLDatabase) -> String {
        match self {
            DropDatabaseError::SanitizationError(err) => {
                err.to_error_message(database_name, DbOrUser::Database)
            }
            DropDatabaseError::OwnershipError(err) => {
                err.to_error_message(database_name, DbOrUser::Database)
            }
            DropDatabaseError::DatabaseDoesNotExist => {
                format!("Database {} does not exist.", database_name)
            }
            DropDatabaseError::MySqlError(err) => {
                format!("MySQL error: {}", err)
            }
        }
    }
}

pub type ListDatabasesOutput = BTreeMap<MySQLDatabase, Result<DatabaseRow, ListDatabasesError>>;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ListDatabasesError {
    SanitizationError(NameValidationError),
    OwnershipError(OwnerValidationError),
    DatabaseDoesNotExist,
    MySqlError(String),
}

impl ListDatabasesError {
    pub fn to_error_message(&self, database_name: &MySQLDatabase) -> String {
        match self {
            ListDatabasesError::SanitizationError(err) => {
                err.to_error_message(database_name, DbOrUser::Database)
            }
            ListDatabasesError::OwnershipError(err) => {
                err.to_error_message(database_name, DbOrUser::Database)
            }
            ListDatabasesError::DatabaseDoesNotExist => {
                format!("Database '{}' does not exist.", database_name)
            }
            ListDatabasesError::MySqlError(err) => {
                format!("MySQL error: {}", err)
            }
        }
    }
}

pub type ListAllDatabasesOutput = Result<Vec<DatabaseRow>, ListAllDatabasesError>;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ListAllDatabasesError {
    MySqlError(String),
}

impl ListAllDatabasesError {
    pub fn to_error_message(&self) -> String {
        match self {
            ListAllDatabasesError::MySqlError(err) => format!("MySQL error: {}", err),
        }
    }
}

// TODO: merge all rows into a single collection.
//       they already contain which database they belong to.
//       no need to index by database name.

pub type GetDatabasesPrivilegeData =
    BTreeMap<MySQLDatabase, Result<Vec<DatabasePrivilegeRow>, GetDatabasesPrivilegeDataError>>;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GetDatabasesPrivilegeDataError {
    SanitizationError(NameValidationError),
    OwnershipError(OwnerValidationError),
    DatabaseDoesNotExist,
    MySqlError(String),
}

impl GetDatabasesPrivilegeDataError {
    pub fn to_error_message(&self, database_name: &MySQLDatabase) -> String {
        match self {
            GetDatabasesPrivilegeDataError::SanitizationError(err) => {
                err.to_error_message(database_name, DbOrUser::Database)
            }
            GetDatabasesPrivilegeDataError::OwnershipError(err) => {
                err.to_error_message(database_name, DbOrUser::Database)
            }
            GetDatabasesPrivilegeDataError::DatabaseDoesNotExist => {
                format!("Database '{}' does not exist.", database_name)
            }
            GetDatabasesPrivilegeDataError::MySqlError(err) => {
                format!("MySQL error: {}", err)
            }
        }
    }
}

pub type GetAllDatabasesPrivilegeData =
    Result<Vec<DatabasePrivilegeRow>, GetAllDatabasesPrivilegeDataError>;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GetAllDatabasesPrivilegeDataError {
    MySqlError(String),
}

impl GetAllDatabasesPrivilegeDataError {
    pub fn to_error_message(&self) -> String {
        match self {
            GetAllDatabasesPrivilegeDataError::MySqlError(err) => format!("MySQL error: {}", err),
        }
    }
}

pub type ModifyDatabasePrivilegesOutput =
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

pub fn print_modify_database_privileges_output_status(output: &ModifyDatabasePrivilegesOutput) {
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
                err.to_error_message(database_name, DbOrUser::Database)
            }
            ModifyDatabasePrivilegesError::DatabaseOwnershipError(err) => {
                err.to_error_message(database_name, DbOrUser::Database)
            }
            ModifyDatabasePrivilegesError::UserSanitizationError(err) => {
                err.to_error_message(username, DbOrUser::User)
            }
            ModifyDatabasePrivilegesError::UserOwnershipError(err) => {
                err.to_error_message(username, DbOrUser::User)
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

pub type CreateUsersOutput = BTreeMap<MySQLUser, Result<(), CreateUserError>>;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CreateUserError {
    SanitizationError(NameValidationError),
    OwnershipError(OwnerValidationError),
    UserAlreadyExists,
    MySqlError(String),
}

pub fn print_create_users_output_status(output: &CreateUsersOutput) {
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

pub fn print_create_users_output_status_json(output: &CreateUsersOutput) {
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

pub type DropUsersOutput = BTreeMap<MySQLUser, Result<(), DropUserError>>;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DropUserError {
    SanitizationError(NameValidationError),
    OwnershipError(OwnerValidationError),
    UserDoesNotExist,
    MySqlError(String),
}

pub fn print_drop_users_output_status(output: &DropUsersOutput) {
    for (username, result) in output {
        match result {
            Ok(()) => {
                println!("User '{}' dropped successfully.", username);
            }
            Err(err) => {
                println!("{}", err.to_error_message(username));
                println!("Skipping...");
            }
        }
        println!();
    }
}

pub fn print_drop_users_output_status_json(output: &DropUsersOutput) {
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

impl DropUserError {
    pub fn to_error_message(&self, username: &MySQLUser) -> String {
        match self {
            DropUserError::SanitizationError(err) => err.to_error_message(username, DbOrUser::User),
            DropUserError::OwnershipError(err) => err.to_error_message(username, DbOrUser::User),
            DropUserError::UserDoesNotExist => {
                format!("User '{}' does not exist.", username)
            }
            DropUserError::MySqlError(err) => {
                format!("MySQL error: {}", err)
            }
        }
    }
}

pub type SetPasswordOutput = Result<(), SetPasswordError>;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SetPasswordError {
    SanitizationError(NameValidationError),
    OwnershipError(OwnerValidationError),
    UserDoesNotExist,
    MySqlError(String),
}

pub fn print_set_password_output_status(output: &SetPasswordOutput, username: &MySQLUser) {
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
            SetPasswordError::SanitizationError(err) => {
                err.to_error_message(username, DbOrUser::User)
            }
            SetPasswordError::OwnershipError(err) => err.to_error_message(username, DbOrUser::User),
            SetPasswordError::UserDoesNotExist => {
                format!("User '{}' does not exist.", username)
            }
            SetPasswordError::MySqlError(err) => {
                format!("MySQL error: {}", err)
            }
        }
    }
}

pub type LockUsersOutput = BTreeMap<MySQLUser, Result<(), LockUserError>>;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LockUserError {
    SanitizationError(NameValidationError),
    OwnershipError(OwnerValidationError),
    UserDoesNotExist,
    UserIsAlreadyLocked,
    MySqlError(String),
}

pub fn print_lock_users_output_status(output: &LockUsersOutput) {
    for (username, result) in output {
        match result {
            Ok(()) => {
                println!("User '{}' locked successfully.", username);
            }
            Err(err) => {
                println!("{}", err.to_error_message(username));
                println!("Skipping...");
            }
        }
        println!();
    }
}

pub fn print_lock_users_output_status_json(output: &LockUsersOutput) {
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

impl LockUserError {
    pub fn to_error_message(&self, username: &MySQLUser) -> String {
        match self {
            LockUserError::SanitizationError(err) => err.to_error_message(username, DbOrUser::User),
            LockUserError::OwnershipError(err) => err.to_error_message(username, DbOrUser::User),
            LockUserError::UserDoesNotExist => {
                format!("User '{}' does not exist.", username)
            }
            LockUserError::UserIsAlreadyLocked => {
                format!("User '{}' is already locked.", username)
            }
            LockUserError::MySqlError(err) => {
                format!("MySQL error: {}", err)
            }
        }
    }
}

pub type UnlockUsersOutput = BTreeMap<MySQLUser, Result<(), UnlockUserError>>;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UnlockUserError {
    SanitizationError(NameValidationError),
    OwnershipError(OwnerValidationError),
    UserDoesNotExist,
    UserIsAlreadyUnlocked,
    MySqlError(String),
}

pub fn print_unlock_users_output_status(output: &UnlockUsersOutput) {
    for (username, result) in output {
        match result {
            Ok(()) => {
                println!("User '{}' unlocked successfully.", username);
            }
            Err(err) => {
                println!("{}", err.to_error_message(username));
                println!("Skipping...");
            }
        }
        println!();
    }
}

pub fn print_unlock_users_output_status_json(output: &UnlockUsersOutput) {
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

impl UnlockUserError {
    pub fn to_error_message(&self, username: &MySQLUser) -> String {
        match self {
            UnlockUserError::SanitizationError(err) => {
                err.to_error_message(username, DbOrUser::User)
            }
            UnlockUserError::OwnershipError(err) => err.to_error_message(username, DbOrUser::User),
            UnlockUserError::UserDoesNotExist => {
                format!("User '{}' does not exist.", username)
            }
            UnlockUserError::UserIsAlreadyUnlocked => {
                format!("User '{}' is already unlocked.", username)
            }
            UnlockUserError::MySqlError(err) => {
                format!("MySQL error: {}", err)
            }
        }
    }
}

pub type ListUsersOutput = BTreeMap<MySQLUser, Result<DatabaseUser, ListUsersError>>;
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

pub type ListAllUsersOutput = Result<Vec<DatabaseUser>, ListAllUsersError>;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ListAllUsersError {
    MySqlError(String),
}

impl ListAllUsersError {
    pub fn to_error_message(&self) -> String {
        match self {
            ListAllUsersError::MySqlError(err) => format!("MySQL error: {}", err),
        }
    }
}
