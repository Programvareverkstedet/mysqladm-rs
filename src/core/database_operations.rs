use anyhow::Context;
use indoc::indoc;
use itertools::Itertools;
use nix::unistd::User;
use serde::{Deserialize, Serialize};
use sqlx::{prelude::*, MySqlConnection};

use super::common::{
    get_current_unix_user, get_unix_groups, quote_identifier, validate_prefix_for_user,
};

pub async fn create_database(name: &str, conn: &mut MySqlConnection) -> anyhow::Result<()> {
    let user = get_current_unix_user()?;
    validate_ownership_of_database_name(name, &user)?;

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
    validate_ownership_of_database_name(name, &user)?;

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
    let unix_groups = get_unix_groups(&unix_user)?
        .into_iter()
        .map(|g| g.name)
        .collect::<Vec<_>>();

    let databases = sqlx::query_as::<_, DatabaseName>(
        r#"
          SELECT `SCHEMA_NAME` AS `database`
          FROM `information_schema`.`SCHEMATA`
          WHERE `SCHEMA_NAME` NOT IN ('information_schema', 'performance_schema', 'mysql', 'sys')
            AND `SCHEMA_NAME` REGEXP ?
        "#,
    )
    .bind(format!(
        "({}|{})_.+",
        unix_user.name,
        unix_groups.iter().map(|g| g.to_string()).join("|")
    ))
    .fetch_all(conn)
    .await
    .context(format!(
        "Failed to get databases for user '{}'",
        unix_user.name
    ))?;

    Ok(databases.into_iter().map(|d| d.database).collect())
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct DatabasePrivileges {
    pub db: String,
    pub user: String,
    pub select_priv: String,
    pub insert_priv: String,
    pub update_priv: String,
    pub delete_priv: String,
    pub create_priv: String,
    pub drop_priv: String,
    pub alter_priv: String,
    pub index_priv: String,
    pub create_tmp_table_priv: String,
    pub lock_tables_priv: String,
    pub references_priv: String,
}

pub const HUMAN_READABLE_DATABASE_PRIVILEGE_NAMES: [(&str, &str); 13] = [
    ("Database", "db"),
    ("User", "user"),
    ("Select", "select_priv"),
    ("Insert", "insert_priv"),
    ("Update", "update_priv"),
    ("Delete", "delete_priv"),
    ("Create", "create_priv"),
    ("Drop", "drop_priv"),
    ("Alter", "alter_priv"),
    ("Index", "index_priv"),
    ("Temp", "create_tmp_table_priv"),
    ("Lock", "lock_tables_priv"),
    ("References", "references_priv"),
];

pub async fn get_database_privileges(
    database_name: &str,
    conn: &mut MySqlConnection,
) -> anyhow::Result<Vec<DatabasePrivileges>> {
    let unix_user = get_current_unix_user()?;
    validate_ownership_of_database_name(database_name, &unix_user)?;

    let result = sqlx::query_as::<_, DatabasePrivileges>(&format!(
        "SELECT {} FROM `db` WHERE `db` = ?",
        HUMAN_READABLE_DATABASE_PRIVILEGE_NAMES
            .iter()
            .map(|(_, prop)| quote_identifier(prop))
            .join(","),
    ))
    .bind(database_name)
    .fetch_all(conn)
    .await
    .context("Failed to show database")?;

    Ok(result)
}

pub async fn get_all_database_privileges(
    conn: &mut MySqlConnection,
) -> anyhow::Result<Vec<DatabasePrivileges>> {
    let unix_user = get_current_unix_user()?;
    let unix_groups = get_unix_groups(&unix_user)?
        .into_iter()
        .map(|g| g.name)
        .collect::<Vec<_>>();

    let result = sqlx::query_as::<_, DatabasePrivileges>(&format!(
        indoc! {r#"
          SELECT {} FROM `db` WHERE `db` IN
          (SELECT DISTINCT `SCHEMA_NAME` AS `database`
            FROM `information_schema`.`SCHEMATA`
            WHERE `SCHEMA_NAME` NOT IN ('information_schema', 'performance_schema', 'mysql', 'sys')
              AND `SCHEMA_NAME` REGEXP ?)
        "#},
        HUMAN_READABLE_DATABASE_PRIVILEGE_NAMES
            .iter()
            .map(|(_, prop)| format!("`{}`", prop))
            .join(","),
    ))
    .bind(format!(
        "({}|{})_.+",
        unix_user.name,
        unix_groups.iter().map(|g| g.to_string()).join("|")
    ))
    .fetch_all(conn)
    .await
    .context("Failed to show databases")?;
    Ok(result)
}

/// NOTE: It is very critical that this function validates the database name
///       properly. MySQL does not seem to allow for prepared statements, binding
///       the database name as a parameter to the query. This means that we have
///       to validate the database name ourselves to prevent SQL injection.
pub fn validate_ownership_of_database_name(name: &str, user: &User) -> anyhow::Result<()> {
    if name.contains(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-') {
        anyhow::bail!(
            indoc! {r#"
              Database name '{}' contains invalid characters.
              Only A-Z, a-z, 0-9, _ (underscore) and - (dash) permitted.
            "#},
            name
        );
    }

    if name.len() > 64 {
        anyhow::bail!(
            indoc! {r#"
              Database name '{}' is too long.
              Maximum length is 64 characters.
            "#},
            name
        );
    }

    validate_prefix_for_user(name, user).context("Invalid database name")?;

    Ok(())
}
