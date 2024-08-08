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

use std::collections::{BTreeSet, HashMap};

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
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
fn get_mysql_row_priv_field(row: &MySqlRow, position: usize) -> Result<bool, sqlx::Error> {
    let field = DATABASE_PRIVILEGE_FIELDS[position];
    let value = row.try_get(position)?;
    match rev_yn(value) {
        Some(val) => Ok(val),
        _ => {
            log::warn!(r#"Invalid value for privilege "{}": '{}'"#, field, value);
            Ok(false)
        }
    }
}

impl FromRow<'_, MySqlRow> for DatabasePrivilegeRow {
    fn from_row(row: &MySqlRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            db: row.try_get("db")?,
            user: row.try_get("user")?,
            select_priv: get_mysql_row_priv_field(row, 2)?,
            insert_priv: get_mysql_row_priv_field(row, 3)?,
            update_priv: get_mysql_row_priv_field(row, 4)?,
            delete_priv: get_mysql_row_priv_field(row, 5)?,
            create_priv: get_mysql_row_priv_field(row, 6)?,
            drop_priv: get_mysql_row_priv_field(row, 7)?,
            alter_priv: get_mysql_row_priv_field(row, 8)?,
            index_priv: get_mysql_row_priv_field(row, 9)?,
            create_tmp_table_priv: get_mysql_row_priv_field(row, 10)?,
            lock_tables_priv: get_mysql_row_priv_field(row, 11)?,
            references_priv: get_mysql_row_priv_field(row, 12)?,
        })
    }
}

/// Get all users + privileges for a single database.
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

/// Get all database + user + privileges pairs that are owned by the current user.
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

/// See documentation for [`DatabaseCommand::EditDbPrivs`].
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

#[inline]
fn parse_privilege(yn: &str, name: &str) -> anyhow::Result<bool> {
    rev_yn(yn)
        .ok_or_else(|| anyhow!("Expected Y or N, found {}", yn))
        .context(format!("Could not parse {} privilege", name))
}

#[derive(Debug)]
enum PrivilegeRowParseResult {
    PrivilegeRow(DatabasePrivilegeRow),
    ParserError(anyhow::Error),
    TooFewFields(usize),
    TooManyFields(usize),
    Header,
    Comment,
    Empty,
}

#[inline]
fn row_is_header(row: &str) -> bool {
    row.split_ascii_whitespace()
        .zip(DATABASE_PRIVILEGE_FIELDS.iter())
        .map(|(field, priv_name)| (field, db_priv_field_human_readable_name(priv_name)))
        .all(|(field, header_field)| field == header_field)
}

/// Parse a single row of the privileges table from the editor.
fn parse_privilege_row_from_editor(row: &str) -> PrivilegeRowParseResult {
    if row.starts_with('#') || row.starts_with("//") {
        return PrivilegeRowParseResult::Comment;
    }

    if row.trim().is_empty() {
        return PrivilegeRowParseResult::Empty;
    }

    let parts: Vec<&str> = row.trim().split_ascii_whitespace().collect();

    match parts.len() {
        n if (n < DATABASE_PRIVILEGE_FIELDS.len()) => {
            return PrivilegeRowParseResult::TooFewFields(n)
        }
        n if (n > DATABASE_PRIVILEGE_FIELDS.len()) => {
            return PrivilegeRowParseResult::TooManyFields(n)
        }
        _ => {}
    }

    if row_is_header(row) {
        return PrivilegeRowParseResult::Header;
    }

    let row = DatabasePrivilegeRow {
        db: (*parts.first().unwrap()).to_owned(),
        user: (*parts.get(1).unwrap()).to_owned(),
        select_priv: match parse_privilege(parts.get(2).unwrap(), DATABASE_PRIVILEGE_FIELDS[2]) {
            Ok(p) => p,
            Err(e) => return PrivilegeRowParseResult::ParserError(e),
        },
        insert_priv: match parse_privilege(parts.get(3).unwrap(), DATABASE_PRIVILEGE_FIELDS[3]) {
            Ok(p) => p,
            Err(e) => return PrivilegeRowParseResult::ParserError(e),
        },
        update_priv: match parse_privilege(parts.get(4).unwrap(), DATABASE_PRIVILEGE_FIELDS[4]) {
            Ok(p) => p,
            Err(e) => return PrivilegeRowParseResult::ParserError(e),
        },
        delete_priv: match parse_privilege(parts.get(5).unwrap(), DATABASE_PRIVILEGE_FIELDS[5]) {
            Ok(p) => p,
            Err(e) => return PrivilegeRowParseResult::ParserError(e),
        },
        create_priv: match parse_privilege(parts.get(6).unwrap(), DATABASE_PRIVILEGE_FIELDS[6]) {
            Ok(p) => p,
            Err(e) => return PrivilegeRowParseResult::ParserError(e),
        },
        drop_priv: match parse_privilege(parts.get(7).unwrap(), DATABASE_PRIVILEGE_FIELDS[7]) {
            Ok(p) => p,
            Err(e) => return PrivilegeRowParseResult::ParserError(e),
        },
        alter_priv: match parse_privilege(parts.get(8).unwrap(), DATABASE_PRIVILEGE_FIELDS[8]) {
            Ok(p) => p,
            Err(e) => return PrivilegeRowParseResult::ParserError(e),
        },
        index_priv: match parse_privilege(parts.get(9).unwrap(), DATABASE_PRIVILEGE_FIELDS[9]) {
            Ok(p) => p,
            Err(e) => return PrivilegeRowParseResult::ParserError(e),
        },
        create_tmp_table_priv: match parse_privilege(
            parts.get(10).unwrap(),
            DATABASE_PRIVILEGE_FIELDS[10],
        ) {
            Ok(p) => p,
            Err(e) => return PrivilegeRowParseResult::ParserError(e),
        },
        lock_tables_priv: match parse_privilege(
            parts.get(11).unwrap(),
            DATABASE_PRIVILEGE_FIELDS[11],
        ) {
            Ok(p) => p,
            Err(e) => return PrivilegeRowParseResult::ParserError(e),
        },
        references_priv: match parse_privilege(
            parts.get(12).unwrap(),
            DATABASE_PRIVILEGE_FIELDS[12],
        ) {
            Ok(p) => p,
            Err(e) => return PrivilegeRowParseResult::ParserError(e),
        },
    };

    PrivilegeRowParseResult::PrivilegeRow(row)
}

// TODO: return better errors

pub fn parse_privilege_data_from_editor_content(
    content: String,
) -> anyhow::Result<Vec<DatabasePrivilegeRow>> {
    content
        .trim()
        .split('\n')
        .map(|line| line.trim())
        .map(parse_privilege_row_from_editor)
        .map(|result| match result {
            PrivilegeRowParseResult::PrivilegeRow(row) => Ok(Some(row)),
            PrivilegeRowParseResult::ParserError(e) => Err(e),
            PrivilegeRowParseResult::TooFewFields(n) => Err(anyhow!(
                "Too few fields in line. Expected to find {} fields, found {}",
                DATABASE_PRIVILEGE_FIELDS.len(),
                n
            )),
            PrivilegeRowParseResult::TooManyFields(n) => Err(anyhow!(
                "Too many fields in line. Expected to find {} fields, found {}",
                DATABASE_PRIVILEGE_FIELDS.len(),
                n
            )),
            PrivilegeRowParseResult::Header => Ok(None),
            PrivilegeRowParseResult::Comment => Ok(None),
            PrivilegeRowParseResult::Empty => Ok(None),
        })
        .filter_map(|result| result.transpose())
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, PartialOrd, Ord)]
pub struct DatabasePrivilegeRowDiff {
    pub db: String,
    pub user: String,
    pub diff: BTreeSet<DatabasePrivilegeChange>,
}

/// This enum represents a change for a single privilege.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, PartialOrd, Ord)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, PartialOrd, Ord)]
pub enum DatabasePrivilegesDiff {
    New(DatabasePrivilegeRow),
    Modified(DatabasePrivilegeRowDiff),
    Deleted(DatabasePrivilegeRow),
}

/// T
pub fn diff_privileges(
    from: &[DatabasePrivilegeRow],
    to: &[DatabasePrivilegeRow],
) -> BTreeSet<DatabasePrivilegesDiff> {
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

    let mut result = BTreeSet::new();

    for p in to {
        if let Some(old_p) = from_lookup_table.get(&(p.db.clone(), p.user.clone())) {
            let diff = old_p.diff(p);
            if !diff.diff.is_empty() {
                result.insert(DatabasePrivilegesDiff::Modified(diff));
            }
        } else {
            result.insert(DatabasePrivilegesDiff::New(p.clone()));
        }
    }

    for p in from {
        if !to_lookup_table.contains_key(&(p.db.clone(), p.user.clone())) {
            result.insert(DatabasePrivilegesDiff::Deleted(p.clone()));
        }
    }

    result
}

/// Uses the resulting diffs to make modifications to the database.
pub async fn apply_privilege_diffs(
    diffs: BTreeSet<DatabasePrivilegesDiff>,
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
        let row_to_be_modified = DatabasePrivilegeRow {
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
        };

        let mut row_to_be_deleted = row_to_be_modified.clone();
        "user2".clone_into(&mut row_to_be_deleted.user);

        let from = vec![row_to_be_modified.clone(), row_to_be_deleted.clone()];

        let mut modified_row = row_to_be_modified.clone();
        modified_row.select_priv = false;
        modified_row.insert_priv = false;
        modified_row.index_priv = true;

        let mut new_row = row_to_be_modified.clone();
        "user3".clone_into(&mut new_row.user);

        let to = vec![modified_row.clone(), new_row.clone()];

        let diffs = diff_privileges(&from, &to);

        assert_eq!(
            diffs,
            BTreeSet::from_iter(vec![
                DatabasePrivilegesDiff::Deleted(row_to_be_deleted),
                DatabasePrivilegesDiff::Modified(DatabasePrivilegeRowDiff {
                    db: "db".to_owned(),
                    user: "user".to_owned(),
                    diff: BTreeSet::from_iter(vec![
                        DatabasePrivilegeChange::YesToNo("select_priv".to_owned()),
                        DatabasePrivilegeChange::YesToNo("insert_priv".to_owned()),
                        DatabasePrivilegeChange::NoToYes("index_priv".to_owned()),
                    ]),
                }),
                DatabasePrivilegesDiff::New(new_row),
            ])
        );
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
