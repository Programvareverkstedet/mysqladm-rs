use std::collections::HashMap;

use anyhow::Context;
use indoc::indoc;
use itertools::Itertools;
use nix::unistd::User;
use serde::{Deserialize, Serialize};
use sqlx::{mysql::MySqlRow, prelude::*, MySqlConnection};

use super::common::{
    create_user_group_matching_regex, get_current_unix_user, quote_identifier, validate_name_token, validate_ownership_by_user_prefix
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

pub const DATABASE_PRIVILEGE_FIELDS: [&str; 13] = [
    "db",
    "user",
    "select_priv",
    "insert_priv",
    "update_priv",
    "delete_priv",
    "create_priv",
    "drop_priv",
    "alter_priv",
    "index_priv",
    "create_tmp_table_priv",
    "lock_tables_priv",
    "references_priv",
];

pub fn db_priv_field_human_readable_name(name: &str) -> String {
    match name {
        "db" => "Database".to_owned(),
        "user" => "User".to_owned(),
        "select_priv" => "Select".to_owned(),
        "insert_priv" => "Insert".to_owned(),
        "update_priv" => "Update".to_owned(),
        "delete_priv" => "Delete".to_owned(),
        "create_priv" => "Create".to_owned(),
        "drop_priv" => "Drop".to_owned(),
        "alter_priv" => "Alter".to_owned(),
        "index_priv" => "Index".to_owned(),
        "create_tmp_table_priv" => "Temp".to_owned(),
        "lock_tables_priv" => "Lock".to_owned(),
        "references_priv" => "References".to_owned(),
        _ => format!("Unknown({})", name),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabasePrivileges {
    pub db: String,
    pub user: String,
    pub select_priv: bool,
    pub insert_priv: bool,
    pub update_priv: bool,
    pub delete_priv: bool,
    pub create_priv: bool,
    pub drop_priv: bool,
    pub alter_priv: bool,
    pub index_priv: bool,
    pub create_tmp_table_priv: bool,
    pub lock_tables_priv: bool,
    pub references_priv: bool,
}

impl DatabasePrivileges {
    pub fn get_privilege_by_name(&self, name: &str) -> bool {
        match name {
            "select_priv" => self.select_priv,
            "insert_priv" => self.insert_priv,
            "update_priv" => self.update_priv,
            "delete_priv" => self.delete_priv,
            "create_priv" => self.create_priv,
            "drop_priv" => self.drop_priv,
            "alter_priv" => self.alter_priv,
            "index_priv" => self.index_priv,
            "create_tmp_table_priv" => self.create_tmp_table_priv,
            "lock_tables_priv" => self.lock_tables_priv,
            "references_priv" => self.references_priv,
            _ => false,
        }
    }
    pub fn diff(&self, other: &DatabasePrivileges) -> DatabasePrivilegeDiffList {
        debug_assert!(self.db == other.db && self.user == other.user);

        DatabasePrivilegeDiffList {
            db: self.db.clone(),
            user: self.user.clone(),
            diff: DATABASE_PRIVILEGE_FIELDS
                .into_iter()
                .skip(2)
                .filter_map(|field| {
                    diff_single_priv(
                        self.get_privilege_by_name(field),
                        other.get_privilege_by_name(field),
                        field,
                    )
                })
                .collect(),
        }
    }
}

#[inline]
pub(crate) fn yn(b: bool) -> &'static str {
    if b {
        "Y"
    } else {
        "N"
    }
}

#[inline]
pub(crate) fn rev_yn(s: &str) -> bool {
    match s.to_lowercase().as_str() {
        "y" => true,
        "n" => false,
        _ => {
            log::warn!("Invalid value for privilege: {}", s);
            false
        }
    }
}

impl FromRow<'_, MySqlRow> for DatabasePrivileges {
    fn from_row(row: &MySqlRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            db: row.try_get("db")?,
            user: row.try_get("user")?,
            select_priv: row.try_get("select_priv").map(rev_yn)?,
            insert_priv: row.try_get("insert_priv").map(rev_yn)?,
            update_priv: row.try_get("update_priv").map(rev_yn)?,
            delete_priv: row.try_get("delete_priv").map(rev_yn)?,
            create_priv: row.try_get("create_priv").map(rev_yn)?,
            drop_priv: row.try_get("drop_priv").map(rev_yn)?,
            alter_priv: row.try_get("alter_priv").map(rev_yn)?,
            index_priv: row.try_get("index_priv").map(rev_yn)?,
            create_tmp_table_priv: row.try_get("create_tmp_table_priv").map(rev_yn)?,
            lock_tables_priv: row.try_get("lock_tables_priv").map(rev_yn)?,
            references_priv: row.try_get("references_priv").map(rev_yn)?,
        })
    }
}

pub async fn get_database_privileges(
    database_name: &str,
    conn: &mut MySqlConnection,
) -> anyhow::Result<Vec<DatabasePrivileges>> {
    let unix_user = get_current_unix_user()?;
    validate_database_name(database_name, &unix_user)?;

    let result = sqlx::query_as::<_, DatabasePrivileges>(&format!(
        "SELECT {} FROM `db` WHERE `db` = ?",
        DATABASE_PRIVILEGE_FIELDS
            .iter()
            .map(|field| quote_identifier(field))
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

    let result = sqlx::query_as::<_, DatabasePrivileges>(&format!(
        indoc! {r#"
          SELECT {} FROM `db` WHERE `db` IN
          (SELECT DISTINCT `SCHEMA_NAME` AS `database`
            FROM `information_schema`.`SCHEMATA`
            WHERE `SCHEMA_NAME` NOT IN ('information_schema', 'performance_schema', 'mysql', 'sys')
              AND `SCHEMA_NAME` REGEXP ?)
        "#},
        DATABASE_PRIVILEGE_FIELDS
            .iter()
            .map(|field| format!("`{field}`"))
            .join(","),
    ))
    .bind(create_user_group_matching_regex(&unix_user))
    .fetch_all(conn)
    .await
    .context("Failed to show databases")?;

    Ok(result)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabasePrivilegeDiffList {
    pub db: String,
    pub user: String,
    pub diff: Vec<DatabasePrivilegeDiff>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DatabasePrivilegeDiff {
    YesToNo(String),
    NoToYes(String),
}

fn diff_single_priv(p1: bool, p2: bool, name: &str) -> Option<DatabasePrivilegeDiff> {
    match (p1, p2) {
        (true, false) => Some(DatabasePrivilegeDiff::YesToNo(name.to_owned())),
        (false, true) => Some(DatabasePrivilegeDiff::NoToYes(name.to_owned())),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DatabasePrivilegesDiff {
    New(DatabasePrivileges),
    Modified(DatabasePrivilegeDiffList),
    Deleted(DatabasePrivileges),
}

pub async fn diff_permissions(
    from: Vec<DatabasePrivileges>,
    to: &[DatabasePrivileges],
) -> Vec<DatabasePrivilegesDiff> {
    let from_lookup_table: HashMap<(String, String), DatabasePrivileges> = HashMap::from_iter(
        from.iter()
            .cloned()
            .map(|p| ((p.db.clone(), p.user.clone()), p)),
    );

    let to_lookup_table: HashMap<(String, String), DatabasePrivileges> = HashMap::from_iter(
        to.iter()
            .cloned()
            .map(|p| ((p.db.clone(), p.user.clone()), p)),
    );

    let mut result = vec![];

    for p in to {
        if let Some(old_p) = from_lookup_table.get(&(p.db.clone(), p.user.clone())) {
            let diff = old_p.diff(p);
            if !diff.diff.is_empty() {
                result.push(DatabasePrivilegesDiff::Modified(diff));
            }
        } else {
            result.push(DatabasePrivilegesDiff::New(p.clone()));
        }
    }

    for p in from {
        if !to_lookup_table.contains_key(&(p.db.clone(), p.user.clone())) {
            result.push(DatabasePrivilegesDiff::Deleted(p));
        }
    }

    result
}

pub async fn apply_permission_diffs(
    diffs: Vec<DatabasePrivilegesDiff>,
    conn: &mut MySqlConnection,
) -> anyhow::Result<()> {
    for diff in diffs {
        match diff {
            DatabasePrivilegesDiff::New(p) => {
                let tables = DATABASE_PRIVILEGE_FIELDS
                    .iter()
                    .map(|field| format!("`{field}`"))
                    .join(",");

                let question_marks = std::iter::repeat("?")
                    .take(DATABASE_PRIVILEGE_FIELDS.len())
                    .join(",");

                sqlx::query(
                    format!("INSERT INTO `db` ({}) VALUES ({})", tables, question_marks).as_str(),
                )
                .bind(p.db)
                .bind(p.user)
                .bind(yn(p.select_priv))
                .bind(yn(p.insert_priv))
                .bind(yn(p.update_priv))
                .bind(yn(p.delete_priv))
                .bind(yn(p.create_priv))
                .bind(yn(p.drop_priv))
                .bind(yn(p.alter_priv))
                .bind(yn(p.index_priv))
                .bind(yn(p.create_tmp_table_priv))
                .bind(yn(p.lock_tables_priv))
                .bind(yn(p.references_priv))
                .execute(&mut *conn)
                .await?;
            }
            DatabasePrivilegesDiff::Modified(p) => {
                let tables = p
                    .diff
                    .iter()
                    .map(|diff| match diff {
                        DatabasePrivilegeDiff::YesToNo(name) => format!("`{}` = 'N'", name),
                        DatabasePrivilegeDiff::NoToYes(name) => format!("`{}` = 'Y'", name),
                    })
                    .join(",");

                sqlx::query(
                    format!("UPDATE `db` SET {} WHERE `db` = ? AND `user` = ?", tables).as_str(),
                )
                .bind(p.db)
                .bind(p.user)
                .execute(&mut *conn)
                .await?;
            }
            DatabasePrivilegesDiff::Deleted(p) => {
                sqlx::query("DELETE FROM `db` WHERE `db` = ? AND `user` = ?")
                    .bind(p.db)
                    .bind(p.user)
                    .execute(&mut *conn)
                    .await?;
            }
        }
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_single_priv() {
        assert_eq!(
            diff_single_priv(true, false, "test"),
            Some(DatabasePrivilegeDiff::YesToNo("test".to_owned()))
        );
        assert_eq!(
            diff_single_priv(false, true, "test"),
            Some(DatabasePrivilegeDiff::NoToYes("test".to_owned()))
        );
        assert_eq!(diff_single_priv(true, true, "test"), None);
        assert_eq!(diff_single_priv(false, false, "test"), None);
    }

    #[tokio::test]
    async fn test_diff_permissions() {
        let from = vec![DatabasePrivileges {
            db: "db".to_owned(),
            user: "user".to_owned(),
            select_priv: true,
            insert_priv: true,
            update_priv: true,
            delete_priv: true,
            create_priv: true,
            drop_priv: true,
            alter_priv: true,
            index_priv: true,
            create_tmp_table_priv: true,
            lock_tables_priv: true,
            references_priv: true,
        }];

        let to = vec![DatabasePrivileges {
            db: "db".to_owned(),
            user: "user".to_owned(),
            select_priv: false,
            insert_priv: true,
            update_priv: true,
            delete_priv: true,
            create_priv: true,
            drop_priv: true,
            alter_priv: true,
            index_priv: true,
            create_tmp_table_priv: true,
            lock_tables_priv: true,
            references_priv: true,
        }];

        let diffs = diff_permissions(from, &to).await;

        assert_eq!(
            diffs,
            vec![DatabasePrivilegesDiff::Modified(
                DatabasePrivilegeDiffList {
                    db: "db".to_owned(),
                    user: "user".to_owned(),
                    diff: vec![DatabasePrivilegeDiff::YesToNo("select_priv".to_owned())],
                }
            )]
        );

        assert!(matches!(&diffs[0], DatabasePrivilegesDiff::Modified(_)));
    }
}
