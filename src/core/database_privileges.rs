use anyhow::{anyhow, Context};
use itertools::Itertools;
use prettytable::Table;
use serde::{Deserialize, Serialize};
use std::{
    cmp::max,
    collections::{BTreeSet, HashMap},
};

use super::common::{rev_yn, yn};
use crate::server::sql::database_privilege_operations::{
    DatabasePrivilegeRow, DATABASE_PRIVILEGE_FIELDS,
};

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

pub fn diff(row1: &DatabasePrivilegeRow, row2: &DatabasePrivilegeRow) -> DatabasePrivilegeRowDiff {
    debug_assert!(row1.db == row2.db && row1.user == row2.user);

    DatabasePrivilegeRowDiff {
        db: row1.db.clone(),
        user: row1.user.clone(),
        diff: DATABASE_PRIVILEGE_FIELDS
            .into_iter()
            .skip(2)
            .filter_map(|field| {
                DatabasePrivilegeChange::new(
                    row1.get_privilege_by_name(field),
                    row2.get_privilege_by_name(field),
                    field,
                )
            })
            .collect(),
    }
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
/* EDITOR CONTENT DISPLAY/DISPLAY */
/**********************************/

/// Generates a single row of the privileges table for the editor.
pub fn format_privileges_line_for_editor(
    privs: &DatabasePrivilegeRow,
    username_len: usize,
    database_name_len: usize,
) -> String {
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
    database_name: Option<&str>,
) -> String {
    let example_user = format!("{}_user", unix_user);
    let example_db = database_name
        .unwrap_or(&format!("{}_db", unix_user))
        .to_string();

    // NOTE: `.max()`` fails when the iterator is empty.
    //       In this case, we know that the only fields in the
    //       editor will be the example user and example db name.
    //       Hence, it's put as the fallback value, despite not really
    //       being a "fallback" in the normal sense.
    let longest_username = max(
        privilege_data
            .iter()
            .map(|p| p.user.len())
            .max()
            .unwrap_or(example_user.len()),
        "User".len(),
    );

    let longest_database_name = max(
        privilege_data
            .iter()
            .map(|p| p.db.len())
            .max()
            .unwrap_or(example_db.len()),
        "Database".len(),
    );

    let mut header: Vec<_> = DATABASE_PRIVILEGE_FIELDS
        .into_iter()
        .map(db_priv_field_human_readable_name)
        .collect();

    // Pad the first two columns with spaces to align the privileges.
    header[0] = format!("{:width$}", header[0], width = longest_database_name);
    header[1] = format!("{:width$}", header[1], width = longest_username);

    let example_line = format_privileges_line_for_editor(
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
                .map(|privs| {
                    format_privileges_line_for_editor(
                        privs,
                        longest_username,
                        longest_database_name,
                    )
                })
                .join("\n")
        }
    )
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
fn parse_privilege_cell_from_editor(yn: &str, name: &str) -> anyhow::Result<bool> {
    rev_yn(yn)
        .ok_or_else(|| anyhow!("Expected Y or N, found {}", yn))
        .context(format!("Could not parse {} privilege", name))
}

#[inline]
fn editor_row_is_header(row: &str) -> bool {
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

    if editor_row_is_header(row) {
        return PrivilegeRowParseResult::Header;
    }

    let row = DatabasePrivilegeRow {
        db: (*parts.first().unwrap()).to_owned(),
        user: (*parts.get(1).unwrap()).to_owned(),
        select_priv: match parse_privilege_cell_from_editor(
            parts.get(2).unwrap(),
            DATABASE_PRIVILEGE_FIELDS[2],
        ) {
            Ok(p) => p,
            Err(e) => return PrivilegeRowParseResult::ParserError(e),
        },
        insert_priv: match parse_privilege_cell_from_editor(
            parts.get(3).unwrap(),
            DATABASE_PRIVILEGE_FIELDS[3],
        ) {
            Ok(p) => p,
            Err(e) => return PrivilegeRowParseResult::ParserError(e),
        },
        update_priv: match parse_privilege_cell_from_editor(
            parts.get(4).unwrap(),
            DATABASE_PRIVILEGE_FIELDS[4],
        ) {
            Ok(p) => p,
            Err(e) => return PrivilegeRowParseResult::ParserError(e),
        },
        delete_priv: match parse_privilege_cell_from_editor(
            parts.get(5).unwrap(),
            DATABASE_PRIVILEGE_FIELDS[5],
        ) {
            Ok(p) => p,
            Err(e) => return PrivilegeRowParseResult::ParserError(e),
        },
        create_priv: match parse_privilege_cell_from_editor(
            parts.get(6).unwrap(),
            DATABASE_PRIVILEGE_FIELDS[6],
        ) {
            Ok(p) => p,
            Err(e) => return PrivilegeRowParseResult::ParserError(e),
        },
        drop_priv: match parse_privilege_cell_from_editor(
            parts.get(7).unwrap(),
            DATABASE_PRIVILEGE_FIELDS[7],
        ) {
            Ok(p) => p,
            Err(e) => return PrivilegeRowParseResult::ParserError(e),
        },
        alter_priv: match parse_privilege_cell_from_editor(
            parts.get(8).unwrap(),
            DATABASE_PRIVILEGE_FIELDS[8],
        ) {
            Ok(p) => p,
            Err(e) => return PrivilegeRowParseResult::ParserError(e),
        },
        index_priv: match parse_privilege_cell_from_editor(
            parts.get(9).unwrap(),
            DATABASE_PRIVILEGE_FIELDS[9],
        ) {
            Ok(p) => p,
            Err(e) => return PrivilegeRowParseResult::ParserError(e),
        },
        create_tmp_table_priv: match parse_privilege_cell_from_editor(
            parts.get(10).unwrap(),
            DATABASE_PRIVILEGE_FIELDS[10],
        ) {
            Ok(p) => p,
            Err(e) => return PrivilegeRowParseResult::ParserError(e),
        },
        lock_tables_priv: match parse_privilege_cell_from_editor(
            parts.get(11).unwrap(),
            DATABASE_PRIVILEGE_FIELDS[11],
        ) {
            Ok(p) => p,
            Err(e) => return PrivilegeRowParseResult::ParserError(e),
        },
        references_priv: match parse_privilege_cell_from_editor(
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

/*****************************/
/* CALCULATE PRIVILEGE DIFFS */
/*****************************/

/// This struct represents encapsulates the differences between two
/// instances of privilege sets for a single user on a single database.
///
/// The `User` and `Database` are the same for both instances.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub struct DatabasePrivilegeRowDiff {
    pub db: String,
    pub user: String,
    pub diff: BTreeSet<DatabasePrivilegeChange>,
}

/// This enum represents a change for a single privilege.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub enum DatabasePrivilegesDiff {
    New(DatabasePrivilegeRow),
    Modified(DatabasePrivilegeRowDiff),
    Deleted(DatabasePrivilegeRow),
}

impl DatabasePrivilegesDiff {
    pub fn get_database_name(&self) -> &str {
        match self {
            DatabasePrivilegesDiff::New(p) => &p.db,
            DatabasePrivilegesDiff::Modified(p) => &p.db,
            DatabasePrivilegesDiff::Deleted(p) => &p.db,
        }
    }

    pub fn get_user_name(&self) -> &str {
        match self {
            DatabasePrivilegesDiff::New(p) => &p.user,
            DatabasePrivilegesDiff::Modified(p) => &p.user,
            DatabasePrivilegesDiff::Deleted(p) => &p.user,
        }
    }
}

/// This function calculates the differences between two sets of database privileges.
/// It returns a set of [`DatabasePrivilegesDiff`] that can be used to display or
/// apply a set of privilege modifications to the database.
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
            let diff = diff(old_p, p);
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

fn display_privilege_cell(diff: &DatabasePrivilegeRowDiff) -> String {
    diff.diff
        .iter()
        .map(|change| match change {
            DatabasePrivilegeChange::YesToNo(name) => {
                format!("{}: Y -> N", db_priv_field_human_readable_name(name))
            }
            DatabasePrivilegeChange::NoToYes(name) => {
                format!("{}: N -> Y", db_priv_field_human_readable_name(name))
            }
        })
        .join("\n")
}

fn display_new_privileges_list(row: &DatabasePrivilegeRow) -> String {
    DATABASE_PRIVILEGE_FIELDS
        .into_iter()
        .skip(2)
        .map(|field| {
            if row.get_privilege_by_name(field) {
                format!("{}: Y", db_priv_field_human_readable_name(field))
            } else {
                format!("{}: N", db_priv_field_human_readable_name(field))
            }
        })
        .join("\n")
}

/// Displays the difference between two sets of database privileges.
pub fn display_privilege_diffs(diffs: &BTreeSet<DatabasePrivilegesDiff>) -> String {
    let mut table = Table::new();
    table.set_titles(row!["Database", "User", "Privilege diff",]);
    for row in diffs {
        match row {
            DatabasePrivilegesDiff::New(p) => {
                table.add_row(row![
                    p.db,
                    p.user,
                    "(Previously unprivileged)\n".to_string() + &display_new_privileges_list(p)
                ]);
            }
            DatabasePrivilegesDiff::Modified(p) => {
                table.add_row(row![p.db, p.user, display_privilege_cell(p),]);
            }
            DatabasePrivilegesDiff::Deleted(p) => {
                table.add_row(row![p.db, p.user, "Removed".to_string()]);
            }
        }
    }

    table.to_string()
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

        let content = generate_editor_content_from_privilege_data(&permissions, "user", None);

        let parsed_permissions = parse_privilege_data_from_editor_content(content).unwrap();

        assert_eq!(permissions, parsed_permissions);
    }
}
