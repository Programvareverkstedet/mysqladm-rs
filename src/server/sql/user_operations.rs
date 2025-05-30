use indoc::formatdoc;
use itertools::Itertools;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use sqlx::MySqlConnection;
use sqlx::prelude::*;

use crate::{
    core::{
        common::UnixUser,
        protocol::{
            CreateUserError, CreateUsersOutput, DropUserError, DropUsersOutput, ListAllUsersError,
            ListAllUsersOutput, ListUsersError, ListUsersOutput, LockUserError, LockUsersOutput,
            MySQLUser, SetPasswordError, SetPasswordOutput, UnlockUserError, UnlockUsersOutput,
        },
    },
    server::{
        common::{create_user_group_matching_regex, try_get_with_binary_fallback},
        input_sanitization::{quote_literal, validate_name, validate_ownership_by_unix_user},
    },
};

use super::database_privilege_operations::DATABASE_PRIVILEGE_FIELDS;

// NOTE: this function is unsafe because it does no input validation.
async fn unsafe_user_exists(
    db_user: &str,
    connection: &mut MySqlConnection,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        r#"
          SELECT EXISTS(
            SELECT 1
            FROM `mysql`.`user`
            WHERE `User` = ?
          )
        "#,
    )
    .bind(db_user)
    .fetch_one(connection)
    .await
    .map(|row| row.get::<bool, _>(0));

    if let Err(err) = &result {
        log::error!("Failed to check if database user exists: {:?}", err);
    }

    result
}

pub async fn create_database_users(
    db_users: Vec<MySQLUser>,
    unix_user: &UnixUser,
    connection: &mut MySqlConnection,
) -> CreateUsersOutput {
    let mut results = BTreeMap::new();

    for db_user in db_users {
        if let Err(err) = validate_name(&db_user) {
            results.insert(db_user, Err(CreateUserError::SanitizationError(err)));
            continue;
        }

        if let Err(err) = validate_ownership_by_unix_user(&db_user, unix_user) {
            results.insert(db_user, Err(CreateUserError::OwnershipError(err)));
            continue;
        }

        match unsafe_user_exists(&db_user, &mut *connection).await {
            Ok(true) => {
                results.insert(db_user, Err(CreateUserError::UserAlreadyExists));
                continue;
            }
            Err(err) => {
                results.insert(db_user, Err(CreateUserError::MySqlError(err.to_string())));
                continue;
            }
            _ => {}
        }

        let result = sqlx::query(format!("CREATE USER {}@'%'", quote_literal(&db_user),).as_str())
            .execute(&mut *connection)
            .await
            .map(|_| ())
            .map_err(|err| CreateUserError::MySqlError(err.to_string()));

        if let Err(err) = &result {
            log::error!("Failed to create database user '{}': {:?}", &db_user, err);
        }

        results.insert(db_user, result);
    }

    results
}

pub async fn drop_database_users(
    db_users: Vec<MySQLUser>,
    unix_user: &UnixUser,
    connection: &mut MySqlConnection,
) -> DropUsersOutput {
    let mut results = BTreeMap::new();

    for db_user in db_users {
        if let Err(err) = validate_name(&db_user) {
            results.insert(db_user, Err(DropUserError::SanitizationError(err)));
            continue;
        }

        if let Err(err) = validate_ownership_by_unix_user(&db_user, unix_user) {
            results.insert(db_user, Err(DropUserError::OwnershipError(err)));
            continue;
        }

        match unsafe_user_exists(&db_user, &mut *connection).await {
            Ok(false) => {
                results.insert(db_user, Err(DropUserError::UserDoesNotExist));
                continue;
            }
            Err(err) => {
                results.insert(db_user, Err(DropUserError::MySqlError(err.to_string())));
                continue;
            }
            _ => {}
        }

        let result = sqlx::query(format!("DROP USER {}@'%'", quote_literal(&db_user),).as_str())
            .execute(&mut *connection)
            .await
            .map(|_| ())
            .map_err(|err| DropUserError::MySqlError(err.to_string()));

        if let Err(err) = &result {
            log::error!("Failed to drop database user '{}': {:?}", &db_user, err);
        }

        results.insert(db_user, result);
    }

    results
}

pub async fn set_password_for_database_user(
    db_user: &MySQLUser,
    password: &str,
    unix_user: &UnixUser,
    connection: &mut MySqlConnection,
) -> SetPasswordOutput {
    if let Err(err) = validate_name(db_user) {
        return Err(SetPasswordError::SanitizationError(err));
    }

    if let Err(err) = validate_ownership_by_unix_user(db_user, unix_user) {
        return Err(SetPasswordError::OwnershipError(err));
    }

    match unsafe_user_exists(db_user, &mut *connection).await {
        Ok(false) => return Err(SetPasswordError::UserDoesNotExist),
        Err(err) => return Err(SetPasswordError::MySqlError(err.to_string())),
        _ => {}
    }

    let result = sqlx::query(
        format!(
            "ALTER USER {}@'%' IDENTIFIED BY {}",
            quote_literal(db_user),
            quote_literal(password).as_str(),
        )
        .as_str(),
    )
    .execute(&mut *connection)
    .await
    .map(|_| ())
    .map_err(|err| SetPasswordError::MySqlError(err.to_string()));

    if result.is_err() {
        log::error!(
            "Failed to set password for database user '{}': <REDACTED>",
            &db_user,
        );
    }

    result
}

// NOTE: this function is unsafe because it does no input validation.
async fn database_user_is_locked_unsafe(
    db_user: &str,
    connection: &mut MySqlConnection,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        r#"
          SELECT COALESCE(
            JSON_EXTRACT(`mysql`.`global_priv`.`priv`, "$.account_locked"),
            'false'
          ) != 'false'
          FROM `mysql`.`global_priv`
          WHERE `User` = ?
          AND `Host` = '%'
        "#,
    )
    .bind(db_user)
    .fetch_one(connection)
    .await
    .map(|row| row.get::<bool, _>(0));

    if let Err(err) = &result {
        log::error!(
            "Failed to check if database user is locked '{}': {:?}",
            &db_user,
            err
        );
    }

    result
}

pub async fn lock_database_users(
    db_users: Vec<MySQLUser>,
    unix_user: &UnixUser,
    connection: &mut MySqlConnection,
) -> LockUsersOutput {
    let mut results = BTreeMap::new();

    for db_user in db_users {
        if let Err(err) = validate_name(&db_user) {
            results.insert(db_user, Err(LockUserError::SanitizationError(err)));
            continue;
        }

        if let Err(err) = validate_ownership_by_unix_user(&db_user, unix_user) {
            results.insert(db_user, Err(LockUserError::OwnershipError(err)));
            continue;
        }

        match unsafe_user_exists(&db_user, &mut *connection).await {
            Ok(true) => {}
            Ok(false) => {
                results.insert(db_user, Err(LockUserError::UserDoesNotExist));
                continue;
            }
            Err(err) => {
                results.insert(db_user, Err(LockUserError::MySqlError(err.to_string())));
                continue;
            }
        }

        match database_user_is_locked_unsafe(&db_user, &mut *connection).await {
            Ok(false) => {}
            Ok(true) => {
                results.insert(db_user, Err(LockUserError::UserIsAlreadyLocked));
                continue;
            }
            Err(err) => {
                results.insert(db_user, Err(LockUserError::MySqlError(err.to_string())));
                continue;
            }
        }

        let result = sqlx::query(
            format!("ALTER USER {}@'%' ACCOUNT LOCK", quote_literal(&db_user),).as_str(),
        )
        .execute(&mut *connection)
        .await
        .map(|_| ())
        .map_err(|err| LockUserError::MySqlError(err.to_string()));

        if let Err(err) = &result {
            log::error!("Failed to lock database user '{}': {:?}", &db_user, err);
        }

        results.insert(db_user, result);
    }

    results
}

pub async fn unlock_database_users(
    db_users: Vec<MySQLUser>,
    unix_user: &UnixUser,
    connection: &mut MySqlConnection,
) -> UnlockUsersOutput {
    let mut results = BTreeMap::new();

    for db_user in db_users {
        if let Err(err) = validate_name(&db_user) {
            results.insert(db_user, Err(UnlockUserError::SanitizationError(err)));
            continue;
        }

        if let Err(err) = validate_ownership_by_unix_user(&db_user, unix_user) {
            results.insert(db_user, Err(UnlockUserError::OwnershipError(err)));
            continue;
        }

        match unsafe_user_exists(&db_user, &mut *connection).await {
            Ok(false) => {
                results.insert(db_user, Err(UnlockUserError::UserDoesNotExist));
                continue;
            }
            Err(err) => {
                results.insert(db_user, Err(UnlockUserError::MySqlError(err.to_string())));
                continue;
            }
            _ => {}
        }

        match database_user_is_locked_unsafe(&db_user, &mut *connection).await {
            Ok(false) => {
                results.insert(db_user, Err(UnlockUserError::UserIsAlreadyUnlocked));
                continue;
            }
            Err(err) => {
                results.insert(db_user, Err(UnlockUserError::MySqlError(err.to_string())));
                continue;
            }
            _ => {}
        }

        let result = sqlx::query(
            format!("ALTER USER {}@'%' ACCOUNT UNLOCK", quote_literal(&db_user),).as_str(),
        )
        .execute(&mut *connection)
        .await
        .map(|_| ())
        .map_err(|err| UnlockUserError::MySqlError(err.to_string()));

        if let Err(err) = &result {
            log::error!("Failed to unlock database user '{}': {:?}", &db_user, err);
        }

        results.insert(db_user, result);
    }

    results
}

/// This struct contains information about a database user.
/// This can be extended if we need more information in the future.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabaseUser {
    pub user: MySQLUser,
    #[serde(skip)]
    pub host: String,
    pub has_password: bool,
    pub is_locked: bool,
    pub databases: Vec<String>,
}

impl FromRow<'_, sqlx::mysql::MySqlRow> for DatabaseUser {
    fn from_row(row: &sqlx::mysql::MySqlRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            user: try_get_with_binary_fallback(row, "User")?.into(),
            host: try_get_with_binary_fallback(row, "Host")?,
            has_password: row.try_get("has_password")?,
            is_locked: row.try_get("is_locked")?,
            databases: Vec::new(),
        })
    }
}

const DB_USER_SELECT_STATEMENT: &str = r#"
SELECT
  `user`.`User`,
  `user`.`Host`,
  `user`.`Password` != '' OR `user`.`authentication_string` != '' AS `has_password`,
  COALESCE(
    JSON_EXTRACT(`global_priv`.`priv`, "$.account_locked"),
    'false'
  ) != 'false' AS `is_locked`
FROM `user`
JOIN `global_priv` ON
  `user`.`User` = `global_priv`.`User`
  AND `user`.`Host` = `global_priv`.`Host`
"#;

pub async fn list_database_users(
    db_users: Vec<MySQLUser>,
    unix_user: &UnixUser,
    connection: &mut MySqlConnection,
) -> ListUsersOutput {
    let mut results = BTreeMap::new();

    for db_user in db_users {
        if let Err(err) = validate_name(&db_user) {
            results.insert(db_user, Err(ListUsersError::SanitizationError(err)));
            continue;
        }

        if let Err(err) = validate_ownership_by_unix_user(&db_user, unix_user) {
            results.insert(db_user, Err(ListUsersError::OwnershipError(err)));
            continue;
        }

        let mut result = sqlx::query_as::<_, DatabaseUser>(
            &(DB_USER_SELECT_STATEMENT.to_string() + "WHERE `mysql`.`user`.`User` = ?"),
        )
        .bind(db_user.as_str())
        .fetch_optional(&mut *connection)
        .await;

        if let Err(err) = &result {
            log::error!("Failed to list database user '{}': {:?}", &db_user, err);
        }

        if let Ok(Some(user)) = result.as_mut() {
            append_databases_where_user_has_privileges(user, &mut *connection).await;
        }

        match result {
            Ok(Some(user)) => results.insert(db_user, Ok(user)),
            Ok(None) => results.insert(db_user, Err(ListUsersError::UserDoesNotExist)),
            Err(err) => results.insert(db_user, Err(ListUsersError::MySqlError(err.to_string()))),
        };
    }

    results
}

pub async fn list_all_database_users_for_unix_user(
    unix_user: &UnixUser,
    connection: &mut MySqlConnection,
) -> ListAllUsersOutput {
    let mut result = sqlx::query_as::<_, DatabaseUser>(
        &(DB_USER_SELECT_STATEMENT.to_string() + "WHERE `user`.`User` REGEXP ?"),
    )
    .bind(create_user_group_matching_regex(unix_user))
    .fetch_all(&mut *connection)
    .await
    .map_err(|err| ListAllUsersError::MySqlError(err.to_string()));

    if let Err(err) = &result {
        log::error!("Failed to list all database users: {:?}", err);
    }

    if let Ok(users) = result.as_mut() {
        for user in users {
            append_databases_where_user_has_privileges(user, &mut *connection).await;
        }
    }

    result
}

pub async fn append_databases_where_user_has_privileges(
    db_user: &mut DatabaseUser,
    connection: &mut MySqlConnection,
) {
    let database_list = sqlx::query(
        formatdoc!(
            r#"
                SELECT `Db` AS `database`
                FROM `db`
                WHERE `User` = ? AND ({})
            "#,
            DATABASE_PRIVILEGE_FIELDS
                .iter()
                .map(|field| format!("`{}` = 'Y'", field))
                .join(" OR "),
        )
        .as_str(),
    )
    .bind(db_user.user.as_str())
    .fetch_all(&mut *connection)
    .await;

    if let Err(err) = &database_list {
        log::error!(
            "Failed to list databases for user '{}': {:?}",
            &db_user.user,
            err
        );
    }

    db_user.databases = database_list
        .map(|rows| {
            rows.into_iter()
                .map(|row| try_get_with_binary_fallback(&row, "database").unwrap())
                .collect()
        })
        .unwrap_or_default();
}
