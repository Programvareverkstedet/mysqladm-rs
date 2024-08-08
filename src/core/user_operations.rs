use anyhow::Context;
use nix::unistd::User;
use serde::{Deserialize, Serialize};
use sqlx::{prelude::*, MySqlConnection};

use crate::core::common::{
    create_user_group_matching_regex, get_current_unix_user, quote_literal, validate_name_or_error,
    validate_ownership_or_error, DbOrUser,
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

async fn user_is_locked(db_user: &str, connection: &mut MySqlConnection) -> anyhow::Result<bool> {
    let unix_user = get_current_unix_user()?;

    validate_user_name(db_user, &unix_user)?;

    if !user_exists(db_user, connection).await? {
        anyhow::bail!("User '{}' does not exist", db_user);
    }

    let is_locked = sqlx::query(
        r#"
          SELECT JSON_EXTRACT(`mysql`.`global_priv`.`priv`, "$.account_locked") = 'true'
          FROM `mysql`.`global_priv`
          WHERE `User` = ?
          AND `Host` = '%'
        "#,
    )
    .bind(db_user)
    .fetch_one(connection)
    .await?
    .get::<bool, _>(0);

    Ok(is_locked)
}

pub async fn lock_database_user(
    db_user: &str,
    connection: &mut MySqlConnection,
) -> anyhow::Result<()> {
    let unix_user = get_current_unix_user()?;

    validate_user_name(db_user, &unix_user)?;

    if !user_exists(db_user, connection).await? {
        anyhow::bail!("User '{}' does not exist", db_user);
    }

    if user_is_locked(db_user, connection).await? {
        anyhow::bail!("User '{}' is already locked", db_user);
    }

    // NOTE: see the note about SQL injections in `validate_ownership_of_user_name`
    sqlx::query(format!("ALTER USER {}@'%' ACCOUNT LOCK", quote_literal(db_user),).as_str())
        .execute(connection)
        .await?;

    Ok(())
}

pub async fn unlock_database_user(
    db_user: &str,
    connection: &mut MySqlConnection,
) -> anyhow::Result<()> {
    let unix_user = get_current_unix_user()?;

    validate_user_name(db_user, &unix_user)?;

    if !user_exists(db_user, connection).await? {
        anyhow::bail!("User '{}' does not exist", db_user);
    }

    if !user_is_locked(db_user, connection).await? {
        anyhow::bail!("User '{}' is already unlocked", db_user);
    }

    // NOTE: see the note about SQL injections in `validate_ownership_of_user_name`
    sqlx::query(format!("ALTER USER {}@'%' ACCOUNT UNLOCK", quote_literal(db_user),).as_str())
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

    #[sqlx(rename = "has_password")]
    pub has_password: bool,

    #[sqlx(rename = "is_locked")]
    pub is_locked: bool,
}

const DB_USER_SELECT_STATEMENT: &str = r#"
SELECT
  `mysql`.`user`.`User`,
  `mysql`.`user`.`Host`,
  `mysql`.`user`.`Password` != '' OR `mysql`.`user`.`authentication_string` != '' AS `has_password`,
  COALESCE(
    JSON_EXTRACT(`mysql`.`global_priv`.`priv`, "$.account_locked"),
    'false'
  ) != 'false' AS `is_locked`
FROM `mysql`.`user`
JOIN `mysql`.`global_priv` ON
  `mysql`.`user`.`User` = `mysql`.`global_priv`.`User`
  AND `mysql`.`user`.`Host` = `mysql`.`global_priv`.`Host`
"#;

/// This function fetches all database users that have a prefix matching the
/// unix username and group names of the given unix user.
pub async fn get_all_database_users_for_unix_user(
    unix_user: &User,
    connection: &mut MySqlConnection,
) -> anyhow::Result<Vec<DatabaseUser>> {
    let users = sqlx::query_as::<_, DatabaseUser>(
        &(DB_USER_SELECT_STATEMENT.to_string() + "WHERE `mysql`.`user`.`User` REGEXP ?"),
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
        &(DB_USER_SELECT_STATEMENT.to_string() + "WHERE `mysql`.`user`.`User` = ?"),
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
    validate_name_or_error(name, DbOrUser::User)
        .context(format!("Invalid username: '{}'", name))?;
    validate_ownership_or_error(name, user, DbOrUser::User)
        .context(format!("Invalid username: '{}'", name))?;

    Ok(())
}
