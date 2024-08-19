use std::collections::BTreeMap;

use sqlx::prelude::*;
use sqlx::MySqlConnection;

use serde::{Deserialize, Serialize};

use crate::{
    core::{
        common::UnixUser,
        protocol::{
            CreateDatabaseError, CreateDatabasesOutput, DropDatabaseError, DropDatabasesOutput,
            ListAllDatabasesError, ListAllDatabasesOutput, ListDatabasesError, ListDatabasesOutput,
        },
    },
    server::{
        common::create_user_group_matching_regex,
        input_sanitization::{quote_identifier, validate_name, validate_ownership_by_unix_user},
    },
};

// NOTE: this function is unsafe because it does no input validation.
pub(super) async fn unsafe_database_exists(
    database_name: &str,
    connection: &mut MySqlConnection,
) -> Result<bool, sqlx::Error> {
    let result =
        sqlx::query("SELECT SCHEMA_NAME FROM information_schema.SCHEMATA WHERE SCHEMA_NAME = ?")
            .bind(database_name)
            .fetch_optional(connection)
            .await;

    if let Err(err) = &result {
        log::error!(
            "Failed to check if database '{}' exists: {:?}",
            &database_name,
            err
        );
    }

    Ok(result?.is_some())
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

        if let Err(err) = &result {
            log::error!("Failed to create database '{}': {:?}", &database_name, err);
        }

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

        if let Err(err) = &result {
            log::error!("Failed to drop database '{}': {:?}", &database_name, err);
        }

        results.insert(database_name, result);
    }

    results
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct DatabaseRow {
    pub database: String,
}

pub async fn list_databases(
    database_names: Vec<String>,
    unix_user: &UnixUser,
    connection: &mut MySqlConnection,
) -> ListDatabasesOutput {
    let mut results = BTreeMap::new();

    for database_name in database_names {
        if let Err(err) = validate_name(&database_name) {
            results.insert(
                database_name.clone(),
                Err(ListDatabasesError::SanitizationError(err)),
            );
            continue;
        }

        if let Err(err) = validate_ownership_by_unix_user(&database_name, unix_user) {
            results.insert(
                database_name.clone(),
                Err(ListDatabasesError::OwnershipError(err)),
            );
            continue;
        }

        let result = sqlx::query_as::<_, DatabaseRow>(
            r#"
          SELECT `SCHEMA_NAME` AS `database`
          FROM `information_schema`.`SCHEMATA`
          WHERE `SCHEMA_NAME` = ?
        "#,
        )
        .bind(&database_name)
        .fetch_optional(&mut *connection)
        .await
        .map_err(|err| ListDatabasesError::MySqlError(err.to_string()))
        .and_then(|database| {
            database
                .map(Ok)
                .unwrap_or_else(|| Err(ListDatabasesError::DatabaseDoesNotExist))
        });

        if let Err(err) = &result {
            log::error!("Failed to list database '{}': {:?}", &database_name, err);
        }

        results.insert(database_name, result);
    }

    results
}

pub async fn list_all_databases_for_user(
    unix_user: &UnixUser,
    connection: &mut MySqlConnection,
) -> ListAllDatabasesOutput {
    let result = sqlx::query_as::<_, DatabaseRow>(
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
    .map_err(|err| ListAllDatabasesError::MySqlError(err.to_string()));

    if let Err(err) = &result {
        log::error!(
            "Failed to list databases for user '{}': {:?}",
            unix_user.username,
            err
        );
    }

    result
}
