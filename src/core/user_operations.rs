use anyhow::Context;
use nix::unistd::User;
use serde::{Deserialize, Serialize};
use sqlx::{prelude::*, MySqlConnection};

use crate::core::common::quote_literal;

use super::common::{
    create_user_group_matching_regex, get_current_unix_user, validate_name_token,
    validate_ownership_by_user_prefix,
};

pub async fn user_exists(db_user: &str, connection: &mut MySqlConnection) -> anyhow::Result<bool> {
    let unix_user = get_current_unix_user()?;

    validate_user_name(db_user, &unix_user)?;

    let user_exists = sqlx::query(
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
    .await?
    .get::<bool, _>(0);

    Ok(user_exists)
}

pub async fn create_database_user(
    db_user: &str,
    connection: &mut MySqlConnection,
) -> anyhow::Result<()> {
    let unix_user = get_current_unix_user()?;

    validate_user_name(db_user, &unix_user)?;

    if user_exists(db_user, connection).await? {
        anyhow::bail!("User '{}' already exists", db_user);
    }

    // NOTE: see the note about SQL injections in `validate_ownership_of_user_name`
    sqlx::query(format!("CREATE USER {}@'%'", quote_literal(db_user),).as_str())
        .execute(connection)
        .await?;

    Ok(())
}

pub async fn delete_database_user(
    db_user: &str,
    connection: &mut MySqlConnection,
) -> anyhow::Result<()> {
    let unix_user = get_current_unix_user()?;

    validate_user_name(db_user, &unix_user)?;

    if !user_exists(db_user, connection).await? {
        anyhow::bail!("User '{}' does not exist", db_user);
    }

    // NOTE: see the note about SQL injections in `validate_ownership_of_user_name`
    sqlx::query(format!("DROP USER {}@'%'", quote_literal(db_user),).as_str())
        .execute(connection)
        .await?;

    Ok(())
}

pub async fn set_password_for_database_user(
    db_user: &str,
    password: &str,
    connection: &mut MySqlConnection,
) -> anyhow::Result<()> {
    let unix_user = crate::core::common::get_current_unix_user()?;
    validate_user_name(db_user, &unix_user)?;

    if !user_exists(db_user, connection).await? {
        anyhow::bail!("User '{}' does not exist", db_user);
    }

    // NOTE: see the note about SQL injections in `validate_ownership_of_user_name`
    sqlx::query(
        format!(
            "ALTER USER {}@'%' IDENTIFIED BY {}",
            quote_literal(db_user),
            quote_literal(password).as_str()
        )
        .as_str(),
    )
    .execute(connection)
    .await?;

    Ok(())
}

/// This struct contains information about a database user.
/// This can be extended if we need more information in the future.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct DatabaseUser {
    #[sqlx(rename = "User")]
    pub user: String,

    #[allow(dead_code)]
    #[serde(skip)]
    #[sqlx(rename = "Host")]
    pub host: String,

    #[sqlx(rename = "`Password` != '' OR `authentication_string` != ''")]
    pub has_password: bool,
}

/// This function fetches all database users that have a prefix matching the
/// unix username and group names of the given unix user.
pub async fn get_all_database_users_for_unix_user(
    unix_user: &User,
    connection: &mut MySqlConnection,
) -> anyhow::Result<Vec<DatabaseUser>> {
    let users = sqlx::query_as::<_, DatabaseUser>(
        r#"
          SELECT
            `User`,
            `Host`,
            `Password` != '' OR `authentication_string` != ''
          FROM `mysql`.`user`
          WHERE `User` REGEXP ?
        "#,
    )
    .bind(create_user_group_matching_regex(unix_user))
    .fetch_all(connection)
    .await?;

    Ok(users)
}

/// This function fetches a database user if it exists.
pub async fn get_database_user_for_user(
    username: &str,
    connection: &mut MySqlConnection,
) -> anyhow::Result<Option<DatabaseUser>> {
    let user = sqlx::query_as::<_, DatabaseUser>(
        r#"
          SELECT
            `User`,
            `Host`,
            `Password` != '' OR `authentication_string` != ''
          FROM `mysql`.`user`
          WHERE `User` = ?
        "#,
    )
    .bind(username)
    .fetch_optional(connection)
    .await?;

    Ok(user)
}

/// NOTE: It is very critical that this function validates the database name
///       properly. MySQL does not seem to allow for prepared statements, binding
///       the database name as a parameter to the query. This means that we have
///       to validate the database name ourselves to prevent SQL injection.
pub fn validate_user_name(name: &str, user: &User) -> anyhow::Result<()> {
    validate_name_token(name).context(format!("Invalid username: '{}'", name))?;
    validate_ownership_by_user_prefix(name, user)
        .context(format!("Invalid username: '{}'", name))?;

    Ok(())
}
