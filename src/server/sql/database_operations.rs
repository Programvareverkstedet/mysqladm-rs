use crate::{
    core::{
        common::UnixUser,
        protocol::{
            CreateDatabaseError, CreateDatabasesOutput, DropDatabaseError, DropDatabasesOutput,
            ListDatabasesError,
        },
    },
    server::{
        common::create_user_group_matching_regex,
        input_sanitization::{quote_identifier, validate_name, validate_ownership_by_unix_user},
    },
};

use sqlx::prelude::*;

use sqlx::MySqlConnection;
use std::collections::BTreeMap;

// NOTE: this function is unsafe because it does no input validation.
pub(super) async fn unsafe_database_exists(
    database_name: &str,
    connection: &mut MySqlConnection,
) -> Result<bool, sqlx::Error> {
    let result =
        sqlx::query("SELECT SCHEMA_NAME FROM information_schema.SCHEMATA WHERE SCHEMA_NAME = ?")
            .bind(database_name)
            .fetch_optional(connection)
            .await?;

    Ok(result.is_some())
}

pub async fn create_databases(
    database_names: Vec<String>,
    unix_user: &UnixUser,
    connection: &mut MySqlConnection,
) -> CreateDatabasesOutput {
    let mut results = BTreeMap::new();

    for database_name in database_names {
        if let Err(err) = validate_name(&database_name) {
            results.insert(
                database_name.clone(),
                Err(CreateDatabaseError::SanitizationError(err)),
            );
            continue;
        }

        if let Err(err) = validate_ownership_by_unix_user(&database_name, unix_user) {
            results.insert(
                database_name.clone(),
                Err(CreateDatabaseError::OwnershipError(err)),
            );
            continue;
        }

        match unsafe_database_exists(&database_name, &mut *connection).await {
            Ok(true) => {
                results.insert(
                    database_name.clone(),
                    Err(CreateDatabaseError::DatabaseAlreadyExists),
                );
                continue;
            }
            Err(err) => {
                results.insert(
                    database_name.clone(),
                    Err(CreateDatabaseError::MySqlError(err.to_string())),
                );
                continue;
            }
            _ => {}
        }

        let result =
            sqlx::query(format!("CREATE DATABASE {}", quote_identifier(&database_name)).as_str())
                .execute(&mut *connection)
                .await
                .map(|_| ())
                .map_err(|err| CreateDatabaseError::MySqlError(err.to_string()));

        results.insert(database_name, result);
    }

    results
}

pub async fn drop_databases(
    database_names: Vec<String>,
    unix_user: &UnixUser,
    connection: &mut MySqlConnection,
) -> DropDatabasesOutput {
    let mut results = BTreeMap::new();

    for database_name in database_names {
        if let Err(err) = validate_name(&database_name) {
            results.insert(
                database_name.clone(),
                Err(DropDatabaseError::SanitizationError(err)),
            );
            continue;
        }

        if let Err(err) = validate_ownership_by_unix_user(&database_name, unix_user) {
            results.insert(
                database_name.clone(),
                Err(DropDatabaseError::OwnershipError(err)),
            );
            continue;
        }

        match unsafe_database_exists(&database_name, &mut *connection).await {
            Ok(false) => {
                results.insert(
                    database_name.clone(),
                    Err(DropDatabaseError::DatabaseDoesNotExist),
                );
                continue;
            }
            Err(err) => {
                results.insert(
                    database_name.clone(),
                    Err(DropDatabaseError::MySqlError(err.to_string())),
                );
                continue;
            }
            _ => {}
        }

        let result =
            sqlx::query(format!("DROP DATABASE {}", quote_identifier(&database_name)).as_str())
                .execute(&mut *connection)
                .await
                .map(|_| ())
                .map_err(|err| DropDatabaseError::MySqlError(err.to_string()));

        results.insert(database_name, result);
    }

    results
}

pub async fn list_databases_for_user(
    unix_user: &UnixUser,
    connection: &mut MySqlConnection,
) -> Result<Vec<String>, ListDatabasesError> {
    sqlx::query(
        r#"
          SELECT `SCHEMA_NAME` AS `database`
          FROM `information_schema`.`SCHEMATA`
          WHERE `SCHEMA_NAME` NOT IN ('information_schema', 'performance_schema', 'mysql', 'sys')
            AND `SCHEMA_NAME` REGEXP ?
        "#,
    )
    .bind(create_user_group_matching_regex(unix_user))
    .fetch_all(connection)
    .await
    .and_then(|rows| {
        rows.into_iter()
            .map(|row| row.try_get::<String, _>("database"))
            .collect::<Result<Vec<String>, sqlx::Error>>()
    })
    .map_err(|err| ListDatabasesError::MySqlError(err.to_string()))
}
