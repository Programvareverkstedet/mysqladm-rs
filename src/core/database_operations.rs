use anyhow::Context;
use indoc::formatdoc;
use itertools::Itertools;
use nix::unistd::User;
use serde::{Deserialize, Serialize};
use sqlx::{prelude::*, MySqlConnection};

use crate::core::{
    common::{
        create_user_group_matching_regex, get_current_unix_user, quote_identifier,
        validate_name_token, validate_ownership_by_user_prefix,
    },
    database_privilege_operations::DATABASE_PRIVILEGE_FIELDS,
};

pub async fn create_database(name: &str, conn: &mut MySqlConnection) -> anyhow::Result<()> {
    let user = get_current_unix_user()?;
    validate_database_name(name, &user)?;

    // NOTE: see the note about SQL injections in `validate_owner_of_database_name`
    sqlx::query(&format!("CREATE DATABASE {}", quote_identifier(name)))
        .execute(conn)
        .await
        .map_err(|e| {
            if e.to_string().contains("database exists") {
                anyhow::anyhow!("Database '{}' already exists", name)
            } else {
                e.into()
            }
        })?;

    Ok(())
}

pub async fn drop_database(name: &str, conn: &mut MySqlConnection) -> anyhow::Result<()> {
    let user = get_current_unix_user()?;
    validate_database_name(name, &user)?;

    // NOTE: see the note about SQL injections in `validate_owner_of_database_name`
    sqlx::query(&format!("DROP DATABASE {}", quote_identifier(name)))
        .execute(conn)
        .await
        .map_err(|e| {
            if e.to_string().contains("doesn't exist") {
                anyhow::anyhow!("Database '{}' does not exist", name)
            } else {
                e.into()
            }
        })?;

    Ok(())
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
struct DatabaseName {
    database: String,
}

pub async fn get_database_list(conn: &mut MySqlConnection) -> anyhow::Result<Vec<String>> {
    let unix_user = get_current_unix_user()?;

    let databases = sqlx::query_as::<_, DatabaseName>(
        r#"
          SELECT `SCHEMA_NAME` AS `database`
          FROM `information_schema`.`SCHEMATA`
          WHERE `SCHEMA_NAME` NOT IN ('information_schema', 'performance_schema', 'mysql', 'sys')
            AND `SCHEMA_NAME` REGEXP ?
        "#,
    )
    .bind(create_user_group_matching_regex(&unix_user))
    .fetch_all(conn)
    .await
    .context(format!(
        "Failed to get databases for user '{}'",
        unix_user.name
    ))?;

    Ok(databases.into_iter().map(|d| d.database).collect())
}

pub async fn get_databases_where_user_has_privileges(
    username: &str,
    conn: &mut MySqlConnection,
) -> anyhow::Result<Vec<String>> {
    let result = sqlx::query(
        formatdoc!(
            r#"
            SELECT `db` AS `database`
            FROM `db`
            WHERE `user` = ?
              AND ({})
        "#,
            DATABASE_PRIVILEGE_FIELDS
                .iter()
                .map(|field| format!("`{}` = 'Y'", field))
                .join(" OR "),
        )
        .as_str(),
    )
    .bind(username)
    .fetch_all(conn)
    .await?
    .into_iter()
    .map(|databases| databases.try_get::<String, _>("database").unwrap())
    .collect();

    Ok(result)
}

/// NOTE: It is very critical that this function validates the database name
///       properly. MySQL does not seem to allow for prepared statements, binding
///       the database name as a parameter to the query. This means that we have
///       to validate the database name ourselves to prevent SQL injection.
pub fn validate_database_name(name: &str, user: &User) -> anyhow::Result<()> {
    validate_name_token(name).context("Invalid database name")?;
    validate_ownership_by_user_prefix(name, user).context("Invalid database name")?;

    Ok(())
}
