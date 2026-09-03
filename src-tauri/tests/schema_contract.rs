//! Every SQL statement in the application, checked against the schema it will
//! actually run against.
//!
//! ZANPOS builds its queries as runtime strings, not through `sqlx::query!`, so
//! the compiler never sees them. A column that is renamed in a migration, or was
//! never there at all, produces code that compiles, links, ships, and fails the
//! first time a customer reaches it. Several already had:
//!
//! * `delivery_orders.shift_id` — the guard blocking a shift close while
//!   cash-on-delivery is outstanding. The column has never existed; the query's
//!   `.unwrap_or(0)` read the error as "nothing pending", so the guard reported
//!   all-clear by failing.
//! * `sales.sale_status`, `sales.total_minor`, `shifts.started_at`,
//!   `shifts.ended_at`, `users.name`, `sales.user_id` — six proactive alert
//!   rules, each wrapped in `.unwrap_or_default()`, silently returning no alerts
//!   every cycle.
//! * `ai_actions.created_at` / `completed_at` / `action_type` — the stuck-action
//!   detector and both of its repair paths.
//! * `products.price_minor`, `products.track_stock`, `shifts.notes` — two ZanAI
//!   tools that could never complete.
//!
//! The pattern in almost every case was the same: the query failed, a
//! `.unwrap_or` turned the failure into a benign-looking value, and the feature
//! reported success while doing nothing. That is why this checks the SQL rather
//! than trusting the runtime to complain.
//!
//! Scope is deliberately what can be checked exactly. Statements assembled with
//! `format!` are skipped — guessing at their shape would produce false failures,
//! which is how a test like this gets disabled.

use sqlx::SqlitePool;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The schema as a fresh install actually gets it — migrated, not hand-listed.
async fn live_schema() -> BTreeMap<String, BTreeSet<String>> {
    let path = std::env::temp_dir().join(format!("zanpos_contract_{}.db", ulid::Ulid::new()));
    let pool = SqlitePool::connect(&format!("sqlite:{}?mode=rwc", path.display()))
        .await
        .expect("open schema database");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("migrations must apply to an empty database");

    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_all(&pool)
    .await
    .expect("read table list");

    let mut schema = BTreeMap::new();
    for table in tables {
        let cols: Vec<String> =
            sqlx::query_scalar(&format!("SELECT name FROM pragma_table_info('{table}')"))
                .fetch_all(&pool)
                .await
                .unwrap_or_default();
        schema.insert(table, cols.into_iter().collect());
    }
    schema
}

fn source_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if !name.starts_with('.') && name != "target" {
                    stack.push(path);
                }
            } else if name.ends_with(".rs") && name != "tests.rs" && !name.ends_with("_tests.rs") {
                // A module that declares itself test-only is not production code
                // however it is named. `db::invariants` is a directory module, so
                // it cannot use the `_tests.rs` suffix to say so.
                if !std::fs::read_to_string(&path)
                    .unwrap_or_default()
                    .contains("#![cfg(test)]")
                {
                    out.push(path);
                }
            }
        }
    }
    out
}

/// Production Rust with `#[cfg(test)]` items removed. Fixtures legitimately
/// write shapes production never does.
fn production_text(path: &Path) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let without_tests = strip_cfg_test(&text);
    // Line comments carry example SQL in this codebase (the migration agent's
    // prompt documents the schema in prose), which is not executed.
    without_tests
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn strip_cfg_test(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    const ATTR: &str = "#[cfg(test)]";
    while let Some(at) = rest.find(ATTR) {
        out.push_str(&rest[..at]);
        let after = &rest[at + ATTR.len()..];
        match (after.find('{'), after.find(';')) {
            (brace, Some(semi)) if brace.map_or(true, |b| semi < b) => rest = &after[semi + 1..],
            (Some(brace), _) => {
                let mut depth = 0usize;
                let mut end = after.len();
                for (i, ch) in after[brace..].char_indices() {
                    match ch {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                end = brace + i + 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                rest = &after[end..];
            }
            _ => rest = after,
        }
    }
    out.push_str(rest);
    out
}

fn flatten(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `INSERT INTO t (a, b, c)` inside one SQL string literal.
///
/// Parsed per literal rather than across the flattened file. Scanning the whole
/// file let an `UPDATE … SET` with no `WHERE` run off the end of its own string
/// and start reading Rust source as column names.
fn insert_columns(statement: &str) -> Option<(String, Vec<String>)> {
    let upper = statement.to_uppercase();
    if !upper.trim_start().starts_with("INSERT") {
        return None;
    }
    let into = upper.find("INTO ")?;
    let after = &statement[into + 5..];
    let table: String = after
        .trim_start()
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    let rest = after.trim_start();
    let paren = rest[table.len()..].trim_start();
    if !paren.starts_with('(') {
        return None;
    }
    let close = paren.find(')')?;
    let cols = paren[1..close]
        .split(',')
        .map(|c| c.trim().trim_matches('"').to_string())
        .filter(|c| !c.is_empty() && c.chars().all(|ch| ch.is_alphanumeric() || ch == '_'))
        .collect();
    Some((table, cols))
}

/// `UPDATE t SET a = ?, b = ?` inside one SQL string literal, up to `WHERE`.
fn update_columns(statement: &str) -> Option<(String, Vec<String>)> {
    let upper = statement.to_uppercase();
    if !upper.trim_start().starts_with("UPDATE ") {
        return None;
    }
    let body = statement.trim_start();
    let after_kw = &body[7..];
    let table: String = after_kw
        .trim_start()
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    let tail = after_kw.trim_start();
    let tail = &tail[table.len()..];
    let tail_upper = tail.to_uppercase();
    let set_at = tail_upper.find("SET ")?;
    // Only a table alias may sit between the name and SET.
    if set_at > 12 {
        return None;
    }
    let assignments = &tail[set_at + 4..];
    let au = assignments.to_uppercase();
    let end = au
        .find(" WHERE ")
        .or_else(|| au.find(" RETURNING "))
        .unwrap_or(assignments.len());
    let cols = assignments[..end]
        .split(',')
        .filter_map(|a| a.split('=').next())
        .map(|c| c.trim().trim_matches('"').to_string())
        .filter(|c| !c.is_empty() && c.chars().all(|ch| ch.is_alphanumeric() || ch == '_'))
        .collect();
    Some((table, cols))
}

/// Every literal write in the application names columns that exist.
///
/// Writes first because they are unambiguous and because a failed write is the
/// half that loses data rather than merely failing to find it.
#[tokio::test]
async fn every_literal_write_names_columns_that_exist() {
    let schema = live_schema().await;
    let mut problems: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let (mut inserts, mut updates) = (0usize, 0usize);

    for file in source_files(&crate_root().join("src")) {
        let label = file
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let flat = flatten(&production_text(&file));

        for statement in string_literals(&flat) {
            if let Some((table, cols)) = insert_columns(&statement) {
                if let Some(known) = schema.get(&table) {
                    inserts += 1;
                    for col in cols {
                        if !known.contains(&col) {
                            problems
                                .entry(format!("{table}.{col}"))
                                .or_default()
                                .insert(format!("{label} (INSERT)"));
                        }
                    }
                }
            }
            if let Some((table, cols)) = update_columns(&statement) {
                if let Some(known) = schema.get(&table) {
                    updates += 1;
                    for col in cols {
                        if !known.contains(&col) {
                            problems
                                .entry(format!("{table}.{col}"))
                                .or_default()
                                .insert(format!("{label} (UPDATE)"));
                        }
                    }
                }
            }
        }
    }

    assert!(
        inserts > 150 && updates > 100,
        "the statement scanner found only {inserts} INSERTs and {updates} UPDATEs — \
         it has stopped matching, and this test is passing by checking nothing"
    );
    let report: Vec<String> = problems
        .iter()
        .map(|(col, files)| format!("{col}  {files:?}"))
        .collect();
    assert!(
        report.is_empty(),
        "{} column(s) written that the migrated schema does not have:\n  {}",
        report.len(),
        report.join("\n  ")
    );
}

/// Qualified references — `s.status`, `pp.price_minor` — resolved through the
/// aliases the statement itself declares.
#[tokio::test]
async fn every_qualified_column_reference_resolves() {
    let schema = live_schema().await;
    let mut problems: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut checked = 0usize;

    for file in source_files(&crate_root().join("src")) {
        let label = file
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let flat = flatten(&production_text(&file));

        for statement in string_literals(&flat) {
            let upper = statement.to_uppercase();
            if !(upper.starts_with("SELECT")
                || upper.starts_with("UPDATE")
                || upper.starts_with("DELETE"))
            {
                continue;
            }
            let aliases = aliases_in(&statement, &schema);
            for (alias, column) in qualified_refs(&statement) {
                let Some(table) = aliases.get(&alias) else {
                    continue;
                };
                checked += 1;
                let known = &schema[table];
                if !known.contains(&column) && column != "rowid" {
                    problems
                        .entry(format!("{table}.{column}"))
                        .or_default()
                        .insert(label.clone());
                }
            }
        }
    }

    assert!(
        checked > 1000,
        "only {checked} qualified references resolved — the scanner is broken and \
         this test is passing vacuously"
    );
    let report: Vec<String> = problems
        .iter()
        .map(|(col, files)| format!("{col}  {files:?}"))
        .collect();
    assert!(
        report.is_empty(),
        "{} qualified reference(s) to columns the schema does not have:\n  {}",
        report.len(),
        report.join("\n  ")
    );
}

fn string_literals(flat: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes: Vec<char> = flat.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == '"' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && bytes[j] != '"' {
                j += 1;
            }
            if j > start + 20 {
                out.push(bytes[start..j].iter().collect());
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    out
}

/// Table names and aliases bound by `FROM t` / `JOIN t a` in one statement.
fn aliases_in(
    statement: &str,
    schema: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeMap<String, String> {
    const RESERVED: &[&str] = &[
        "ON", "WHERE", "SET", "LEFT", "RIGHT", "INNER", "OUTER", "JOIN", "GROUP", "ORDER", "LIMIT",
        "AS", "USING", "SELECT", "VALUES", "CROSS", "NATURAL",
    ];
    let mut out = BTreeMap::new();
    let words: Vec<&str> = statement.split_whitespace().collect();
    for (i, word) in words.iter().enumerate() {
        let upper = word.to_uppercase();
        if upper != "FROM" && upper != "JOIN" {
            continue;
        }
        let Some(raw) = words.get(i + 1) else {
            continue;
        };
        let table: String = raw
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !schema.contains_key(&table) {
            continue;
        }
        out.insert(table.clone(), table.clone());
        let mut next = i + 2;
        if words.get(next).map(|w| w.to_uppercase()) == Some("AS".into()) {
            next += 1;
        }
        if let Some(candidate) = words.get(next) {
            let alias: String = candidate
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !alias.is_empty() && !RESERVED.contains(&alias.to_uppercase().as_str()) {
                out.insert(alias, table);
            }
        }
    }
    out
}

fn qualified_refs(statement: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let chars: Vec<char> = statement.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '.' && i > 0 {
            let mut a = i;
            while a > 0 && (chars[a - 1].is_alphanumeric() || chars[a - 1] == '_') {
                a -= 1;
            }
            let mut b = i + 1;
            while b < chars.len() && (chars[b].is_alphanumeric() || chars[b] == '_') {
                b += 1;
            }
            let alias: String = chars[a..i].iter().collect();
            let column: String = chars[i + 1..b].iter().collect();
            if !alias.is_empty()
                && !column.is_empty()
                && alias.chars().next().is_some_and(|c| c.is_alphabetic())
            {
                out.push((alias, column));
            }
            i = b;
        } else {
            i += 1;
        }
    }
    out
}
