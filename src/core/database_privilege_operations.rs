//! Database privilege operations
//!
//! This module contains functions for querying, modifying,
//! displaying and comparing database privileges.
//!
//! A lot of the complexity comes from two core components:
//!
//! - The privilege editor that needs to be able to print
//!   an editable table of privileges and reparse the content
//!   after the user has made manual changes.
//!
//! - The comparison functionality that tells the user what
//!   changes will be made when applying a set of changes
//!   to the list of database privileges.

use std::collections::HashMap;

use anyhow::{anyhow, Context};
use indoc::indoc;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use sqlx::{mysql::MySqlRow, prelude::*, MySqlConnection};

use crate::core::{
    common::{
        create_user_group_matching_regex, get_current_unix_user, quote_identifier, rev_yn, yn,
    },
    database_operations::validate_database_name,
};

/// This is the list of fields that are used to fetch the db + user + privileges
/// from the `db` table in the database. If you need to add or remove privilege
/// fields, this is a good place to start.
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

/// This struct represents the set of privileges for a single user on a single database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabasePrivilegeRow {
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

impl DatabasePrivilegeRow {
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

    pub fn diff(&self, other: &DatabasePrivilegeRow) -> DatabasePrivilegeRowDiff {
        debug_assert!(self.db == other.db && self.user == other.user);

        DatabasePrivilegeRowDiff {
            db: self.db.clone(),
            user: self.user.clone(),
            diff: DATABASE_PRIVILEGE_FIELDS
                .into_iter()
                .skip(2)
                .filter_map(|field| {
                    DatabasePrivilegeChange::new(
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
fn get_row_priv_field(row: &MySqlRow, field: &str) -> Result<bool, sqlx::Error> {
    match rev_yn(row.try_get(field)?) {
        Some(val) => Ok(val),
        _ => {
            log::warn!("Invalid value for privilege: {}", field);
            Ok(false)
        }
    }
}

impl FromRow<'_, MySqlRow> for DatabasePrivilegeRow {
    fn from_row(row: &MySqlRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            db: row.try_get("db")?,
            user: row.try_get("user")?,
            select_priv: get_row_priv_field(row, "select_priv")?,
            insert_priv: get_row_priv_field(row, "insert_priv")?,
            update_priv: get_row_priv_field(row, "update_priv")?,
            delete_priv: get_row_priv_field(row, "delete_priv")?,
            create_priv: get_row_priv_field(row, "create_priv")?,
            drop_priv: get_row_priv_field(row, "drop_priv")?,
            alter_priv: get_row_priv_field(row, "alter_priv")?,
            index_priv: get_row_priv_field(row, "index_priv")?,
            create_tmp_table_priv: get_row_priv_field(row, "create_tmp_table_priv")?,
            lock_tables_priv: get_row_priv_field(row, "lock_tables_priv")?,
            references_priv: get_row_priv_field(row, "references_priv")?,
        })
    }
}

pub async fn get_database_privileges(
    database_name: &str,
    connection: &mut MySqlConnection,
) -> anyhow::Result<Vec<DatabasePrivilegeRow>> {
    let unix_user = get_current_unix_user()?;
    validate_database_name(database_name, &unix_user)?;

    let result = sqlx::query_as::<_, DatabasePrivilegeRow>(&format!(
        "SELECT {} FROM `db` WHERE `db` = ?",
        DATABASE_PRIVILEGE_FIELDS
            .iter()
            .map(|field| quote_identifier(field))
            .join(","),
    ))
    .bind(database_name)
    .fetch_all(connection)
    .await
    .context("Failed to show database")?;

    Ok(result)
}

pub async fn get_all_database_privileges(
    connection: &mut MySqlConnection,
) -> anyhow::Result<Vec<DatabasePrivilegeRow>> {
    let unix_user = get_current_unix_user()?;

    let result = sqlx::query_as::<_, DatabasePrivilegeRow>(&format!(
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
    .fetch_all(connection)
    .await
    .context("Failed to show databases")?;

    Ok(result)
}

/*************************/
/* CLI INTERFACE PARSING */
/*************************/

/// See documentation for `DatabaseCommand::EditDbPrivs`.
pub fn parse_privilege_table_cli_arg(arg: &str) -> anyhow::Result<DatabasePrivilegeRow> {
    let parts: Vec<&str> = arg.split(':').collect();
    if parts.len() != 3 {
        anyhow::bail!("Invalid argument format. See `edit-db-privs --help` for more information.");
    }

    let db = parts[0].to_string();
    let user = parts[1].to_string();
    let privs = parts[2].to_string();

    let mut result = DatabasePrivilegeRow {
        db,
        user,
        select_priv: false,
        insert_priv: false,
        update_priv: false,
        delete_priv: false,
        create_priv: false,
        drop_priv: false,
        alter_priv: false,
        index_priv: false,
        create_tmp_table_priv: false,
        lock_tables_priv: false,
        references_priv: false,
    };

    for char in privs.chars() {
        match char {
            's' => result.select_priv = true,
            'i' => result.insert_priv = true,
            'u' => result.update_priv = true,
            'd' => result.delete_priv = true,
            'c' => result.create_priv = true,
            'D' => result.drop_priv = true,
            'a' => result.alter_priv = true,
            'I' => result.index_priv = true,
            't' => result.create_tmp_table_priv = true,
            'l' => result.lock_tables_priv = true,
            'r' => result.references_priv = true,
            'A' => {
                result.select_priv = true;
                result.insert_priv = true;
                result.update_priv = true;
                result.delete_priv = true;
                result.create_priv = true;
                result.drop_priv = true;
                result.alter_priv = true;
                result.index_priv = true;
                result.create_tmp_table_priv = true;
                result.lock_tables_priv = true;
                result.references_priv = true;
            }
            _ => anyhow::bail!("Invalid privilege character: {}", char),
        }
    }

    Ok(result)
}

/**********************************/
/* EDITOR CONTENT PARSING/DISPLAY */
/**********************************/

// TODO: merge with `rev_yn` in `common.rs`

fn parse_privilege(yn: &str) -> anyhow::Result<bool> {
    match yn.to_ascii_lowercase().as_str() {
        "y" => Ok(true),
        "n" => Ok(false),
        _ => Err(anyhow!("Expected Y or N, found {}", yn)),
    }
}

pub fn parse_privilege_data_from_editor_content(
    content: String,
) -> anyhow::Result<Vec<DatabasePrivilegeRow>> {
    content
        .trim()
        .split('\n')
        .map(|line| line.trim())
        .filter(|line| !(line.starts_with('#') || line.starts_with("//") || line == &""))
        .skip(1)
        .map(|line| {
            let line_parts: Vec<&str> = line.trim().split_ascii_whitespace().collect();
            if line_parts.len() != DATABASE_PRIVILEGE_FIELDS.len() {
                anyhow::bail!("")
            }

            Ok(DatabasePrivilegeRow {
                db: (*line_parts.first().unwrap()).to_owned(),
                user: (*line_parts.get(1).unwrap()).to_owned(),
                select_priv: parse_privilege(line_parts.get(2).unwrap())
                    .context("Could not parse SELECT privilege")?,
                insert_priv: parse_privilege(line_parts.get(3).unwrap())
                    .context("Could not parse INSERT privilege")?,
                update_priv: parse_privilege(line_parts.get(4).unwrap())
                    .context("Could not parse UPDATE privilege")?,
                delete_priv: parse_privilege(line_parts.get(5).unwrap())
                    .context("Could not parse DELETE privilege")?,
                create_priv: parse_privilege(line_parts.get(6).unwrap())
                    .context("Could not parse CREATE privilege")?,
                drop_priv: parse_privilege(line_parts.get(7).unwrap())
                    .context("Could not parse DROP privilege")?,
                alter_priv: parse_privilege(line_parts.get(8).unwrap())
                    .context("Could not parse ALTER privilege")?,
                index_priv: parse_privilege(line_parts.get(9).unwrap())
                    .context("Could not parse INDEX privilege")?,
                create_tmp_table_priv: parse_privilege(line_parts.get(10).unwrap())
                    .context("Could not parse CREATE TEMPORARY TABLE privilege")?,
                lock_tables_priv: parse_privilege(line_parts.get(11).unwrap())
                    .context("Could not parse LOCK TABLES privilege")?,
                references_priv: parse_privilege(line_parts.get(12).unwrap())
                    .context("Could not parse REFERENCES privilege")?,
            })
        })
        .collect::<anyhow::Result<Vec<DatabasePrivilegeRow>>>()
}

/// Generates a single row of the privileges table for the editor.
pub fn format_privileges_line(
    privs: &DatabasePrivilegeRow,
    username_len: usize,
    database_name_len: usize,
) -> String {
    // Format a privileges line by padding each value with spaces
    // The first two fields are padded to the length of the longest username and database name
    // The remaining fields are padded to the length of the corresponding field name

    DATABASE_PRIVILEGE_FIELDS
        .into_iter()
        .map(|field| match field {
            "db" => format!("{:width$}", privs.db, width = database_name_len),
            "user" => format!("{:width$}", privs.user, width = username_len),
            privilege => format!(
                "{:width$}",
                yn(privs.get_privilege_by_name(privilege)),
                width = db_priv_field_human_readable_name(privilege).len()
            ),
        })
        .join(" ")
        .trim()
        .to_string()
}

const EDITOR_COMMENT: &str = r#"
# Welcome to the privilege editor.
# Each line defines what privileges a single user has on a single database.
# The first two columns respectively represent the database name and the user, and the remaining columns are the privileges.
# If the user should have a certain privilege, write 'Y', otherwise write 'N'.
#
# Lines starting with '#' are comments and will be ignored.
"#;

/// Generates the content for the privilege editor.
///
/// The unix user is used in case there are no privileges to edit,
/// so that the user can see an example line based on their username.
pub fn generate_editor_content_from_privilege_data(
    privilege_data: &[DatabasePrivilegeRow],
    unix_user: &str,
) -> String {
    let example_user = format!("{}_user", unix_user);
    let example_db = format!("{}_db", unix_user);

    // NOTE: `.max()`` fails when the iterator is empty.
    //       In this case, we know that the only fields in the
    //       editor will be the example user and example db name.
    //       Hence, it's put as the fallback value, despite not really
    //       being a "fallback" in the normal sense.
    let longest_username = privilege_data
        .iter()
        .map(|p| p.user.len())
        .max()
        .unwrap_or(example_user.len());

    let longest_database_name = privilege_data
        .iter()
        .map(|p| p.db.len())
        .max()
        .unwrap_or(example_db.len());

    let mut header: Vec<_> = DATABASE_PRIVILEGE_FIELDS
        .into_iter()
        .map(db_priv_field_human_readable_name)
        .collect();

    // Pad the first two columns with spaces to align the privileges.
    header[0] = format!("{:width$}", header[0], width = longest_database_name);
    header[1] = format!("{:width$}", header[1], width = longest_username);

    let example_line = format_privileges_line(
        &DatabasePrivilegeRow {
            db: example_db,
            user: example_user,
            select_priv: true,
            insert_priv: true,
            update_priv: true,
            delete_priv: true,
            create_priv: false,
            drop_priv: false,
            alter_priv: false,
            index_priv: false,
            create_tmp_table_priv: false,
            lock_tables_priv: false,
            references_priv: false,
        },
        longest_username,
        longest_database_name,
    );

    format!(
        "{}\n{}\n{}",
        EDITOR_COMMENT,
        header.join(" "),
        if privilege_data.is_empty() {
            format!("# {}", example_line)
        } else {
            privilege_data
                .iter()
                .map(|privs| format_privileges_line(privs, longest_username, longest_database_name))
                .join("\n")
        }
    )
}

/*****************************/
/* CALCULATE PRIVILEGE DIFFS */
/*****************************/

/// This struct represents encapsulates the differences between two
/// instances of privilege sets for a single user on a single database.
///
/// The `User` and `Database` are the same for both instances.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabasePrivilegeRowDiff {
    pub db: String,
    pub user: String,
    pub diff: Vec<DatabasePrivilegeChange>,
}

/// This enum represents a change for a single privilege.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DatabasePrivilegeChange {
    YesToNo(String),
    NoToYes(String),
}

impl DatabasePrivilegeChange {
    pub fn new(p1: bool, p2: bool, name: &str) -> Option<DatabasePrivilegeChange> {
        match (p1, p2) {
            (true, false) => Some(DatabasePrivilegeChange::YesToNo(name.to_owned())),
            (false, true) => Some(DatabasePrivilegeChange::NoToYes(name.to_owned())),
            _ => None,
        }
    }
}

/// This enum encapsulates whether a [`DatabasePrivilegeRow`] was intrduced, modified or deleted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DatabasePrivilegesDiff {
    New(DatabasePrivilegeRow),
    Modified(DatabasePrivilegeRowDiff),
    Deleted(DatabasePrivilegeRow),
}

pub fn diff_privileges(
    from: Vec<DatabasePrivilegeRow>,
    to: &[DatabasePrivilegeRow],
) -> Vec<DatabasePrivilegesDiff> {
    let from_lookup_table: HashMap<(String, String), DatabasePrivilegeRow> = HashMap::from_iter(
        from.iter()
            .cloned()
            .map(|p| ((p.db.clone(), p.user.clone()), p)),
    );

    let to_lookup_table: HashMap<(String, String), DatabasePrivilegeRow> = HashMap::from_iter(
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

/// Uses the resulting diffs to make modifications to the database.
pub async fn apply_privilege_diffs(
    diffs: Vec<DatabasePrivilegesDiff>,
    connection: &mut MySqlConnection,
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
                .execute(&mut *connection)
                .await?;
            }
            DatabasePrivilegesDiff::Modified(p) => {
                let tables = p
                    .diff
                    .iter()
                    .map(|diff| match diff {
                        DatabasePrivilegeChange::YesToNo(name) => format!("`{}` = 'N'", name),
                        DatabasePrivilegeChange::NoToYes(name) => format!("`{}` = 'Y'", name),
                    })
                    .join(",");

                sqlx::query(
                    format!("UPDATE `db` SET {} WHERE `db` = ? AND `user` = ?", tables).as_str(),
                )
                .bind(p.db)
                .bind(p.user)
                .execute(&mut *connection)
                .await?;
            }
            DatabasePrivilegesDiff::Deleted(p) => {
                sqlx::query("DELETE FROM `db` WHERE `db` = ? AND `user` = ?")
                    .bind(p.db)
                    .bind(p.user)
                    .execute(&mut *connection)
                    .await?;
            }
        }
    }
    Ok(())
}

/*********/
/* TESTS */
/*********/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_privilege_change_creation() {
        assert_eq!(
            DatabasePrivilegeChange::new(true, false, "test"),
            Some(DatabasePrivilegeChange::YesToNo("test".to_owned()))
        );
        assert_eq!(
            DatabasePrivilegeChange::new(false, true, "test"),
            Some(DatabasePrivilegeChange::NoToYes("test".to_owned()))
        );
        assert_eq!(DatabasePrivilegeChange::new(true, true, "test"), None);
        assert_eq!(DatabasePrivilegeChange::new(false, false, "test"), None);
    }

    #[test]
    fn test_diff_privileges() {
        let from = vec![DatabasePrivilegeRow {
            db: "db".to_owned(),
            user: "user".to_owned(),
            select_priv: true,
            insert_priv: true,
            update_priv: true,
            delete_priv: true,
            create_priv: true,
            drop_priv: true,
            alter_priv: true,
            index_priv: false,
            create_tmp_table_priv: true,
            lock_tables_priv: true,
            references_priv: false,
        }];

        let mut to = from.clone();
        to[0].select_priv = false;
        to[0].insert_priv = false;
        to[0].index_priv = true;

        let diffs = diff_privileges(from, &to);

        assert_eq!(
            diffs,
            vec![DatabasePrivilegesDiff::Modified(DatabasePrivilegeRowDiff {
                db: "db".to_owned(),
                user: "user".to_owned(),
                diff: vec![
                    DatabasePrivilegeChange::YesToNo("select_priv".to_owned()),
                    DatabasePrivilegeChange::YesToNo("insert_priv".to_owned()),
                    DatabasePrivilegeChange::NoToYes("index_priv".to_owned()),
                ],
            })]
        );

        assert!(matches!(&diffs[0], DatabasePrivilegesDiff::Modified(_)));
    }

    #[test]
    fn ensure_generated_and_parsed_editor_content_is_equal() {
        let permissions = vec![
            DatabasePrivilegeRow {
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
            },
            DatabasePrivilegeRow {
                db: "db2".to_owned(),
                user: "user2".to_owned(),
                select_priv: false,
                insert_priv: false,
                update_priv: false,
                delete_priv: false,
                create_priv: false,
                drop_priv: false,
                alter_priv: false,
                index_priv: false,
                create_tmp_table_priv: false,
                lock_tables_priv: false,
                references_priv: false,
            },
        ];

        let content = generate_editor_content_from_privilege_data(&permissions, "user");

        let parsed_permissions = parse_privilege_data_from_editor_content(content).unwrap();

        assert_eq!(permissions, parsed_permissions);
    }
}
