use anyhow::Context;
use indoc::indoc;
use nix::unistd::User;
use serde::{Deserialize, Serialize};
use sqlx::{prelude::*, MySqlConnection};

use crate::core::common::quote_literal;

use super::common::{create_user_group_matching_regex, get_current_unix_user, validate_prefix_for_user};

pub async fn create_database_user(db_user: &str, conn: &mut MySqlConnection) -> anyhow::Result<()> {
    let unix_user = get_current_unix_user()?;

    validate_ownership_of_user_name(db_user, &unix_user)?;

    // NOTE: see the note about SQL injections in `validate_ownershipt_of_user_name`
    sqlx::query(format!("CREATE USER {}@'%'", quote_literal(db_user),).as_str())
        .execute(conn)
        .await?;

    Ok(())
}

pub async fn delete_database_user(db_user: &str, conn: &mut MySqlConnection) -> anyhow::Result<()> {
    let unix_user = get_current_unix_user()?;

    validate_ownership_of_user_name(db_user, &unix_user)?;

    // NOTE: see the note about SQL injections in `validate_ownershipt_of_user_name`
    sqlx::query(format!("DROP USER {}@'%'", quote_literal(db_user),).as_str())
        .execute(conn)
        .await?;

    Ok(())
}

pub async fn set_password_for_database_user(
    db_user: &str,
    password: &str,
    conn: &mut MySqlConnection,
) -> anyhow::Result<()> {
    let unix_user = crate::core::common::get_current_unix_user()?;
    validate_ownership_of_user_name(db_user, &unix_user)?;

    // NOTE: see the note about SQL injections in `validate_ownershipt_of_user_name`
    sqlx::query(
        format!(
            "ALTER USER {}@'%' IDENTIFIED BY {}",
            quote_literal(db_user),
            quote_literal(password).as_str()
        )
        .as_str(),
    )
    .execute(conn)
    .await?;

    Ok(())
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct DatabaseUser {
    #[sqlx(rename = "User")]
    pub user: String,

    #[sqlx(rename = "Host")]
    pub host: String,

    #[sqlx(rename = "Password")]
    pub password: String,

    pub authentication_string: String,
}

pub async fn get_all_database_users_for_unix_user(
    unix_user: &User,
    conn: &mut MySqlConnection,
) -> anyhow::Result<Vec<DatabaseUser>> {
    let users = sqlx::query_as::<_, DatabaseUser>(
        r#"
          SELECT `User`, `Host`, `Password`, `authentication_string`
          FROM `mysql`.`user`
          WHERE `User` REGEXP ?
        "#,
    )
    .bind(create_user_group_matching_regex(unix_user))
    .fetch_all(conn)
    .await?;

    Ok(users)
}

pub async fn get_database_user_for_user(
    username: &str,
    conn: &mut MySqlConnection,
) -> anyhow::Result<Option<DatabaseUser>> {
    let user = sqlx::query_as::<_, DatabaseUser>(
        r#"
          SELECT `User`, `Host`, `Password`, `authentication_string`
          FROM `mysql`.`user`
          WHERE `User` = ?
        "#,
    )
    .bind(username)
    .fetch_optional(conn)
    .await?;

    Ok(user)
}

/// NOTE: It is very critical that this function validates the database name
///       properly. MySQL does not seem to allow for prepared statements, binding
///       the database name as a parameter to the query. This means that we have
///       to validate the database name ourselves to prevent SQL injection.
pub fn validate_ownership_of_user_name(name: &str, user: &User) -> anyhow::Result<()> {
    if name.contains(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-') {
        anyhow::bail!(
            indoc! {r#"
              Username '{}' contains invalid characters.
              Only A-Z, a-z, 0-9, _ (underscore) and - (dash) permitted.
            "#},
            name
        );
    }

    // TODO: does the name have a length limit?
    // if name.len() > 48 {
    //     anyhow::bail!(
    //         indoc! {r#"
    //           Username '{}' is too long.
    //           Maximum length is 48 characters. Skipping.
    //         "#},
    //         name
    //     );
    // }

    validate_prefix_for_user(name, user).context(format!("Invalid username: '{}'", name))?;

    Ok(())
}
