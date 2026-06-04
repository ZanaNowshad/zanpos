// ─── Imports ──────────────────────────────────────────────────────────────────
use crate::commands::rbac;
use crate::ai::client::ToolDef;
use crate::ai::provider::{Provider, ToolCallResult};
use crate::db::repositories::audit_hash;
use crate::domain::ai_admin::ChatMessage;
use crate::errors::{AppError, AppResult};
use crate::AppState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::collections::HashMap;
use tauri::State;
use ulid::Ulid;

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSchema {
    pub file_path: String,
    pub file_type: String, // "csv" | "xlsx" | "sqlite"
    pub sheets: Vec<SheetSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SheetSchema {
    pub name: String,
    pub columns: Vec<ColumnSchema>,
    pub row_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnSchema {
    pub name: String,
    pub samples: Vec<String>, // up to 3 non-empty values
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingConfig {
    pub sheet_mappings: Vec<SheetMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SheetMapping {
    pub source_sheet: String,
    pub target_table: String, // "products"|"categories"|"customers"|"sales"|"sale_items"|"stock_levels"|"skip"
    pub column_mappings: Vec<ColumnMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnMapping {
    pub source_col: String,
    pub target_col: String, // ZANPOS column name, or "skip"
    pub transform: String,  // "identity"|"price_minor"|"integer"|"boolean"|"date"|"phone"|"skip"
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MigrationProgress {
    Started { total_sheets: usize },
    SheetStart { sheet: String, target: String, total_rows: usize },
    SheetProgress { sheet: String, done: usize, total: usize },
    SheetDone { sheet: String, target: String, inserted: usize, skipped: usize },
    Done { message: String },
    Error { message: String },
}

/// Parse a decimal price string (e.g. "12.500") to minor units using integer arithmetic.
/// Avoids IEEE 754 floating-point precision issues (e.g. 0.1 + 0.2 != 0.3).
/// Returns None if the value is out of range or not a valid number.
fn parse_price_string(s: &str, exp: u32) -> Option<i64> {
    crate::domain::money::parse_major_to_minor(s, exp)
}

// ─── Security helpers ─────────────────────────────────────────────────────────

/// Returns true if the path is within an approved root for the migration agent.
/// Rejects sensitive Windows directories even if they are children of approved roots.
fn is_safe_read_path(path: &str) -> bool {
    let p = match std::fs::canonicalize(path) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let p_str = p.to_string_lossy().to_lowercase();

    // Deny sensitive paths unconditionally
    let denied_substrings = [
        r"\.ssh",
        r"\openssh",
        r"\.aws",
        r"\microsoft\credentials",
        r"\microsoft\crypto",
        r"\google\chrome\user data",
        r"\mozilla\firefox\profiles",
        r"\edge\user data",
        r"\appdata\roaming\microsoft\protect",
        r"\windows\system32",
        r"\windows\syswow64",
    ];
    if denied_substrings.iter().any(|d| p_str.contains(d)) {
        return false;
    }

    // Build approved roots from env vars (same pattern as migration_find_db_files)
    let mut approved_roots: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(appdata) = std::env::var("APPDATA") {
        approved_roots.push(std::path::PathBuf::from(appdata));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        approved_roots.push(std::path::PathBuf::from(local));
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        let base = std::path::PathBuf::from(&profile);
        approved_roots.push(base.join("Desktop"));
        approved_roots.push(base.join("Documents"));
        approved_roots.push(base.join("Downloads"));
    }
    approved_roots.push(std::path::PathBuf::from("C:\\Program Files"));
    approved_roots.push(std::path::PathBuf::from("C:\\Program Files (x86)"));

    // Canonicalize each root so comparisons are reliable
    let canonical_roots: Vec<std::path::PathBuf> = approved_roots
        .iter()
        .filter_map(|r| std::fs::canonicalize(r).ok().or_else(|| Some(r.clone())))
        .collect();

    canonical_roots.iter().any(|root| p.starts_with(root))
}

/// Returns true if a name is safe to interpolate as a SQL table or column identifier.
/// Allows alphanumeric, underscore, and space (for display names); rejects everything else.
fn is_safe_sql_identifier(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// ─── Command 1: migration_inspect_file ───────────────────────────────────────

#[tauri::command]
pub async fn migration_inspect_file(path: String) -> AppResult<FileSchema> {
    let lower = path.to_lowercase();
    if lower.ends_with(".csv") {
        inspect_csv(path).await
    } else if lower.ends_with(".xlsx")
        || lower.ends_with(".xls")
        || lower.ends_with(".xlsm")
    {
        inspect_excel(path).await
    } else if lower.ends_with(".db")
        || lower.ends_with(".sqlite")
        || lower.ends_with(".sqlite3")
        || lower.ends_with(".db3")
        || lower.ends_with(".s3db")
    {
        inspect_sqlite(path).await
    } else if lower.ends_with(".sql") {
        inspect_sql_dump(path).await
    } else if lower.ends_with(".json") {
        inspect_json(path).await
    } else {
        // Try each format in order
        let p2 = path.clone();
        let p3 = path.clone();
        let p4 = path.clone();
        match inspect_sqlite(path).await {
            Ok(s) => Ok(s),
            Err(_) => match inspect_excel(p2).await {
                Ok(s) => Ok(s),
                Err(_) => match inspect_sql_dump(p3).await {
                    Ok(s) => Ok(s),
                    Err(_) => inspect_csv(p4).await,
                },
            },
        }
    }
}

async fn inspect_csv(path: String) -> AppResult<FileSchema> {
    tokio::task::spawn_blocking(move || {
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_path(&path)
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let headers = rdr
            .headers()
            .map_err(|e| AppError::Internal(e.to_string()))?
            .clone();

        let col_names: Vec<String> = headers.iter().map(|s| s.to_string()).collect();

        // Sample up to 3 rows
        let sample_rows: Vec<Vec<String>> = rdr
            .records()
            .take(3)
            .filter_map(|r| r.ok())
            .map(|r| r.iter().map(|f| f.to_string()).collect())
            .collect();

        // Estimate row count from file size / avg bytes per row
        let row_count = std::fs::metadata(&path)
            .map(|m| {
                let avg = if sample_rows.is_empty() {
                    50
                } else {
                    sample_rows
                        .iter()
                        .map(|r| r.join(",").len() + 2)
                        .sum::<usize>()
                        / sample_rows.len()
                };
                m.len() as usize / avg.max(1)
            })
            .unwrap_or(0);

        let columns = col_names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let samples: Vec<String> = sample_rows
                    .iter()
                    .filter_map(|row| row.get(i))
                    .filter(|v| !v.is_empty())
                    .take(3)
                    .cloned()
                    .collect();
                ColumnSchema { name: name.clone(), samples }
            })
            .collect();

        Ok(FileSchema {
            file_path: path.clone(),
            file_type: "csv".into(),
            sheets: vec![SheetSchema {
                name: std::path::Path::new(&path)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Sheet1".into()),
                columns,
                row_count,
            }],
        })
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

async fn inspect_excel(path: String) -> AppResult<FileSchema> {
    tokio::task::spawn_blocking(move || {
        use calamine::Reader;
        let mut wb = calamine::open_workbook_auto(&path)
            .map_err(|e| AppError::Internal(format!("Could not open Excel: {}", e)))?;

        let sheet_names = wb.sheet_names().to_vec();
        let mut sheets = Vec::new();

        for sheet_name in &sheet_names {
            let range = match wb.worksheet_range(sheet_name) {
                Ok(r) => r,
                Err(_) => continue,
            };

            // get_size() reads dimensions from metadata — no full row scan
            let (height, _) = range.get_size();
            let row_count = height.saturating_sub(1);

            let mut rows_iter = range.rows();

            // Only iterate first row for headers
            let header_cells: Vec<String> = rows_iter
                .next()
                .map(|r| r.iter().map(|c| c.to_string()).collect())
                .unwrap_or_default();

            // Skip sheets where all headers are empty
            if header_cells.iter().all(|h| h.trim().is_empty()) {
                continue;
            }

            // Sample up to 3 data rows
            let sample_rows: Vec<Vec<String>> = rows_iter
                .take(3)
                .map(|r| r.iter().map(|c| c.to_string()).collect())
                .collect();

            let columns = header_cells
                .iter()
                .enumerate()
                .filter(|(_, h)| !h.trim().is_empty())
                .map(|(i, name)| {
                    let samples: Vec<String> = sample_rows
                        .iter()
                        .filter_map(|row| row.get(i))
                        .filter(|v| !v.is_empty())
                        .take(3)
                        .cloned()
                        .collect();
                    ColumnSchema { name: name.clone(), samples }
                })
                .collect();

            sheets.push(SheetSchema {
                name: sheet_name.clone(),
                columns,
                row_count,
            });
        }

        let ext = std::path::Path::new(&path)
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_else(|| "xlsx".into());

        Ok(FileSchema {
            file_path: path,
            file_type: ext.to_string(),
            sheets,
        })
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

async fn inspect_sqlite(path: String) -> AppResult<FileSchema> {
    let url = format!("sqlite:{}?mode=ro", path);
    let pool = sqlx::SqlitePool::connect(&url)
        .await
        .map_err(|e| AppError::Internal(format!("Could not open SQLite: {}", e)))?;

    let table_names: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .fetch_all(&pool)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

    let mut sheets = Vec::new();

    for table in &table_names {
        // Finding 5: reject table names that contain SQL-injection characters
        if !is_safe_sql_identifier(table) {
            tracing::warn!("inspect_sqlite: skipping table '{}' — unsafe identifier", table);
            continue;
        }

        // Get columns via pragma
        let cols: Vec<(String, String)> = sqlx::query_as::<_, (String, String)>(
            &format!("SELECT name, type FROM pragma_table_info('{}')", table),
        )
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

        if cols.is_empty() {
            continue;
        }

        // Row count
        let row_count: i64 =
            sqlx::query_scalar(&format!("SELECT COUNT(*) FROM \"{}\"", table))
                .fetch_one(&pool)
                .await
                .unwrap_or(0);

        // Sample rows
        let sample_rows = sqlx::query(&format!("SELECT * FROM \"{}\" LIMIT 3", table))
            .fetch_all(&pool)
            .await
            .unwrap_or_default();

        let columns = cols
            .iter()
            .enumerate()
            .map(|(i, (col_name, _))| {
                use sqlx::Row;
                let samples: Vec<String> = sample_rows
                    .iter()
                    .filter_map(|row| {
                        row.try_get::<String, _>(i)
                            .or_else(|_| row.try_get::<i64, _>(i).map(|v| v.to_string()))
                            .or_else(|_| row.try_get::<f64, _>(i).map(|v| v.to_string()))
                            .ok()
                    })
                    .filter(|v| !v.is_empty())
                    .take(3)
                    .collect();
                ColumnSchema { name: col_name.clone(), samples }
            })
            .collect();

        sheets.push(SheetSchema {
            name: table.clone(),
            columns,
            row_count: row_count as usize,
        });
    }

    pool.close().await;

    Ok(FileSchema {
        file_path: path,
        file_type: "sqlite".into(),
        sheets,
    })
}

// ─── SQL dump inspector ───────────────────────────────────────────────────────

async fn inspect_sql_dump(path: String) -> AppResult<FileSchema> {
    tokio::task::spawn_blocking(move || -> AppResult<FileSchema> {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| AppError::Internal(format!("Cannot read SQL file: {}", e)))?;

        let mut tables: std::collections::HashMap<String, (Vec<ColumnSchema>, usize)> = std::collections::HashMap::new();
        let mut order: Vec<String> = Vec::new();

        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i].trim();
            let upper = line.to_uppercase();

            // Detect CREATE TABLE statement (handles multi-line)
            if upper.starts_with("CREATE TABLE") {
                // Extract table name from this line or next
                let rest = &line[12..].trim(); // after CREATE TABLE
                let rest = if rest.to_uppercase().starts_with("IF NOT EXISTS") {
                    rest[13..].trim()
                } else {
                    rest
                };
                // Name may be quoted with `, ", [, or plain
                let tname = rest
                    .trim_start_matches('`')
                    .trim_start_matches('"')
                    .trim_start_matches('[')
                    .split(|c: char| c.is_whitespace() || c == '(' || c == '`' || c == '"' || c == ']')
                    .next()
                    .unwrap_or("")
                    .trim_matches('`')
                    .trim_matches('"')
                    .trim_matches('[')
                    .trim_matches(']')
                    .to_string();

                if tname.is_empty() || tname.to_uppercase() == "IF" {
                    i += 1;
                    continue;
                }

                // Collect lines until the closing ')' of the column block
                let mut col_block = String::new();
                let mut depth = 0i32;
                let mut j = i;
                while j < lines.len() {
                    let l = lines[j];
                    for c in l.chars() {
                        match c {
                            '(' => depth += 1,
                            ')' => { depth -= 1; if depth <= 0 { break; } }
                            _ => {}
                        }
                    }
                    col_block.push_str(l);
                    col_block.push('\n');
                    if depth <= 0 && col_block.contains('(') { break; }
                    j += 1;
                }

                // Extract column definitions between first ( and last )
                let col_start = col_block.find('(').map(|p| p + 1).unwrap_or(0);
                let col_end = col_block.rfind(')').unwrap_or(col_block.len());
                let col_section = &col_block[col_start..col_end];

                // Split on commas (ignoring nested parens)
                let mut cols: Vec<ColumnSchema> = Vec::new();
                let mut current = String::new();
                let mut nest = 0i32;
                for ch in col_section.chars() {
                    match ch {
                        '(' => { nest += 1; current.push(ch); }
                        ')' => { nest -= 1; current.push(ch); }
                        ',' if nest == 0 => {
                            let trimmed = current.trim().to_string();
                            current.clear();
                            let upper_trim = trimmed.to_uppercase();
                            // Skip constraint/index lines
                            if !upper_trim.starts_with("KEY")
                                && !upper_trim.starts_with("INDEX")
                                && !upper_trim.starts_with("UNIQUE")
                                && !upper_trim.starts_with("PRIMARY")
                                && !upper_trim.starts_with("CONSTRAINT")
                                && !upper_trim.starts_with("FOREIGN")
                                && !upper_trim.starts_with("CHECK")
                                && !trimmed.is_empty()
                            {
                                // First token = column name, strip quotes/brackets
                                let col_name = trimmed
                                    .split_whitespace()
                                    .next()
                                    .unwrap_or("")
                                    .trim_matches('`')
                                    .trim_matches('"')
                                    .trim_matches('[')
                                    .trim_matches(']')
                                    .to_string();
                                if !col_name.is_empty() {
                                    cols.push(ColumnSchema { name: col_name, samples: vec![] });
                                }
                            }
                        }
                        _ => { current.push(ch); }
                    }
                }
                // Handle last column in block
                let trimmed = current.trim().to_string();
                let upper_trim = trimmed.to_uppercase();
                if !trimmed.is_empty()
                    && !upper_trim.starts_with("KEY") && !upper_trim.starts_with("INDEX")
                    && !upper_trim.starts_with("UNIQUE") && !upper_trim.starts_with("PRIMARY")
                    && !upper_trim.starts_with("CONSTRAINT") && !upper_trim.starts_with("FOREIGN")
                {
                    let col_name = trimmed.split_whitespace().next().unwrap_or("")
                        .trim_matches('`').trim_matches('"').trim_matches('[').trim_matches(']').to_string();
                    if !col_name.is_empty() {
                        cols.push(ColumnSchema { name: col_name, samples: vec![] });
                    }
                }

                if !tables.contains_key(&tname) {
                    order.push(tname.clone());
                }
                tables.entry(tname).or_insert((cols, 0));
                i = j + 1;
                continue;
            }

            // Count INSERT INTO rows
            if upper.starts_with("INSERT INTO") || upper.starts_with("INSERT IGNORE INTO") || upper.starts_with("REPLACE INTO") {
                // Extract table name
                let after = if upper.starts_with("INSERT IGNORE INTO") { &line[18..] }
                    else if upper.starts_with("REPLACE INTO") { &line[12..] }
                    else { &line[11..] };
                let tname = after.trim()
                    .trim_start_matches('`').trim_start_matches('"').trim_start_matches('[')
                    .split(|c: char| c.is_whitespace() || c == '(' || c == '`' || c == '"' || c == ']')
                    .next().unwrap_or("")
                    .trim_matches('`').trim_matches('"').trim_matches('[').trim_matches(']')
                    .to_string();
                if let Some(entry) = tables.get_mut(&tname) {
                    entry.1 += 1;
                } else {
                    // INSERT before CREATE TABLE — still count it
                    order.push(tname.clone());
                    tables.insert(tname, (vec![], 1));
                }
            }

            i += 1;
        }

        let sheets = order.iter().filter_map(|name| {
            tables.get(name).map(|(cols, count)| SheetSchema {
                name: name.clone(),
                columns: cols.clone(),
                row_count: *count,
            })
        }).collect();

        Ok(FileSchema { file_path: path, file_type: "sql".into(), sheets })
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

// ─── JSON export inspector ────────────────────────────────────────────────────

async fn inspect_json(path: String) -> AppResult<FileSchema> {
    tokio::task::spawn_blocking(move || -> AppResult<FileSchema> {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| AppError::Internal(format!("Cannot read JSON file: {}", e)))?;

        let v: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| AppError::Internal(format!("Invalid JSON: {}", e)))?;

        let mut sheets: Vec<SheetSchema> = Vec::new();

        match &v {
            // Top-level array of objects: [{"col1":..., "col2":...}, ...]
            serde_json::Value::Array(arr) => {
                let cols = if let Some(serde_json::Value::Object(first)) = arr.first() {
                    first.keys().take(50).map(|k| {
                        let samples: Vec<String> = arr.iter().take(3).filter_map(|row| {
                            row.get(k).map(|v| match v {
                                serde_json::Value::String(s) => s[..s.len().min(40)].to_string(),
                                other => other.to_string()[..other.to_string().len().min(40)].to_string(),
                            })
                        }).collect();
                        ColumnSchema { name: k.clone(), samples }
                    }).collect()
                } else { vec![] };
                sheets.push(SheetSchema { name: "data".into(), columns: cols, row_count: arr.len() });
            }
            // Top-level object with array values: {"products": [...], "customers": [...]}
            serde_json::Value::Object(map) => {
                for (key, val) in map {
                    if let serde_json::Value::Array(arr) = val {
                        let cols = if let Some(serde_json::Value::Object(first)) = arr.first() {
                            first.keys().take(50).map(|k| {
                                let samples: Vec<String> = arr.iter().take(3).filter_map(|row| {
                                    row.get(k).map(|v| match v {
                                        serde_json::Value::String(s) => s[..s.len().min(40)].to_string(),
                                        other => other.to_string()[..other.to_string().len().min(40)].to_string(),
                                    })
                                }).collect();
                                ColumnSchema { name: k.clone(), samples }
                            }).collect()
                        } else { vec![] };
                        sheets.push(SheetSchema { name: key.clone(), columns: cols, row_count: arr.len() });
                    }
                }
                if sheets.is_empty() {
                    // Flat object — treat keys as a single-row sheet
                    let cols = map.keys().take(50).map(|k| ColumnSchema { name: k.clone(), samples: vec![] }).collect();
                    sheets.push(SheetSchema { name: "config".into(), columns: cols, row_count: 1 });
                }
            }
            _ => return Err(AppError::Internal("JSON root must be an array or object".into())),
        }

        Ok(FileSchema { file_path: path, file_type: "json".into(), sheets })
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

// ─── Command 2: migration_ai_map ─────────────────────────────────────────────

#[tauri::command]
pub async fn migration_ai_map(
    schema: FileSchema,
    currency_exponent: u32,
    state: State<'_, AppState>,
) -> AppResult<MappingConfig> {
    let provider = match Provider::from_db(&state.db).await? {
        Some(p) => p,
        None => return Err(AppError::Internal("No AI provider configured".into())),
    };

    // Build compact schema summary
    let mut summary = String::new();
    for sheet in &schema.sheets {
        summary.push_str(&format!(
            "Sheet '{}' ({} rows):\n",
            sheet.name, sheet.row_count
        ));
        for col in &sheet.columns {
            let samples = col.samples.join(", ");
            summary.push_str(&format!("  - {} [samples: {}]\n", col.name, samples));
        }
        summary.push('\n');
    }

    let target_desc = format!(
        r#"ZANPOS target tables:

categories: category_id (auto), name* (TEXT), sort_order (INT), is_active (0/1)
products: product_id (auto), category_id (via category_name lookup), name* (TEXT), barcode, sku, track_inventory (0/1), is_active (0/1)
  note: price goes to product_prices table as price_minor (INT, multiply by 10^{exp})
customers: customer_id (auto), name* (TEXT), phone (E.164), email, loyalty_points (INT), notes
stock_levels: stock_level_id (auto), product_id (via product_name lookup), branch_id, quantity_on_hand (decimal string)
sales: sale_id (auto), receipt_number*, branch_id, shift_id, cashier_user_id, status, net_total_minor (INT), business_date (YYYY-MM-DD)
sale_items: sale_item_id (auto), sale_id, product_id, product_name*, qty (decimal), unit_price_minor (INT), line_total_minor (INT)

Available transforms: identity, price_minor (multiply float by 10^{exp}), integer, boolean, date (→ YYYY-MM-DD), phone (→ E.164), skip
"#,
        exp = currency_exponent
    );

    let example_json = r#"{"sheet_mappings":[{"source_sheet":"Products","target_table":"products","column_mappings":[{"source_col":"ItemName","target_col":"name","transform":"identity","notes":null},{"source_col":"Price","target_col":"price_minor","transform":"price_minor","notes":null},{"source_col":"Barcode","target_col":"barcode","transform":"identity","notes":null},{"source_col":"InternalID","target_col":"skip","transform":"skip","notes":"auto-generated"}]}]}"#;

    let user_message = format!(
        r#"SOURCE FILE SCHEMA:
{}

ZANPOS TARGET TABLES:
{}

RULES:
- Use "skip" for target_table if the sheet has no useful mapping to ZANPOS
- Use "skip" for target_col if the column is not needed
- ID columns are auto-generated — never map source IDs to ZANPOS IDs
- For products, if there is a category column, map it to target_col "category_name" with transform "identity"
- For products, map the price column to target_col "price_minor" with transform "price_minor"

Return a JSON object exactly matching this format:
{}

Only return the raw JSON — no markdown, no explanation."#,
        summary, target_desc, example_json
    );

    let system = "You are a database schema mapping assistant for ZANPOS POS system.\nAnalyze the source schema and produce a column mapping to ZANPOS target tables.\nRespond with ONLY a valid JSON object — no explanation, no markdown fences, just the raw JSON.";

    let result = provider
        .send_chat(system, &[], &user_message, &[])
        .await?;

    let json_str = extract_json_from_text(&result.text);

    let mapping: MappingConfig = serde_json::from_str(&json_str).map_err(|e| {
        tracing::error!(
            "migration_ai_map parse failure: {}. Raw response: {}",
            e,
            &result.text[..result.text.len().min(500)]
        );
        AppError::Internal("AI response could not be parsed. Check logs.".into())
    })?;

    Ok(mapping)
}

fn extract_json_from_text(text: &str) -> String {
    // Try to find {…} by first { and last }
    if let (Some(start), Some(end)) = (text.find('{'), text.rfind('}')) {
        if end > start {
            return text[start..=end].to_string();
        }
    }
    text.to_string()
}

// ─── Command 3: migration_execute ────────────────────────────────────────────

#[tauri::command]
pub async fn migration_execute(
    path: String,
    mapping: MappingConfig,
    currency_exponent: u32,
    actor_user_id: String,
    on_event: tauri::ipc::Channel<MigrationProgress>,
    state: State<'_, AppState>,
) -> AppResult<()> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let pool = state.db.clone();

    // Step 1: Read all source data into memory
    let source_data = load_source_data(&path).await?;

    // Determine active sheet mappings (not "skip")
    let active_mappings: Vec<&SheetMapping> = mapping
        .sheet_mappings
        .iter()
        .filter(|sm| sm.target_table != "skip")
        .collect();

    let _ = on_event.send(MigrationProgress::Started {
        total_sheets: active_mappings.len(),
    });

    // Step 2: Execute in FK-safe order
    let fk_order = [
        "categories",
        "products",
        "customers",
        "sales",
        "sale_items",
        "stock_levels",
    ];

    // Caches for FK lookups
    let mut category_cache: HashMap<String, String> = HashMap::new();
    let mut product_cache: HashMap<String, String> = HashMap::new();
    // F-HIGH-01: maps legacy receipt_number → new sale_id so sale_items can link
    let mut sale_cache: HashMap<String, String> = HashMap::new();

    let mut total_rows_imported: usize = 0;

    for target in &fk_order {
        // Find any sheet mapping for this target
        for sheet_mapping in &mapping.sheet_mappings {
            if &sheet_mapping.target_table.as_str() != target {
                continue;
            }

            let sheet_data = match source_data.get(&sheet_mapping.source_sheet) {
                Some(d) => d,
                None => continue,
            };

            let total_rows = sheet_data.len();

            let _ = on_event.send(MigrationProgress::SheetStart {
                sheet: sheet_mapping.source_sheet.clone(),
                target: target.to_string(),
                total_rows,
            });

            let (inserted, skipped) = execute_sheet_mapping(
                &pool,
                sheet_mapping,
                sheet_data,
                target,
                currency_exponent,
                &mut category_cache,
                &mut product_cache,
                &mut sale_cache,
                &on_event,
            )
            .await
            .unwrap_or_else(|e| {
                let _ = on_event.send(MigrationProgress::Error {
                    message: format!("Sheet '{}' error: {}", sheet_mapping.source_sheet, e),
                });
                (0, total_rows)
            });

            total_rows_imported += inserted;

            let _ = on_event.send(MigrationProgress::SheetDone {
                sheet: sheet_mapping.source_sheet.clone(),
                target: target.to_string(),
                inserted,
                skipped,
            });
        }
    }

    // ── Audit log: record the migration for tamper-evident chain ──────────────
    {
        let device_id: String = sqlx::query_scalar(
            "SELECT device_id FROM devices WHERE is_active=1 ORDER BY device_code LIMIT 1",
        )
        .fetch_optional(&pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();

        let branch_id: String = sqlx::query_scalar(
            "SELECT branch_id FROM branches WHERE is_active=1 ORDER BY created_at LIMIT 1",
        )
        .fetch_optional(&pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();

        if !device_id.is_empty() && !branch_id.is_empty() {
            let migration_id = Ulid::new().to_string();
            let after = serde_json::json!({
                "migration_id": migration_id,
                "source_path": path,
                "rows_imported": total_rows_imported,
            })
            .to_string();
            let _ = audit_hash::insert_audit_entry(
                &pool, "MIGRATION_EXECUTED", "migration", &migration_id,
                &actor_user_id, "user", &device_id, &branch_id,
                None, Some(&after), None,
            ).await;
        }
    }

    let _ = on_event.send(MigrationProgress::Done {
        message: "Migration complete.".into(),
    });

    Ok(())
}

// ─── Load source data ─────────────────────────────────────────────────────────

type SourceData = HashMap<String, Vec<HashMap<String, String>>>;

async fn load_source_data(path: &str) -> AppResult<SourceData> {
    let lower = path.to_lowercase();
    if lower.ends_with(".csv") {
        load_csv_data(path.to_string()).await
    } else if lower.ends_with(".xlsx")
        || lower.ends_with(".xls")
        || lower.ends_with(".xlsm")
    {
        load_excel_data(path.to_string()).await
    } else if lower.ends_with(".db")
        || lower.ends_with(".sqlite")
        || lower.ends_with(".sqlite3")
    {
        load_sqlite_data(path.to_string()).await
    } else {
        let path2 = path.to_string();
        match load_excel_data(path.to_string()).await {
            Ok(d) => Ok(d),
            Err(_) => load_sqlite_data(path2).await,
        }
    }
}

async fn load_csv_data(path: String) -> AppResult<SourceData> {
    tokio::task::spawn_blocking(move || {
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_path(&path)
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let headers: Vec<String> = rdr
            .headers()
            .map_err(|e| AppError::Internal(e.to_string()))?
            .iter()
            .map(|h| h.to_string())
            .collect();

        let sheet_name = std::path::Path::new(&path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Sheet1".into());

        let rows: Vec<HashMap<String, String>> = rdr
            .records()
            .filter_map(|r| r.ok())
            .map(|record| {
                headers
                    .iter()
                    .enumerate()
                    .map(|(i, h)| {
                        (
                            h.clone(),
                            record.get(i).unwrap_or("").to_string(),
                        )
                    })
                    .collect()
            })
            .collect();

        let mut result = HashMap::new();
        result.insert(sheet_name, rows);
        Ok(result)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

async fn load_excel_data(path: String) -> AppResult<SourceData> {
    tokio::task::spawn_blocking(move || {
        use calamine::Reader;
        let mut wb = calamine::open_workbook_auto(&path)
            .map_err(|e| AppError::Internal(format!("Could not open Excel: {}", e)))?;

        let sheet_names = wb.sheet_names().to_vec();
        let mut result: SourceData = HashMap::new();

        for sheet_name in &sheet_names {
            let range = match wb.worksheet_range(sheet_name) {
                Ok(r) => r,
                Err(_) => continue,
            };

            let mut rows_iter = range.rows();
            let headers: Vec<String> = rows_iter
                .next()
                .map(|r| r.iter().map(|c| c.to_string()).collect())
                .unwrap_or_default();

            if headers.iter().all(|h| h.trim().is_empty()) {
                continue;
            }

            let rows: Vec<HashMap<String, String>> = rows_iter
                .map(|row| {
                    headers
                        .iter()
                        .enumerate()
                        .map(|(i, h)| {
                            let val = row.get(i).map(|c| c.to_string()).unwrap_or_default();
                            (h.clone(), val)
                        })
                        .collect()
                })
                .collect();

            result.insert(sheet_name.clone(), rows);
        }

        Ok(result)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

async fn load_sqlite_data(path: String) -> AppResult<SourceData> {
    let url = format!("sqlite:{}?mode=ro", path);
    let pool = sqlx::SqlitePool::connect(&url)
        .await
        .map_err(|e| AppError::Internal(format!("Could not open SQLite: {}", e)))?;

    let table_names: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .fetch_all(&pool)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

    let mut result: SourceData = HashMap::new();

    for table in &table_names {
        // Finding 5: reject table names containing SQL-injection characters
        if !is_safe_sql_identifier(table) {
            tracing::warn!("load_sqlite_data: skipping table '{}' — unsafe identifier", table);
            continue;
        }

        let col_names: Vec<String> =
            sqlx::query_scalar(&format!("SELECT name FROM pragma_table_info('{}')", table))
                .fetch_all(&pool)
                .await
                .unwrap_or_default();

        let db_rows = sqlx::query(&format!("SELECT * FROM \"{}\"", table))
            .fetch_all(&pool)
            .await
            .unwrap_or_default();

        let rows: Vec<HashMap<String, String>> = db_rows
            .iter()
            .map(|row| {
                use sqlx::Row;
                col_names
                    .iter()
                    .enumerate()
                    .map(|(i, col)| {
                        let val = row
                            .try_get::<String, _>(i)
                            .or_else(|_| row.try_get::<i64, _>(i).map(|v| v.to_string()))
                            .or_else(|_| row.try_get::<f64, _>(i).map(|v| v.to_string()))
                            .unwrap_or_default();
                        (col.clone(), val)
                    })
                    .collect()
            })
            .collect();

        result.insert(table.clone(), rows);
    }

    pool.close().await;
    Ok(result)
}

// ─── Execute a single sheet mapping ──────────────────────────────────────────

async fn execute_sheet_mapping(
    pool: &SqlitePool,
    sheet_mapping: &SheetMapping,
    rows: &[HashMap<String, String>],
    target: &str,
    currency_exponent: u32,
    category_cache: &mut HashMap<String, String>,
    product_cache: &mut HashMap<String, String>,
    sale_cache: &mut HashMap<String, String>,
    on_event: &tauri::ipc::Channel<MigrationProgress>,
) -> AppResult<(usize, usize)> {
    let mut tx = pool.begin().await?;
    let mut inserted = 0usize;
    let mut skipped = 0usize;

    for (row_idx, row) in rows.iter().enumerate() {
        let ok = match target {
            "categories" => {
                insert_category(&mut tx, sheet_mapping, row, row_idx, category_cache, currency_exponent).await
            }
            "products" => {
                insert_product(pool, &mut tx, sheet_mapping, row, category_cache, product_cache, currency_exponent).await
            }
            "customers" => {
                insert_customer(&mut tx, sheet_mapping, row, currency_exponent).await
            }
            "stock_levels" => {
                insert_stock_level(&mut tx, sheet_mapping, row, product_cache, currency_exponent).await
            }
            // F-HIGH-01: Historical sales + line items now import via a synthetic
            // "Imported History" shift so FK constraints are satisfied.
            "sales" => {
                insert_sale(&mut tx, sheet_mapping, row, sale_cache, currency_exponent).await
            }
            "sale_items" => {
                insert_sale_item(&mut tx, sheet_mapping, row, sale_cache, product_cache, currency_exponent).await
            }
            _ => Ok(false),
        };

        match ok {
            Ok(true) => inserted += 1,
            Ok(false) => skipped += 1,
            Err(e) => {
                tracing::warn!("Row {} skipped due to error: {}", row_idx, e);
                skipped += 1;
            }
        }

        // Send progress every 50 rows
        if (row_idx + 1) % 50 == 0 {
            let _ = on_event.send(MigrationProgress::SheetProgress {
                sheet: sheet_mapping.source_sheet.clone(),
                done: row_idx + 1,
                total: rows.len(),
            });
        }
    }

    // Final progress event
    let _ = on_event.send(MigrationProgress::SheetProgress {
        sheet: sheet_mapping.source_sheet.clone(),
        done: rows.len(),
        total: rows.len(),
    });

    tx.commit().await?;
    Ok((inserted, skipped))
}

// ─── Transform helpers ────────────────────────────────────────────────────────

fn apply_transform(value: &str, transform: &str, currency_exponent: u32) -> Option<String> {
    match transform {
        "skip" => None,
        "identity" => Some(value.to_string()),
        "integer" => {
            let v: i64 = value.trim().parse().unwrap_or(0);
            Some(v.to_string())
        }
        "price_minor" => {
            let cleaned = value.trim().replace(',', ".");
            let minor = parse_price_string(&cleaned, currency_exponent)?;
            Some(minor.to_string())
        }
        "boolean" => {
            let lower = value.trim().to_lowercase();
            let v = if matches!(lower.as_str(), "yes" | "true" | "1" | "active" | "y") {
                1
            } else {
                0
            };
            Some(v.to_string())
        }
        "date" => Some(parse_date(value)),
        "phone" => {
            let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                return None;
            }
            let phone = if digits.len() == 8 {
                format!("+973{}", digits)
            } else if digits.starts_with("973") {
                format!("+{}", digits)
            } else {
                format!("+{}", digits)
            };
            Some(phone)
        }
        _ => Some(value.to_string()),
    }
}

fn parse_date(value: &str) -> String {
    let v = value.trim();
    let b = v.as_bytes();
    // Try YYYY-MM-DD
    if v.len() == 10 && b.get(4) == Some(&b'-') && b.get(7) == Some(&b'-') {
        return v.to_string();
    }
    // Try DD/MM/YYYY
    if v.len() == 10 && b.get(2) == Some(&b'/') && b.get(5) == Some(&b'/') {
        let parts: Vec<&str> = v.split('/').collect();
        if parts.len() == 3 {
            return format!("{}-{}-{}", parts[2], parts[1], parts[0]);
        }
    }
    // Try MM/DD/YYYY (same separator pattern — leave as-is, ambiguous with DD/MM/YYYY)
    // Try DD-MM-YYYY
    if v.len() == 10 && b.get(2) == Some(&b'-') && b.get(5) == Some(&b'-') {
        let parts: Vec<&str> = v.split('-').collect();
        if parts.len() == 3 && parts[2].len() == 4 {
            return format!("{}-{}-{}", parts[2], parts[1], parts[0]);
        }
    }
    // Return original on failure
    v.to_string()
}

/// Get the mapped value for a target column from a row
fn get_mapped_value(
    sheet_mapping: &SheetMapping,
    row: &HashMap<String, String>,
    target_col: &str,
    currency_exponent: u32,
) -> Option<String> {
    let cm = sheet_mapping
        .column_mappings
        .iter()
        .find(|m| m.target_col == target_col)?;
    let raw_value = row.get(&cm.source_col).map(|s| s.as_str()).unwrap_or("");
    apply_transform(raw_value, &cm.transform, currency_exponent)
}

// ─── Insert functions per target table ───────────────────────────────────────

async fn insert_category(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    sheet_mapping: &SheetMapping,
    row: &HashMap<String, String>,
    row_idx: usize,
    category_cache: &mut HashMap<String, String>,
    currency_exponent: u32,
) -> AppResult<bool> {
    let name = match get_mapped_value(sheet_mapping, row, "name", currency_exponent) {
        Some(n) if !n.trim().is_empty() => n,
        _ => return Ok(false),
    };

    let category_id = Ulid::new().to_string();
    let sort_order = get_mapped_value(sheet_mapping, row, "sort_order", currency_exponent)
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or((row_idx as i64) + 1);
    let is_active = get_mapped_value(sheet_mapping, row, "is_active", currency_exponent)
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(1);

    let result = sqlx::query(
        "INSERT OR IGNORE INTO categories (category_id, name, sort_order, is_active, created_at, updated_at) VALUES (?, ?, ?, ?, datetime('now'), datetime('now'))"
    )
    .bind(&category_id)
    .bind(&name)
    .bind(sort_order)
    .bind(is_active)
    .execute(&mut **tx)
    .await?;

    if result.rows_affected() > 0 {
        category_cache.insert(name.trim().to_lowercase(), category_id);
        Ok(true)
    } else {
        Ok(false)
    }
}

async fn insert_product(
    pool: &SqlitePool,
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    sheet_mapping: &SheetMapping,
    row: &HashMap<String, String>,
    category_cache: &HashMap<String, String>,
    product_cache: &mut HashMap<String, String>,
    currency_exponent: u32,
) -> AppResult<bool> {
    let name = match get_mapped_value(sheet_mapping, row, "name", currency_exponent) {
        Some(n) if !n.trim().is_empty() => n,
        _ => return Ok(false),
    };

    let product_id = Ulid::new().to_string();

    // Resolve category_id from cache
    let category_id: Option<String> = {
        // Check if any column maps to "category_name"
        let cat_name_mapping = sheet_mapping
            .column_mappings
            .iter()
            .find(|m| m.target_col == "category_name");

        if let Some(cm) = cat_name_mapping {
            let raw = row.get(&cm.source_col).map(|s| s.as_str()).unwrap_or("");
            let cat_key = raw.trim().to_lowercase();
            category_cache.get(&cat_key).cloned()
        } else {
            // Try direct category_id mapping
            get_mapped_value(sheet_mapping, row, "category_id", currency_exponent)
        }
    };

    // F-MED-03: Fallback — query the real first active category, never use hardcoded seed ID.
    // The seed category ID (01JCAT000000000000DRINK01 etc.) doesn't match 01JCAT000000000000000001
    // so the old hardcoded value would produce FK violations on any real install.
    let category_id = match category_id {
        Some(id) if !id.is_empty() => id,
        _ => {
            sqlx::query_scalar::<_, String>(
                "SELECT category_id FROM categories WHERE is_active=1 ORDER BY sort_order, name LIMIT 1",
            )
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .ok_or_else(|| crate::errors::AppError::NotFound(
                "No active category found — create at least one category before importing products".into()
            ))?
        }
    };

    let barcode = get_mapped_value(sheet_mapping, row, "barcode", currency_exponent);
    let sku = get_mapped_value(sheet_mapping, row, "sku", currency_exponent);
    let track_inventory = get_mapped_value(sheet_mapping, row, "track_inventory", currency_exponent)
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);
    let is_active = get_mapped_value(sheet_mapping, row, "is_active", currency_exponent)
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(1);

    let product_result = sqlx::query(
        "INSERT OR IGNORE INTO products \
         (product_id, category_id, name, barcode, sku, \
          track_inventory, allow_decimal_quantity, is_active, \
          tax_rule_id, reorder_point, cost_minor, currency, \
          created_at, updated_at, version) \
         VALUES (?, ?, ?, ?, ?, ?, 0, ?, NULL, 0, 0, 'BHD', datetime('now'), datetime('now'), 1)"
    )
    .bind(&product_id)
    .bind(&category_id)
    .bind(&name)
    .bind(barcode.as_deref())
    .bind(sku.as_deref())
    .bind(track_inventory)
    .bind(is_active)
    .execute(&mut **tx)
    .await?;

    if product_result.rows_affected() == 0 {
        return Ok(false);
    }

    // Insert price if present
    let price_minor = get_mapped_value(sheet_mapping, row, "price_minor", currency_exponent)
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);

    let price_id = Ulid::new().to_string();
    sqlx::query(
        "INSERT OR IGNORE INTO product_prices \
         (price_id, product_id, price_type, price_minor, currency, \
          effective_from, created_at) \
         VALUES (?, ?, 'selling', ?, 'BHD', datetime('now'), datetime('now'))"
    )
    .bind(&price_id)
    .bind(&product_id)
    .bind(price_minor)
    .execute(&mut **tx)
    .await?;

    product_cache.insert(name.trim().to_lowercase(), product_id);
    Ok(true)
}

async fn insert_customer(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    sheet_mapping: &SheetMapping,
    row: &HashMap<String, String>,
    currency_exponent: u32,
) -> AppResult<bool> {
    let name = match get_mapped_value(sheet_mapping, row, "name", currency_exponent) {
        Some(n) if !n.trim().is_empty() => n,
        _ => return Ok(false),
    };

    let customer_id = Ulid::new().to_string();
    let phone = get_mapped_value(sheet_mapping, row, "phone", currency_exponent);
    let email = get_mapped_value(sheet_mapping, row, "email", currency_exponent);
    let loyalty_points = get_mapped_value(sheet_mapping, row, "loyalty_points", currency_exponent)
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);
    let notes = get_mapped_value(sheet_mapping, row, "notes", currency_exponent);

    // Resolve active branch — never use seed fallback (customers would be invisible under real branch)
    let branch_id: String = sqlx::query_scalar(
        "SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| crate::errors::AppError::NotFound("No active branch configured".into()))?;

    let result = sqlx::query(
        "INSERT OR IGNORE INTO customers (customer_id, branch_id, name, phone, email, loyalty_points, notes, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))"
    )
    .bind(&customer_id)
    .bind(branch_id)
    .bind(&name)
    .bind(phone.as_deref())
    .bind(email.as_deref())
    .bind(loyalty_points)
    .bind(notes.as_deref())
    .execute(&mut **tx)
    .await?;

    Ok(result.rows_affected() > 0)
}

// ─── Extended tool result types ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ConnectTestResult {
    pub success: bool,
    pub message: String,
    pub server_version: Option<String>,
    pub db_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteTableInfo {
    pub name: String,
    pub row_count: i64,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub row_count: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessInfo {
    pub name: String,
    pub pid: String,
    pub memory_kb: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DbFileInfo {
    pub path: String,
    pub size_bytes: u64,
    pub file_type: String,
    pub modified: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DecompressResult {
    pub extracted_files: Vec<String>,
    pub db_files: Vec<String>,
    pub dest_dir: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ZanposStats {
    pub products: i64,
    pub categories: i64,
    pub customers: i64,
    pub sales: i64,
    pub sale_items: i64,
    pub stock_levels: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RollbackResult {
    pub deleted_counts: Vec<(String, i64)>,
    pub total_deleted: i64,
}

// ─── MSSQL helper ─────────────────────────────────────────────────────────────

type MssqlClient = tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>;

async fn mssql_connect(conn_str: &str) -> AppResult<MssqlClient> {
    use tokio_util::compat::TokioAsyncWriteCompatExt;
    let config = tiberius::Config::from_ado_string(conn_str)
        .map_err(|e| AppError::Internal(format!("Invalid MSSQL config: {}", e)))?;
    let addr = config.get_addr();
    let tcp = tokio::net::TcpStream::connect(addr)
        .await
        .map_err(|e| AppError::Internal(format!("MSSQL TCP connect: {}", e)))?;
    tcp.set_nodelay(true).ok();
    tiberius::Client::connect(config, tcp.compat_write())
        .await
        .map_err(|e| AppError::Internal(format!("MSSQL auth: {}", e)))
}

fn format_ts(secs: u64) -> String {
    use chrono::TimeZone;
    match chrono::Utc.timestamp_opt(secs as i64, 0) {
        chrono::LocalResult::Single(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
        _ => format!("ts:{}", secs),
    }
}

// ─── Command 4: migration_connect_test ────────────────────────────────────────

#[tauri::command]
pub async fn migration_connect_test(
    db_type: String,
    conn_str: String,
) -> AppResult<ConnectTestResult> {
    match db_type.as_str() {
        "sqlite" => {
            let url = format!("sqlite:{}?mode=ro", conn_str);
            match sqlx::SqlitePool::connect(&url).await {
                Ok(pool) => {
                    let ver: String = sqlx::query_scalar("SELECT sqlite_version()")
                        .fetch_one(&pool)
                        .await
                        .unwrap_or_else(|_| "unknown".into());
                    pool.close().await;
                    Ok(ConnectTestResult {
                        success: true,
                        message: "Connected successfully".into(),
                        server_version: Some(format!("SQLite {}", ver)),
                        db_type,
                    })
                }
                Err(e) => Ok(ConnectTestResult {
                    success: false,
                    message: e.to_string(),
                    server_version: None,
                    db_type,
                }),
            }
        }
        "mysql" => {
            let conn = conn_str.clone();
            let res = tokio::task::spawn_blocking(move || -> Result<String, String> {
                use mysql::prelude::Queryable;
                let pool = mysql::Pool::new(conn.as_str()).map_err(|e| e.to_string())?;
                let mut c = pool.get_conn().map_err(|e| e.to_string())?;
                let ver: Option<String> = c
                    .query_first("SELECT VERSION()")
                    .map_err(|e| e.to_string())?;
                Ok(ver.unwrap_or_else(|| "unknown".into()))
            })
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

            match res {
                Ok(ver) => Ok(ConnectTestResult {
                    success: true,
                    message: "Connected successfully".into(),
                    server_version: Some(format!("MySQL {}", ver)),
                    db_type,
                }),
                Err(e) => Ok(ConnectTestResult {
                    success: false,
                    message: e,
                    server_version: None,
                    db_type,
                }),
            }
        }
        "mssql" => {
            use tokio_util::compat::TokioAsyncWriteCompatExt;
            let config = match tiberius::Config::from_ado_string(&conn_str) {
                Ok(c) => c,
                Err(e) => {
                    return Ok(ConnectTestResult {
                        success: false,
                        message: format!("Invalid connection string: {}", e),
                        server_version: None,
                        db_type,
                    })
                }
            };
            let addr = config.get_addr();
            let tcp = match tokio::net::TcpStream::connect(addr).await {
                Ok(t) => t,
                Err(e) => {
                    return Ok(ConnectTestResult {
                        success: false,
                        message: format!("Cannot reach server: {}", e),
                        server_version: None,
                        db_type,
                    })
                }
            };
            tcp.set_nodelay(true).ok();
            match tiberius::Client::connect(config, tcp.compat_write()).await {
                Ok(_) => Ok(ConnectTestResult {
                    success: true,
                    message: "Connected successfully".into(),
                    server_version: Some("SQL Server".into()),
                    db_type,
                }),
                Err(e) => Ok(ConnectTestResult {
                    success: false,
                    message: e.to_string(),
                    server_version: None,
                    db_type,
                }),
            }
        }
        _ => Ok(ConnectTestResult {
            success: false,
            message: format!("Unknown db_type '{}'. Use: sqlite, mysql, mssql", db_type),
            server_version: None,
            db_type,
        }),
    }
}

// ─── Command 5: migration_list_tables ─────────────────────────────────────────

#[tauri::command]
pub async fn migration_list_tables(
    db_type: String,
    conn_str: String,
) -> AppResult<Vec<RemoteTableInfo>> {
    match db_type.as_str() {
        "sqlite" => {
            let url = format!("sqlite:{}?mode=ro", conn_str);
            let pool = sqlx::SqlitePool::connect(&url)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;
            let tables: Vec<String> = sqlx::query_scalar(
                "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name",
            )
            .fetch_all(&pool)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

            let mut result = Vec::new();
            for table in &tables {
                // Finding 5: reject table names containing SQL-injection characters
                if !is_safe_sql_identifier(table) {
                    result.push(RemoteTableInfo {
                        name: format!("[table name contains unsafe characters, skipped]"),
                        row_count: 0,
                        columns: vec![],
                    });
                    continue;
                }
                let count: i64 =
                    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM \"{}\"", table))
                        .fetch_one(&pool)
                        .await
                        .unwrap_or(0);
                let cols: Vec<String> = sqlx::query_scalar(&format!(
                    "SELECT name FROM pragma_table_info('{}')",
                    table
                ))
                .fetch_all(&pool)
                .await
                .unwrap_or_default();
                result.push(RemoteTableInfo { name: table.clone(), row_count: count, columns: cols });
            }
            pool.close().await;
            Ok(result)
        }
        "mysql" => {
            let conn = conn_str.clone();
            tokio::task::spawn_blocking(move || -> AppResult<Vec<RemoteTableInfo>> {
                use mysql::prelude::Queryable;
                let pool = mysql::Pool::new(conn.as_str())
                    .map_err(|e| AppError::Internal(e.to_string()))?;
                let mut c = pool.get_conn().map_err(|e| AppError::Internal(e.to_string()))?;

                let tables: Vec<String> = c
                    .query_map("SHOW TABLES", |row: mysql::Row| {
                        row.get::<String, usize>(0).unwrap_or_default()
                    })
                    .map_err(|e| AppError::Internal(e.to_string()))?;

                let mut result = Vec::new();
                for table in &tables {
                    let count: i64 = c
                        .query_first(format!("SELECT COUNT(*) FROM `{}`", table))
                        .unwrap_or_default()
                        .unwrap_or(0);
                    let cols: Vec<String> = c
                        .query_map(
                            format!("SHOW COLUMNS FROM `{}`", table),
                            |row: mysql::Row| row.get::<String, usize>(0).unwrap_or_default(),
                        )
                        .unwrap_or_default();
                    result.push(RemoteTableInfo { name: table.clone(), row_count: count, columns: cols });
                }
                Ok(result)
            })
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?
        }
        "mssql" => {
            use futures::StreamExt;
            let mut client = mssql_connect(&conn_str).await?;

            // Tables + row counts
            let sql = "SELECT t.name, SUM(p.rows) \
                       FROM sys.tables t \
                       JOIN sys.partitions p ON t.object_id = p.object_id AND p.index_id IN (0,1) \
                       GROUP BY t.name ORDER BY t.name";
            let mut stream = client
                .simple_query(sql)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;

            let mut tables: Vec<RemoteTableInfo> = Vec::new();
            while let Some(item) = stream.next().await {
                if let Ok(tiberius::QueryItem::Row(row)) = item {
                    let name: String = row.get::<&str, usize>(0).unwrap_or("").to_string();
                    let count: i64 = row.get::<i64, usize>(1).unwrap_or(0);
                    if !name.is_empty() {
                        tables.push(RemoteTableInfo { name, row_count: count, columns: Vec::new() });
                    }
                }
            }
            drop(stream);

            // Column names — fresh connection to avoid borrow issues
            let mut client2 = mssql_connect(&conn_str).await?;
            let col_sql = "SELECT t.name, c.name FROM sys.columns c \
                           JOIN sys.tables t ON c.object_id = t.object_id \
                           ORDER BY t.name, c.column_id";
            let mut col_stream = client2
                .simple_query(col_sql)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;

            while let Some(item) = col_stream.next().await {
                if let Ok(tiberius::QueryItem::Row(row)) = item {
                    let tname: String = row.get::<&str, usize>(0).unwrap_or("").to_string();
                    let cname: String = row.get::<&str, usize>(1).unwrap_or("").to_string();
                    if let Some(ti) = tables.iter_mut().find(|t| t.name == tname) {
                        ti.columns.push(cname);
                    }
                }
            }

            Ok(tables)
        }
        _ => Err(AppError::Internal(format!("Unsupported db_type: {}", db_type))),
    }
}

// ─── Command 6: migration_query_remote ───────────────────────────────────────

#[tauri::command]
pub async fn migration_query_remote(
    db_type: String,
    conn_str: String,
    query: String,
    max_rows: Option<usize>,
) -> AppResult<QueryResult> {
    // Finding 6: block CTE-wrapped destructive queries (e.g. WITH x AS (SELECT 1) DELETE …)
    let q_upper = query.trim().to_uppercase();
    let allowed_starts = ["SELECT", "SHOW", "DESCRIBE", "DESC", "EXPLAIN", "PRAGMA", "WITH"];
    let is_read_intent = allowed_starts.iter().any(|s| q_upper.starts_with(s));

    // Block destructive keywords anywhere in the query (catches WITH…DELETE CTEs)
    let destructive_keywords = [
        " DELETE ", " UPDATE ", " INSERT ", " MERGE ", " DROP ",
        " TRUNCATE ", " ALTER ", " CREATE ", " REPLACE ", " GRANT ", " REVOKE ",
        "\nDELETE ", "\nUPDATE ", "\nINSERT ", "\nDROP ",
    ];
    let has_destructive = destructive_keywords.iter().any(|kw| q_upper.contains(kw));

    if !is_read_intent || has_destructive {
        return Err(AppError::Validation(
            "Only SELECT/SHOW/DESCRIBE/EXPLAIN/PRAGMA queries are allowed.".into(),
        ));
    }

    let limit = max_rows.unwrap_or(20).min(50);

    // Inject LIMIT for SQLite/MySQL SELECT queries that don't already have one
    let limited_query = if q_upper.starts_with("SELECT") && !q_upper.contains(" LIMIT ") {
        match db_type.as_str() {
            "sqlite" | "mysql" => {
                format!("{} LIMIT {}", query.trim_end_matches(';'), limit)
            }
            _ => query.clone(),
        }
    } else {
        query.clone()
    };

    match db_type.as_str() {
        "sqlite" => {
            let url = format!("sqlite:{}?mode=ro", conn_str);
            let pool = sqlx::SqlitePool::connect(&url)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;

            let rows = sqlx::query(&limited_query)
                .fetch_all(&pool)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;
            pool.close().await;

            use sqlx::{Column as _, Row as _};
            let columns: Vec<String> = rows
                .first()
                .map(|r| r.columns().iter().map(|c| c.name().to_string()).collect())
                .unwrap_or_default();

            let result_rows: Vec<Vec<String>> = rows
                .iter()
                .map(|row| {
                    use sqlx::Row as _;
                    (0..row.len())
                        .map(|i| {
                            row.try_get::<String, _>(i)
                                .or_else(|_| row.try_get::<i64, _>(i).map(|v| v.to_string()))
                                .or_else(|_| row.try_get::<f64, _>(i).map(|v| v.to_string()))
                                .unwrap_or_else(|_| "NULL".to_string())
                        })
                        .collect()
                })
                .collect();

            let total = result_rows.len();
            Ok(QueryResult { columns, rows: result_rows, row_count: total, truncated: total >= limit })
        }
        "mysql" => {
            let conn = conn_str.clone();
            let q = limited_query.clone();
            tokio::task::spawn_blocking(move || -> AppResult<QueryResult> {
                use mysql::prelude::Queryable;
                let pool = mysql::Pool::new(conn.as_str())
                    .map_err(|e| AppError::Internal(e.to_string()))?;
                let mut c = pool.get_conn().map_err(|e| AppError::Internal(e.to_string()))?;

                let db_rows: Vec<mysql::Row> =
                    c.query(q).map_err(|e| AppError::Internal(e.to_string()))?;

                let columns: Vec<String> = db_rows
                    .first()
                    .map(|r| {
                        r.columns_ref()
                            .iter()
                            .map(|col| col.name_str().to_string())
                            .collect()
                    })
                    .unwrap_or_default();

                let result_rows: Vec<Vec<String>> = db_rows
                    .iter()
                    .map(|row| {
                        (0..row.len())
                            .map(|i| {
                                row.get::<String, usize>(i)
                                    .or_else(|| row.get::<i64, usize>(i).map(|v| v.to_string()))
                                    .or_else(|| row.get::<f64, usize>(i).map(|v| v.to_string()))
                                    .unwrap_or_else(|| "NULL".to_string())
                            })
                            .collect()
                    })
                    .collect();

                let total = result_rows.len();
                Ok(QueryResult {
                    columns,
                    rows: result_rows,
                    row_count: total,
                    truncated: total >= limit,
                })
            })
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?
        }
        "mssql" => {
            use futures::StreamExt;
            // Wrap SELECT with TOP if not already limited
            let mssql_query = if q_upper.starts_with("SELECT") && !q_upper.contains(" TOP ") {
                let rest = query
                    .trim()
                    .splitn(2, char::is_whitespace)
                    .nth(1)
                    .unwrap_or("");
                format!("SELECT TOP {} {}", limit, rest)
            } else {
                limited_query.clone()
            };

            let mut client = mssql_connect(&conn_str).await?;
            let mut stream = client
                .simple_query(mssql_query.as_str())
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;

            let mut columns: Vec<String> = Vec::new();
            let mut result_rows: Vec<Vec<String>> = Vec::new();

            while let Some(item) = stream.next().await {
                if let Ok(tiberius::QueryItem::Row(row)) = item {
                    if columns.is_empty() {
                        columns = row
                            .columns()
                            .iter()
                            .map(|c| c.name().to_string())
                            .collect();
                    }
                    let vals: Vec<String> = (0..row.len())
                        .map(|i| {
                            row.get::<&str, usize>(i)
                                .map(|s| s.to_string())
                                .or_else(|| row.get::<i64, usize>(i).map(|v| v.to_string()))
                                .or_else(|| row.get::<f64, usize>(i).map(|v| v.to_string()))
                                .or_else(|| row.get::<bool, usize>(i).map(|v| v.to_string()))
                                .unwrap_or_else(|| "NULL".to_string())
                        })
                        .collect();
                    result_rows.push(vals);
                }
            }

            let total = result_rows.len();
            Ok(QueryResult {
                columns,
                rows: result_rows,
                row_count: total,
                truncated: total >= limit,
            })
        }
        _ => Err(AppError::Internal(format!("Unsupported db_type: {}", db_type))),
    }
}

// ─── Command 7: migration_list_processes ──────────────────────────────────────

#[tauri::command]
pub async fn migration_list_processes(
    filter: Option<String>,
) -> AppResult<Vec<ProcessInfo>> {
    tokio::task::spawn_blocking(move || -> AppResult<Vec<ProcessInfo>> {
        let output = std::process::Command::new("tasklist")
            .args(["/FO", "CSV", "/NH"])
            .output()
            .map_err(|e| AppError::Internal(format!("tasklist failed: {}", e)))?;

        let text = String::from_utf8_lossy(&output.stdout);
        let filter_lower = filter.as_deref().map(|f| f.to_lowercase());

        let mut processes: Vec<ProcessInfo> = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            // CSV format: "Image Name","PID","Session Name","Session#","Mem Usage"
            let parts: Vec<&str> = line.split("\",\"").collect();
            if parts.len() < 5 {
                continue;
            }
            let name = parts[0].trim_matches('"').to_string();
            let pid = parts[1].trim_matches('"').to_string();
            let mem = parts[4].trim_matches('"').to_string();

            if let Some(ref f) = filter_lower {
                if !name.to_lowercase().contains(f.as_str()) {
                    continue;
                }
            }
            processes.push(ProcessInfo { name, pid, memory_kb: mem });
        }
        Ok(processes)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

// ─── Command 8: migration_find_db_files ───────────────────────────────────────

#[tauri::command]
pub async fn migration_find_db_files(
    extra_paths: Option<Vec<String>>,
) -> AppResult<Vec<DbFileInfo>> {
    tokio::task::spawn_blocking(move || -> AppResult<Vec<DbFileInfo>> {
        let db_exts = ["db", "sqlite", "sqlite3", "db3", "s3db", "mdf", "ndf", "mdb", "accdb", "fdb", "gdb", "bak", "sql", "json"];

        let mut roots: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(appdata) = std::env::var("APPDATA") {
            roots.push(std::path::PathBuf::from(appdata));
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            roots.push(std::path::PathBuf::from(local));
        }
        if let Ok(profile) = std::env::var("USERPROFILE") {
            let p = std::path::PathBuf::from(&profile);
            roots.push(p.join("Desktop"));
            roots.push(p.join("Documents"));
        }
        roots.push(std::path::PathBuf::from("C:\\Program Files"));
        roots.push(std::path::PathBuf::from("C:\\Program Files (x86)"));

        if let Some(extra) = extra_paths {
            for p in extra {
                roots.push(std::path::PathBuf::from(p));
            }
        }

        let mut results: Vec<DbFileInfo> = Vec::new();
        for root in &roots {
            walk_for_db_files(root, &db_exts, 0, 4, &mut results);
        }
        // Sort by size descending
        results.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
        results.truncate(200);
        Ok(results)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

fn walk_for_db_files(
    dir: &std::path::Path,
    exts: &[&str],
    depth: usize,
    max_depth: usize,
    results: &mut Vec<DbFileInfo>,
) {
    if depth > max_depth {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    let skip_dirs = [
        "windows", "system32", "syswow64", "winsxs", "$recycle.bin",
        "node_modules", ".git", "temp", "tmp",
    ];
    for entry in entries.flatten() {
        let path = entry.path();
        let name_lower = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        if path.is_dir() {
            if skip_dirs.iter().any(|s| name_lower == *s) {
                continue;
            }
            walk_for_db_files(&path, exts, depth + 1, max_depth, results);
        } else if path.is_file() {
            let ext = path
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            if exts.contains(&ext.as_str()) {
                let meta = std::fs::metadata(&path).ok();
                let size_bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                let modified = meta
                    .as_ref()
                    .and_then(|m| m.modified().ok())
                    .map(|t| {
                        let secs = t
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        format_ts(secs)
                    })
                    .unwrap_or_else(|| "unknown".into());
                results.push(DbFileInfo {
                    path: path.to_string_lossy().to_string(),
                    size_bytes,
                    file_type: ext.to_string(),
                    modified,
                });
            }
        }
    }
}

// ─── Command 9: migration_read_file ──────────────────────────────────────────

#[tauri::command]
pub async fn migration_read_file(
    path: String,
    max_chars: Option<usize>,
) -> AppResult<String> {
    // Finding 4: reject paths outside approved directories or in sensitive locations
    if !is_safe_read_path(&path) {
        return Err(AppError::Validation(
            "Access denied: path is outside approved directories or is a sensitive system path.".into(),
        ));
    }
    let limit = max_chars.unwrap_or(8000).min(32_000);
    tokio::task::spawn_blocking(move || -> AppResult<String> {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| {
                tracing::error!("Cannot read '{}': {}", path, e);
                AppError::Internal("Cannot read the requested file. Check logs.".into())
            })?;
        if content.len() <= limit {
            Ok(content)
        } else {
            let truncated: String = content.chars().take(limit).collect();
            Ok(format!("{}\n\n[... truncated at {} chars]", truncated, limit))
        }
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

// ─── Command 10: migration_decompress ────────────────────────────────────────

#[tauri::command]
pub async fn migration_decompress(
    archive_path: String,
    dest_dir: Option<String>,
) -> AppResult<DecompressResult> {
    tokio::task::spawn_blocking(move || -> AppResult<DecompressResult> {
        let archive = std::path::PathBuf::from(&archive_path);
        let dest = match dest_dir {
            Some(d) => std::path::PathBuf::from(d),
            None => {
                // Default: same dir as archive, sub-folder named after archive stem
                let stem = archive
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                archive
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("."))
                    .join(stem)
            }
        };

        std::fs::create_dir_all(&dest)
            .map_err(|e| AppError::Internal(format!("Cannot create dest dir: {}", e)))?;

        // Finding 8: canonicalize dest so zip-slip checks are reliable
        let dest_canonical = std::fs::canonicalize(&dest).unwrap_or_else(|_| dest.clone());

        let file = std::fs::File::open(&archive)
            .map_err(|e| AppError::Internal(format!("Cannot open archive: {}", e)))?;

        let mut zip = zip::ZipArchive::new(file)
            .map_err(|e| AppError::Internal(format!("Not a valid ZIP: {}", e)))?;

        let db_exts = ["db", "sqlite", "sqlite3", "mdf", "mdb", "fdb", "accdb"];
        let mut extracted: Vec<String> = Vec::new();
        let mut db_files: Vec<String> = Vec::new();
        let mut err_msg: Option<String> = None;

        for i in 0..zip.len() {
            let mut entry = match zip.by_index(i) {
                Ok(e) => e,
                Err(e) => {
                    err_msg = Some(e.to_string());
                    continue;
                }
            };

            // enclosed_name() sanitizes ".." components; we also verify the resolved
            // path stays within dest_canonical (belt-and-suspenders zip-slip protection).
            let entry_path = match entry.enclosed_name() {
                Some(p) => dest_canonical.join(p),
                None => continue,
            };

            // Finding 8: reject any entry that resolves outside the destination directory
            // (zip-slip protection — enclosed_name already strips "..", this is belt-and-suspenders)
            {
                let resolved = std::fs::canonicalize(&entry_path)
                    .unwrap_or_else(|_| entry_path.clone());
                if !resolved.starts_with(&dest_canonical) && !entry_path.starts_with(&dest_canonical) {
                    continue; // skip zip-slip attempt
                }
            }

            if entry.is_dir() {
                std::fs::create_dir_all(&entry_path).ok();
                continue;
            }

            if let Some(parent) = entry_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }

            match std::fs::File::create(&entry_path) {
                Ok(mut out) => {
                    if std::io::copy(&mut entry, &mut out).is_ok() {
                        let path_str = entry_path.to_string_lossy().to_string();
                        extracted.push(path_str.clone());
                        let ext = entry_path
                            .extension()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_lowercase();
                        if db_exts.contains(&ext.as_str()) {
                            db_files.push(path_str);
                        }
                    }
                }
                Err(e) => {
                    err_msg = Some(format!("Write error {}: {}", entry_path.display(), e));
                }
            }
        }

        Ok(DecompressResult {
            extracted_files: extracted,
            db_files,
            dest_dir: dest.to_string_lossy().to_string(),
            error: err_msg,
        })
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

// ─── Command 11: migration_zanpos_stats ──────────────────────────────────────

#[tauri::command]
pub async fn migration_zanpos_stats(
    state: State<'_, AppState>,
) -> AppResult<ZanposStats> {
    let pool = &state.db;
    let products: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products")
        .fetch_one(pool).await.unwrap_or(0);
    let categories: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM categories")
        .fetch_one(pool).await.unwrap_or(0);
    let customers: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM customers")
        .fetch_one(pool).await.unwrap_or(0);
    let sales: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sales")
        .fetch_one(pool).await.unwrap_or(0);
    let sale_items: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sale_items")
        .fetch_one(pool).await.unwrap_or(0);
    let stock_levels: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM stock_levels")
        .fetch_one(pool).await.unwrap_or(0);

    Ok(ZanposStats { products, categories, customers, sales, sale_items, stock_levels })
}

// ─── Command 12: migration_rollback ──────────────────────────────────────────
/// Delete records created at or after `since_iso` (ISO 8601 UTC string, e.g.
/// "2025-01-15T10:30:00") from the migration-relevant ZANPOS tables.
/// Only deletes in FK-safe reverse order.

#[tauri::command]
pub async fn migration_rollback(
    since_iso: String,
    user_id: String,
    state: State<'_, AppState>,
) -> AppResult<RollbackResult> {
    crate::commands::rbac::manager_or_owner(&state.db, &user_id).await?;
    let pool = &state.db;
    let mut tx = pool.begin().await?;

    // Reverse FK order: items → sales → stock → product_prices → products → customers → categories
    let tables_and_ts_col: &[(&str, &str)] = &[
        ("sale_items",      "created_at"),
        ("sales",           "created_at"),
        ("stock_levels",    "updated_at"),
        ("product_prices",  "created_at"),
        ("products",        "created_at"),
        ("customers",       "created_at"),
        ("categories",      "created_at"),
    ];

    let mut deleted_counts: Vec<(String, i64)> = Vec::new();
    let mut total_deleted: i64 = 0;

    for (table, ts_col) in tables_and_ts_col {
        let sql = format!("DELETE FROM {} WHERE {} >= ?", table, ts_col);
        let result = sqlx::query(&sql)
            .bind(&since_iso)
            .execute(&mut *tx)
            .await;
        match result {
            Ok(r) => {
                let n = r.rows_affected() as i64;
                if n > 0 {
                    deleted_counts.push((table.to_string(), n));
                    total_deleted += n;
                }
            }
            Err(e) => {
                tracing::warn!("Rollback delete from {} failed: {}", table, e);
            }
        }
    }

    tx.commit().await?;

    // Audit log — best-effort, don't fail the rollback if write fails
    let audit_id = Ulid::new().to_string();
    let _ = sqlx::query(
        "INSERT INTO audit_logs \
           (audit_log_id, event_type, entity_type, entity_id, \
            actor_user_id, actor_type, created_at, hash, previous_hash) \
         VALUES (?, 'migration.rollback', 'migration', 'rollback', ?, 'user', datetime('now'), '', NULL)",
    )
    .bind(&audit_id)
    .bind(&user_id)
    .execute(pool)
    .await;

    Ok(RollbackResult { deleted_counts, total_deleted })
}

// ─── Original insert_stock_level (unchanged below) ────────────────────────────

async fn insert_stock_level(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    sheet_mapping: &SheetMapping,
    row: &HashMap<String, String>,
    product_cache: &HashMap<String, String>,
    currency_exponent: u32,
) -> AppResult<bool> {
    // Resolve product_id
    let product_id: Option<String> = {
        let product_name_mapping = sheet_mapping
            .column_mappings
            .iter()
            .find(|m| m.target_col == "product_name");

        if let Some(cm) = product_name_mapping {
            let raw = row.get(&cm.source_col).map(|s| s.as_str()).unwrap_or("");
            product_cache.get(&raw.trim().to_lowercase()).cloned()
        } else {
            get_mapped_value(sheet_mapping, row, "product_id", currency_exponent)
        }
    };

    let product_id = match product_id {
        Some(id) if !id.is_empty() => id,
        _ => return Ok(false), // Can't insert without a product
    };

    let stock_level_id = Ulid::new().to_string();
    // Use active branch — a hardcoded seed ID would assign stock to a phantom branch
    let branch_id = match get_mapped_value(sheet_mapping, row, "branch_id", currency_exponent)
        .filter(|s| !s.is_empty())
    {
        Some(id) => id,
        None => sqlx::query_scalar::<_, String>(
            "SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
        )
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| crate::errors::AppError::NotFound("No active branch configured".into()))?,
    };
    let quantity_on_hand = get_mapped_value(sheet_mapping, row, "quantity_on_hand", currency_exponent)
        .unwrap_or_else(|| "0".to_string());

    let result = sqlx::query(
        "INSERT OR IGNORE INTO stock_levels (stock_level_id, product_id, branch_id, quantity_on_hand, updated_at) VALUES (?, ?, ?, ?, datetime('now'))"
    )
    .bind(&stock_level_id)
    .bind(&product_id)
    .bind(&branch_id)
    .bind(&quantity_on_hand)
    .execute(&mut **tx)
    .await?;

    Ok(result.rows_affected() > 0)
}

// ─── Sales import (F-HIGH-01) ─────────────────────────────────────────────────

/// Deterministic synthetic shift used to anchor all imported historical sales.
/// Created once (INSERT OR IGNORE), it satisfies the sales→shift FK without
/// inventing a fake "open" shift that would pollute live shift reporting.
const IMPORT_SHIFT_ID: &str = "SHIFT-IMPORTED-HISTORY-000001";

/// Resolve active branch, device, and an owner user — required FKs for an
/// imported sale. Lazily creates the synthetic "Imported History" shift.
/// Returns (branch_id, device_id, cashier_user_id, shift_id).
async fn ensure_import_anchors(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
) -> AppResult<(String, String, String, String)> {
    let branch_id: String =
        sqlx::query_scalar("SELECT branch_id FROM branches WHERE is_active=1 ORDER BY created_at LIMIT 1")
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| crate::errors::AppError::NotFound("No active branch".into()))?;
    let device_id: String =
        sqlx::query_scalar("SELECT device_id FROM devices WHERE is_active=1 ORDER BY device_code LIMIT 1")
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| crate::errors::AppError::NotFound("No active device".into()))?;
    // Prefer an owner; fall back to any active user.
    let cashier_user_id: String = sqlx::query_scalar(
        "SELECT u.user_id FROM users u JOIN roles r ON r.role_id = u.role_id
         WHERE u.is_active=1 ORDER BY (r.name='owner') DESC, u.created_at LIMIT 1",
    )
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| crate::errors::AppError::NotFound("No active user".into()))?;

    // Lazily create the synthetic closed shift (idempotent).
    sqlx::query(
        "INSERT OR IGNORE INTO shifts
           (shift_id, branch_id, device_id, origin_device_id, cashier_user_id, opened_at, closed_at,
            opening_cash_minor, status, close_notes, sync_status)
         VALUES (?,?,?,?,?, datetime('now'), datetime('now'), 0, 'closed',
                 'Synthetic shift anchoring imported historical sales', 'synced')",
    )
    .bind(IMPORT_SHIFT_ID)
    .bind(&branch_id)
    .bind(&device_id)
    .bind(&device_id)
    .bind(&cashier_user_id)
    .execute(&mut **tx)
    .await?;

    Ok((branch_id, device_id, cashier_user_id, IMPORT_SHIFT_ID.to_string()))
}

/// Import one historical sale. The legacy receipt number is the join key that
/// sale_items rows reference; it is cached so line items can resolve their parent.
async fn insert_sale(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    sheet_mapping: &SheetMapping,
    row: &HashMap<String, String>,
    sale_cache: &mut HashMap<String, String>,
    currency_exponent: u32,
) -> AppResult<bool> {
    // Legacy receipt number — required as the join key for line items.
    let legacy_receipt = match get_mapped_value(sheet_mapping, row, "receipt_number", currency_exponent)
    {
        Some(r) if !r.trim().is_empty() => r.trim().to_string(),
        _ => return Ok(false), // no receipt → cannot anchor line items, skip
    };

    let (branch_id, device_id, cashier_user_id, shift_id) = ensure_import_anchors(tx).await?;

    let sale_id = Ulid::new().to_string();
    let net = get_mapped_value(sheet_mapping, row, "net_total_minor", currency_exponent)
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0)
        .max(0);
    let gross = get_mapped_value(sheet_mapping, row, "gross_total_minor", currency_exponent)
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(net);
    let tax = get_mapped_value(sheet_mapping, row, "tax_total_minor", currency_exponent)
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);
    let discount = get_mapped_value(sheet_mapping, row, "discount_total_minor", currency_exponent)
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);
    // Business date / sold_at — accept either, default to now.
    let sold_at = get_mapped_value(sheet_mapping, row, "sold_at", currency_exponent)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    let business_date = get_mapped_value(sheet_mapping, row, "business_date", currency_exponent)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            // Derive YYYY-MM-DD from sold_at when possible
            sold_at.get(0..10).unwrap_or("1970-01-01").to_string()
        });

    // Receipt number must be unique — prefix imported receipts to avoid clashing
    // with live receipts, while staying traceable to the legacy value.
    let receipt_number = format!("IMP-{}", legacy_receipt);
    let idempotency_key = format!("import-sale-{}-{}", legacy_receipt, sale_id);

    let res = sqlx::query(
        "INSERT OR IGNORE INTO sales
           (sale_id, receipt_number, branch_id, device_id, origin_device_id, shift_id, cashier_user_id,
            status, gross_total_minor, discount_total_minor, tax_total_minor,
            net_total_minor, currency, business_date, sold_at, created_offline,
            idempotency_key, sync_status)
         VALUES (?,?,?,?,?,?,?, 'completed', ?,?,?,?, 'BHD', ?,?, 0, ?, 'pending')",
    )
    .bind(&sale_id)
    .bind(&receipt_number)
    .bind(&branch_id)
    .bind(&device_id)
    .bind(&device_id)
    .bind(&shift_id)
    .bind(&cashier_user_id)
    .bind(gross)
    .bind(discount)
    .bind(tax)
    .bind(net)
    .bind(&business_date)
    .bind(&sold_at)
    .bind(&idempotency_key)
    .execute(&mut **tx)
    .await?;

    if res.rows_affected() > 0 {
        sale_cache.insert(legacy_receipt.to_lowercase(), sale_id);
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Import one historical sale line item, linking it to its parent sale via the
/// cached legacy receipt number. Product FK is resolved from the product cache
/// when available; otherwise the line is stored snapshot-only (product_id NULL).
async fn insert_sale_item(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    sheet_mapping: &SheetMapping,
    row: &HashMap<String, String>,
    sale_cache: &HashMap<String, String>,
    product_cache: &HashMap<String, String>,
    currency_exponent: u32,
) -> AppResult<bool> {
    // Resolve parent sale via legacy receipt number.
    let legacy_receipt = match get_mapped_value(sheet_mapping, row, "receipt_number", currency_exponent)
    {
        Some(r) if !r.trim().is_empty() => r.trim().to_lowercase(),
        _ => return Ok(false),
    };
    let sale_id = match sale_cache.get(&legacy_receipt) {
        Some(id) => id.clone(),
        None => return Ok(false), // parent sale not imported → skip orphan line
    };

    // Resolve device_id for origin_device_id (anchors already created by insert_sale).
    let (_branch_id, device_id, _cashier_user_id, _shift_id) = ensure_import_anchors(tx).await?;

    let product_name = get_mapped_value(sheet_mapping, row, "product_name", currency_exponent)
        .or_else(|| get_mapped_value(sheet_mapping, row, "name", currency_exponent))
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Imported Item".to_string());

    // Optional product FK from cache (by name).
    let product_id: Option<String> =
        product_cache.get(&product_name.trim().to_lowercase()).cloned();

    let quantity = get_mapped_value(sheet_mapping, row, "quantity", currency_exponent)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "1".to_string());
    let unit_price_minor = get_mapped_value(sheet_mapping, row, "unit_price_minor", currency_exponent)
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);
    let line_total_minor = get_mapped_value(sheet_mapping, row, "line_total_minor", currency_exponent)
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(unit_price_minor);
    let tax_amount_minor = get_mapped_value(sheet_mapping, row, "tax_amount_minor", currency_exponent)
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);

    let sale_item_id = Ulid::new().to_string();
    let res = sqlx::query(
        "INSERT INTO sale_items
           (sale_item_id, sale_id, origin_device_id, product_id, product_name_snapshot,
            quantity, unit_price_minor, line_discount_minor, tax_rule_snapshot,
            tax_amount_minor, line_total_minor, voided)
         VALUES (?,?,?,?,?,?,?,0,'{}',?,?,0)",
    )
    .bind(&sale_item_id)
    .bind(&sale_id)
    .bind(&device_id)
    .bind(product_id)
    .bind(&product_name)
    .bind(&quantity)
    .bind(unit_price_minor)
    .bind(tax_amount_minor)
    .bind(line_total_minor)
    .execute(&mut **tx)
    .await?;

    Ok(res.rows_affected() > 0)
}

// ═══════════════════════════════════════════════════════════════════════════════
// MIGRATION AGENT CHAT  —  Real AI chat with tool-use loop
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct MigrationAgentChatInput {
    pub history: Vec<ChatMessage>,
    pub message: String,
}

// ── Tool definitions ───────────────────────────────────────────────────────────

fn migration_agent_tool_definitions() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "mg_inspect_file".into(),
            description: "Read a local file (SQLite .db, CSV, Excel .xlsx/.xls) and return its schema: table/sheet names, column names, data types, row counts, and up to 3 sample values per column. Use this to understand what data is in a file before planning a migration.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute path to the file" }
                },
                "required": ["path"]
            }),
        },
        ToolDef {
            name: "mg_connect_db".into(),
            description: "Test connectivity to an external database (SQLite, MySQL, or MSSQL) and list all its tables with row counts and column names. Use this to verify a connection string works and see what tables the source system has.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "db_type": { "type": "string", "enum": ["sqlite", "mysql", "mssql"], "description": "Database type" },
                    "conn_str": { "type": "string", "description": "File path for sqlite, connection string for mysql/mssql. MySQL: mysql://user:pass@host:3306/dbname. MSSQL: Server=host;Database=db;User Id=user;Password=pass" }
                },
                "required": ["db_type", "conn_str"]
            }),
        },
        ToolDef {
            name: "mg_query_db".into(),
            description: "Run a read-only SQL query (SELECT/SHOW/DESCRIBE/PRAGMA) against an external database. Use to sample data, count rows, check column values, or understand the data before mapping it to ZANPOS. Max 50 rows returned.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "db_type": { "type": "string", "enum": ["sqlite", "mysql", "mssql"] },
                    "conn_str": { "type": "string" },
                    "sql": { "type": "string", "description": "SQL query to execute (SELECT only)" },
                    "max_rows": { "type": "integer", "description": "Max rows to return (default 20, max 50)" }
                },
                "required": ["db_type", "conn_str", "sql"]
            }),
        },
        ToolDef {
            name: "mg_find_db_files".into(),
            description: "Scan common Windows locations (AppData, Program Files, Desktop, Documents) for database files (.db, .sqlite, .mdf, .mdb, .fdb). Use when the user doesn't know where their old POS database is stored.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "extra_paths": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Additional directory paths to search"
                    }
                },
                "required": []
            }),
        },
        ToolDef {
            name: "mg_list_processes".into(),
            description: "List all running Windows processes. Use to detect if the old POS software is currently running (which might indicate where its data directory is).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "filter": { "type": "string", "description": "Optional substring filter on process name (case-insensitive)" }
                },
                "required": []
            }),
        },
        ToolDef {
            name: "mg_read_file".into(),
            description: "Read a text-based file (config file, INI, XML, JSON, connection string file, registry export, log) and return its contents. Useful for finding database connection strings hidden in configuration files.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute path to the file" },
                    "max_chars": { "type": "integer", "description": "Max characters to return (default 4000)" }
                },
                "required": ["path"]
            }),
        },
        ToolDef {
            name: "mg_shell".into(),
            description: "Run a safe read-only shell command on this Windows machine. Useful for exploring the filesystem, querying registry, checking file contents, or running sqlite3 commands. Only safe read-only commands are permitted.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Shell command to run. Permitted: dir, type, where, tasklist, reg query, wmic process, sqlite3, powershell Get-*, powershell Select-String. Example: 'dir C:\\Program Files /s /b | findstr .db'" }
                },
                "required": ["command"]
            }),
        },
        ToolDef {
            name: "mg_extract_zip".into(),
            description: "Extract a ZIP archive and list the files inside, highlighting any database files found. Use when the user has a backup archive of their old POS system.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "archive_path": { "type": "string", "description": "Absolute path to the .zip file" },
                    "dest_dir": { "type": "string", "description": "Optional destination directory (default: same folder as zip, with _extracted suffix)" }
                },
                "required": ["archive_path"]
            }),
        },
        ToolDef {
            name: "mg_zanpos_stats".into(),
            description: "Count the current records in the ZANPOS database (products, categories, customers, sales, sale items, stock levels). Use before and after migration to verify data was imported correctly.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        ToolDef {
            name: "mg_attach_mdf".into(),
            description: "Attach a SQL Server primary database file (.mdf) to a local SQL Server Express instance using sqlcmd. Once attached, you can connect to it using mg_connect_db with db_type=mssql. Returns the connection string to use.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "mdf_path": { "type": "string", "description": "Absolute path to the .mdf file" },
                    "db_name": { "type": "string", "description": "Name to use for the attached database (default: derived from filename)" }
                },
                "required": ["mdf_path"]
            }),
        },
        ToolDef {
            name: "mg_access_query".into(),
            description: "Query a Microsoft Access database (.accdb or .mdb) using PowerShell OleDb. Use this to read data from Access-based POS systems. Lists tables if sql is omitted. Requires Microsoft Access Database Engine (ACE) driver, which ships with MS Office or can be downloaded free.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "db_path": { "type": "string", "description": "Absolute path to the .accdb or .mdb file" },
                    "sql": { "type": "string", "description": "SQL SELECT query. If omitted, lists all tables." }
                },
                "required": ["db_path"]
            }),
        },
        ToolDef {
            name: "mg_read_sql_dump".into(),
            description: "Read and analyse a SQL dump file (.sql). Returns the detected tables, column names, and row counts. Also shows the first few INSERT statements so you can understand the data format. Works with MySQL dumps, SQLite .dump output, and generic SQL exports.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute path to the .sql file" }
                },
                "required": ["path"]
            }),
        },
    ]
}

// ── Tool executor ──────────────────────────────────────────────────────────────

async fn execute_migration_agent_tool(
    name: &str,
    input: &Value,
    pool: &SqlitePool,
) -> AppResult<String> {
    match name {
        "mg_inspect_file" => {
            let path = input.get("path").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing path".into()))?;
            let schema = migration_inspect_file(path.to_string()).await?;
            let mut out = format!("File: {}\nType: {}\nSheets/Tables: {}\n\n",
                schema.file_path, schema.file_type, schema.sheets.len());
            for sheet in &schema.sheets {
                out += &format!("── {} ({} rows, {} columns)\n",
                    sheet.name, sheet.row_count, sheet.columns.len());
                for col in &sheet.columns {
                    let samples = if col.samples.is_empty() {
                        "(empty)".to_string()
                    } else {
                        col.samples.iter().take(3).map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(", ")
                    };
                    out += &format!("   {} → samples: {}\n", col.name, samples);
                }
                out += "\n";
            }
            Ok(out)
        }

        "mg_connect_db" => {
            let db_type = input.get("db_type").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing db_type".into()))?;
            let conn_str = input.get("conn_str").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing conn_str".into()))?;

            let test = migration_connect_test(db_type.to_string(), conn_str.to_string()).await?;
            if !test.success {
                return Ok(format!("Connection FAILED: {}", test.message));
            }
            let tables = migration_list_tables(db_type.to_string(), conn_str.to_string()).await?;
            let mut out = format!("Connected! {}\n\n{} tables:\n",
                test.server_version.as_deref().unwrap_or(""), tables.len());
            for t in &tables {
                out += &format!("  {} — {} rows | columns: {}\n",
                    t.name, t.row_count,
                    t.columns.iter().take(8).cloned().collect::<Vec<_>>().join(", "));
            }
            Ok(out)
        }

        "mg_query_db" => {
            let db_type = input.get("db_type").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing db_type".into()))?;
            let conn_str = input.get("conn_str").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing conn_str".into()))?;
            let sql = input.get("sql").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing sql".into()))?;
            let max_rows = input.get("max_rows").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

            let result = migration_query_remote(
                db_type.to_string(), conn_str.to_string(), sql.to_string(), Some(max_rows)
            ).await?;

            let mut out = format!("Query: {}\n{} rows{}\nColumns: {}\n\n",
                sql, result.row_count,
                if result.truncated { " (truncated)" } else { "" },
                result.columns.join(", "));
            for row in &result.rows {
                out += &row.iter().zip(result.columns.iter())
                    .map(|(v, c)| format!("{}: {}", c, v))
                    .collect::<Vec<_>>()
                    .join(" | ");
                out += "\n";
            }
            Ok(out)
        }

        "mg_find_db_files" => {
            let extra: Vec<String> = input.get("extra_paths")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            let files = migration_find_db_files(if extra.is_empty() { None } else { Some(extra) }).await?;
            if files.is_empty() {
                return Ok("No database files found in common locations.".to_string());
            }
            let mut out = format!("Found {} database files:\n", files.len());
            for f in files.iter().take(40) {
                out += &format!("  {} ({}, {} KB, modified {})\n",
                    f.path, f.file_type, f.size_bytes / 1024, f.modified);
            }
            if files.len() > 40 { out += &format!("  ...and {} more\n", files.len() - 40); }
            Ok(out)
        }

        "mg_list_processes" => {
            let filter = input.get("filter").and_then(|v| v.as_str()).map(|s| s.to_string());
            let procs = migration_list_processes(filter).await?;
            if procs.is_empty() {
                return Ok("No processes found.".to_string());
            }
            let pos_kw = ["pos","retail","revel","lightspeed","square","shopify",
                "odoo","quickbooks","quicksale","loyverse","vend","toast","clover","talech",
                "restaurant","cashier","billing","erp","sage","inventory"];
            let mut interesting = vec![];
            let mut rest = vec![];
            for p in &procs {
                let n = p.name.to_lowercase();
                if pos_kw.iter().any(|k| n.contains(k)) {
                    interesting.push(p);
                } else {
                    rest.push(p);
                }
            }
            let mut out = String::new();
            if !interesting.is_empty() {
                out += &format!("⚠ Likely POS processes ({}):\n", interesting.len());
                for p in &interesting {
                    out += &format!("  {} (PID: {}, {})\n", p.name, p.pid, p.memory_kb);
                }
                out += "\n";
            }
            out += &format!("All processes ({}), first 30:\n", procs.len());
            for p in rest.iter().take(30) {
                out += &format!("  {} (PID: {})\n", p.name, p.pid);
            }
            Ok(out)
        }

        "mg_read_file" => {
            let path = input.get("path").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing path".into()))?;
            // Finding 4: reject paths outside approved directories or in sensitive locations
            if !is_safe_read_path(path) {
                return Ok("❌ Access denied: path is outside approved directories or is a sensitive system path.".to_string());
            }
            let max_chars = input.get("max_chars").and_then(|v| v.as_u64()).unwrap_or(4000) as usize;
            migration_read_file(path.to_string(), Some(max_chars)).await
        }

        "mg_shell" => {
            let command = input.get("command").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing command".into()))?
                .to_string();
            exec_safe_shell(command).await
        }

        "mg_extract_zip" => {
            let archive_path = input.get("archive_path").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing archive_path".into()))?;
            let dest_dir = input.get("dest_dir").and_then(|v| v.as_str()).map(|s| s.to_string());
            let r = migration_decompress(archive_path.to_string(), dest_dir).await?;
            let mut out = format!("Extracted {} files to: {}\n", r.extracted_files.len(), r.dest_dir);
            if !r.db_files.is_empty() {
                out += &format!("\n✅ Found {} database file(s):\n", r.db_files.len());
                for f in &r.db_files {
                    out += &format!("  {}\n", f);
                }
            } else {
                out += "\nNo database files found inside the archive.";
            }
            if let Some(e) = &r.error { out += &format!("\nWarning: {}", e); }
            Ok(out)
        }

        "mg_attach_mdf" => {
            let mdf_path = input.get("mdf_path").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing mdf_path".into()))?
                .to_string();
            let db_name = input.get("db_name").and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    let stem = std::path::Path::new(&mdf_path)
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "imported_db".to_string());
                    // Sanitize stem: keep only alphanumeric, underscore, hyphen; fall back if invalid
                    let sanitized: String = stem.chars()
                        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                        .collect();
                    if sanitized.is_empty()
                        || !sanitized.chars().next().map(|c| c.is_ascii_alphabetic() || c == '_').unwrap_or(false)
                    {
                        "imported_db".to_string()
                    } else {
                        sanitized
                    }
                });
            exec_attach_mdf(mdf_path, db_name).await
        }

        "mg_access_query" => {
            let db_path = input.get("db_path").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing db_path".into()))?
                .to_string();
            let sql = input.get("sql").and_then(|v| v.as_str())
                .map(|s| s.to_string());
            exec_access_query(db_path, sql).await
        }

        "mg_read_sql_dump" => {
            let path = input.get("path").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing path".into()))?
                .to_string();
            let schema = inspect_sql_dump(path.clone()).await?;
            let mut out = format!("SQL dump: {}\n{} tables detected:\n\n", path, schema.sheets.len());
            for sheet in &schema.sheets {
                out += &format!("  {} — {} INSERT rows, {} columns: {}\n",
                    sheet.name, sheet.row_count,
                    sheet.columns.len(),
                    sheet.columns.iter().take(10).map(|c| c.name.as_str()).collect::<Vec<_>>().join(", "));
            }
            // Also show first 20 INSERT lines for context
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            let inserts: Vec<&str> = content.lines()
                .filter(|l| { let u = l.to_uppercase(); u.starts_with("INSERT") || u.starts_with("REPLACE") })
                .take(15)
                .collect();
            if !inserts.is_empty() {
                out += &format!("\nSample INSERT statements:\n");
                for ins in &inserts {
                    out += &format!("  {}\n", &ins[..ins.len().min(200)]);
                }
            }
            Ok(out)
        }

        "mg_zanpos_stats" => {
            let products: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products").fetch_one(pool).await.unwrap_or(0);
            let categories: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM categories").fetch_one(pool).await.unwrap_or(0);
            let customers: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM customers").fetch_one(pool).await.unwrap_or(0);
            let sales: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sales").fetch_one(pool).await.unwrap_or(0);
            let sale_items: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sale_items").fetch_one(pool).await.unwrap_or(0);
            let stock_levels: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM stock_levels").fetch_one(pool).await.unwrap_or(0);
            Ok(format!(
                "Current ZANPOS records:\n  Products: {}\n  Categories: {}\n  Customers: {}\n  Sales: {}\n  Sale Items: {}\n  Stock Levels: {}",
                products, categories, customers, sales, sale_items, stock_levels
            ))
        }

        other => Err(AppError::Validation(format!("Unknown migration tool: {}", other))),
    }
}

// ── MDF attach (SQL Server Express) ───────────────────────────────────────────

async fn exec_attach_mdf(mdf_path: String, db_name: String) -> AppResult<String> {
    // Validate db_name: must be a safe SQL identifier
    let db_name_valid = !db_name.is_empty()
        && db_name.len() <= 64
        && db_name.chars().next().map(|c| c.is_ascii_alphabetic() || c == '_').unwrap_or(false)
        && db_name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !db_name_valid {
        return Ok(format!(
            "❌ Invalid database name '{db_name}'. Use only letters, digits, underscores, and hyphens."
        ));
    }

    // Validate mdf_path: must end in .mdf and contain no SQL-injection characters
    let mdf_lower = mdf_path.to_lowercase();
    if !mdf_lower.ends_with(".mdf") {
        return Ok("❌ Path must end in .mdf".to_string());
    }
    if mdf_path.chars().any(|c| matches!(c, '\'' | ';' | '[' | ']' | '\n' | '\r')) {
        return Ok("❌ MDF path contains invalid characters.".to_string());
    }

    tokio::task::spawn_blocking(move || -> AppResult<String> {
        // Try to find sqlcmd in PATH or common locations
        let sqlcmd_paths = [
            "sqlcmd".to_string(),
            r"C:\Program Files\Microsoft SQL Server\Client SDK\ODBC\170\Tools\Binn\sqlcmd.exe".to_string(),
            r"C:\Program Files\Microsoft SQL Server\Client SDK\ODBC\160\Tools\Binn\sqlcmd.exe".to_string(),
            r"C:\Program Files\Microsoft SQL Server\Client SDK\ODBC\150\Tools\Binn\sqlcmd.exe".to_string(),
        ];

        // Try common SQL Server Express instance names
        let instances = [r".\SQLEXPRESS", r".\MSSQLSERVER", r"(local)", r".\SQL2019", r".\SQL2022"];

        // First check if sqlcmd is available
        let sqlcmd_available = sqlcmd_paths.iter().any(|cmd| {
            std::process::Command::new(cmd).arg("-?").output().is_ok()
        });

        if !sqlcmd_available {
            return Ok(format!(
                "sqlcmd not found on this machine.\n\nTo attach .mdf files you need SQL Server Express (free download from Microsoft). Install it, then retry.\n\nAlternatively:\n1. If you have SQL Server Management Studio (SSMS), attach the .mdf manually: right-click Databases → Attach → select {}\n2. Then connect using: Server=.\\SQLEXPRESS;Database={};Trusted_Connection=True;",
                mdf_path, db_name
            ));
        }

        // F-MED-11: Use expect with a descriptive message instead of bare .unwrap()
        // (sqlcmd_available was already checked above, so this path is unreachable,
        // but we avoid a panic with no context if that logic ever changes)
        let sqlcmd_bin = sqlcmd_paths.iter().find(|cmd| {
            std::process::Command::new(*cmd).arg("-?").output().is_ok()
        }).ok_or_else(|| crate::errors::AppError::Internal(
            "sqlcmd binary not found — install SQL Server Express and retry".into()
        ))?;

        // Try each instance to find one that works
        for instance in &instances {
            let attach_sql = format!(
                "IF NOT EXISTS (SELECT * FROM sys.databases WHERE name = N'{db}') \
                 BEGIN CREATE DATABASE [{db}] ON (FILENAME = N'{mdf}') FOR ATTACH_REBUILD_LOG END; \
                 SELECT name FROM sys.databases WHERE name = N'{db}'",
                db = db_name,
                mdf = mdf_path
            );

            let result = std::process::Command::new(sqlcmd_bin)
                .args(["-S", instance, "-Q", &attach_sql, "-b"])
                .output();

            if let Ok(out) = result {
                if out.status.success() {
                    let conn_str = format!("Server={};Database={};Trusted_Connection=True;", instance, db_name);
                    return Ok(format!(
                        "✅ Database '{}' attached successfully to SQL Server instance: {}\n\nConnection string:\n{}\n\nYou can now use mg_connect_db with:\n  db_type: \"mssql\"\n  conn_str: \"{}\"",
                        db_name, instance, conn_str, conn_str
                    ));
                }
            }
        }

        Ok(format!(
            "Could not attach {} to any SQL Server Express instance.\n\nTried instances: {}\n\nManual steps:\n1. Open SQL Server Management Studio (SSMS)\n2. Right-click Databases → Attach\n3. Navigate to: {}\n4. Then connect using: Server=.\\SQLEXPRESS;Database={};Trusted_Connection=True;",
            mdf_path,
            instances.join(", "),
            mdf_path,
            db_name
        ))
    })
    .await
    .map_err(|e| AppError::Internal(format!("MDF attach task failed: {}", e)))?
}

// ── Access DB query (PowerShell OleDb) ────────────────────────────────────────

async fn exec_access_query(db_path: String, sql_query: Option<String>) -> AppResult<String> {
    // Validate db_path: must end in .accdb or .mdb and contain no injection characters
    let db_lower = db_path.to_lowercase();
    if !db_lower.ends_with(".accdb") && !db_lower.ends_with(".mdb") {
        return Ok("❌ Path must end in .accdb or .mdb".to_string());
    }
    if db_path.chars().any(|c| matches!(c, '\'' | '"' | ';' | '`' | '\n' | '\r' | '$')) {
        return Ok("❌ Database path contains invalid characters.".to_string());
    }
    // Validate the SQL query if provided: reject obviously destructive statements
    if let Some(ref q) = sql_query {
        let q_up = q.to_uppercase();
        for banned in &["DELETE", "UPDATE", "INSERT", "DROP", "CREATE", "ALTER", "TRUNCATE", "EXEC", "EXECUTE"] {
            if q_up.contains(banned) {
                return Ok(format!("❌ Destructive keyword '{banned}' is not allowed in Access queries."));
            }
        }
    }

    let sql = sql_query;
    // Build PowerShell script using OleDb / ACE driver
    let ps_script = if let Some(ref query) = sql {
        // Execute a query
        format!(r#"
try {{
    $conn = New-Object System.Data.OleDb.OleDbConnection
    # Try ACE (Office 2016+), then Jet (older)
    $providers = @('Microsoft.ACE.OLEDB.16.0','Microsoft.ACE.OLEDB.12.0','Microsoft.Jet.OLEDB.4.0')
    $opened = $false
    foreach ($prov in $providers) {{
        try {{
            $conn.ConnectionString = "Provider=$prov;Data Source='{path}';Persist Security Info=False"
            $conn.Open()
            $opened = $true
            break
        }} catch {{ }}
    }}
    if (-not $opened) {{ Write-Error "No Access driver found. Install Microsoft Access Database Engine from microsoft.com/download"; exit 1 }}
    $cmd = $conn.CreateCommand()
    $cmd.CommandText = '{query}'
    $rdr = $cmd.ExecuteReader()
    $cols = @(); for ($i=0; $i -lt $rdr.FieldCount; $i++) {{ $cols += $rdr.GetName($i) }}
    Write-Output ($cols -join "`t")
    $n = 0
    while ($rdr.Read() -and $n -lt 100) {{
        $row = @(); for ($i=0; $i -lt $rdr.FieldCount; $i++) {{ $row += "$($rdr.GetValue($i))" }}
        Write-Output ($row -join "`t"); $n++
    }}
    $conn.Close()
    Write-Output "--- $n rows returned ---"
}} catch {{ Write-Error $_.Exception.Message }}
"#, path = db_path, query = query.replace('\'', "''"))
    } else {
        // List tables
        format!(r#"
try {{
    $conn = New-Object System.Data.OleDb.OleDbConnection
    $providers = @('Microsoft.ACE.OLEDB.16.0','Microsoft.ACE.OLEDB.12.0','Microsoft.Jet.OLEDB.4.0')
    $opened = $false
    foreach ($prov in $providers) {{
        try {{
            $conn.ConnectionString = "Provider=$prov;Data Source='{path}';Persist Security Info=False"
            $conn.Open()
            $opened = $true
            break
        }} catch {{ }}
    }}
    if (-not $opened) {{ Write-Error "No Access driver found. Install Microsoft Access Database Engine from microsoft.com/download"; exit 1 }}
    $schema = $conn.GetOleDbSchemaTable([System.Data.OleDb.OleDbSchemaGuid]::Tables, $null)
    $tables = $schema.Rows | Where-Object {{ $_['TABLE_TYPE'] -eq 'TABLE' }} | ForEach-Object {{ $_['TABLE_NAME'] }}
    Write-Output "Tables in $($conn.Database):"
    foreach ($t in $tables) {{
        try {{
            $cmd = $conn.CreateCommand(); $cmd.CommandText = "SELECT COUNT(*) FROM [$t]"
            $cnt = $cmd.ExecuteScalar()
            Write-Output "  $t ($cnt rows)"
        }} catch {{ Write-Output "  $t" }}
    }}
    $conn.Close()
}} catch {{ Write-Error $_.Exception.Message }}
"#, path = db_path)
    };

    // Run via PowerShell
    let cmd = format!("powershell -NoProfile -Command \"{}\"", ps_script.replace('"', "\\\""));
    exec_safe_shell(cmd).await
}

// ── Safe shell executor ────────────────────────────────────────────────────────

async fn exec_safe_shell(command: String) -> AppResult<String> {
    // Reject shell metacharacters that cmd.exe interprets as command separators,
    // redirectors, or code-execution operators regardless of the allowlist.
    let dangerous_chars = ['&', '|', '>', '<', '^', ';', '`'];
    if command.chars().any(|c| dangerous_chars.contains(&c))
        || command.contains("$(")
        || command.contains('\n')
        || command.contains('\r')
    {
        return Ok("Command rejected: shell metacharacters are not permitted.".to_string());
    }

    // Whitelist: only safe read-only commands allowed
    let lower = command.trim().to_lowercase();
    let allowed_prefixes = [
        "dir ", "dir\n", "type ", "where ", "tasklist", "reg query",
        "wmic process", "sqlite3 ", "powershell get-", "powershell -command \"get-",
        "powershell -command \"select-", "powershell -noprofile",
        "findstr ", "find ", "echo ", "more ", "sort ", "attrib "
    ];
    let allowed_exact = ["dir", "tasklist", "where"];

    let dangerous = lower.contains("remove-item") || lower.contains("del ") || lower.contains("rd ")
        || lower.contains("invoke-expression") || lower.contains("iex ")
        || lower.contains(" | iex") || lower.contains("net user") || lower.contains("format-disk")
        || lower.contains("clear-disk") || lower.contains("set-acl") || lower.contains("start-process cmd");

    let is_allowed = !dangerous && (
        allowed_prefixes.iter().any(|p| lower.starts_with(p))
        || allowed_exact.iter().any(|e| lower == *e)
        || lower.starts_with("dir ")
        || lower.starts_with("sqlite3 ")
        || (lower.starts_with("powershell") && !lower.contains("remove") && !lower.contains("delete")
            && !lower.contains("write-") && !lower.contains("set-content") && !lower.contains("new-item")
            && !lower.contains("invoke-expression") && !lower.contains("iex "))
    );

    if !is_allowed {
        return Ok(format!(
            "Command not permitted for safety reasons: '{}'\n\nAllowed commands: dir, type, where, tasklist, reg query, wmic process, sqlite3, powershell Get-* / Select-String, findstr, echo, attrib",
            command
        ));
    }

    tokio::task::spawn_blocking(move || {
        let output = std::process::Command::new("cmd")
            .args(["/C", &command])
            .output()
            .map_err(|e| AppError::Internal(format!("Shell exec failed: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let mut result = String::new();
        if !stdout.is_empty() {
            // Cap at 6000 chars
            let trimmed = if stdout.len() > 6000 {
                format!("{}\n...(output truncated to 6000 chars)", &stdout[..6000])
            } else {
                stdout
            };
            result += &trimmed;
        }
        if !stderr.is_empty() {
            result += &format!("\nSTDERR: {}", &stderr[..stderr.len().min(500)]);
        }
        if result.is_empty() {
            result = "(no output)".to_string();
        }
        Ok(result)
    })
    .await
    .map_err(|e| AppError::Internal(format!("Shell task failed: {}", e)))?
}

// ── System prompt ──────────────────────────────────────────────────────────────

fn migration_agent_system_prompt() -> String {
    r#"You are the ZANPOS Migration Agent — an expert AI assistant that helps business owners import their existing POS/retail data into ZANPOS.

You have access to powerful tools that let you read and migrate ALL common database formats:

## Supported source formats

| Format | Extension | Tool to use |
|--------|-----------|-------------|
| SQLite | .db .sqlite .sqlite3 .db3 .s3db | `mg_inspect_file` or `mg_connect_db` (db_type=sqlite) |
| SQL dump | .sql | `mg_read_sql_dump` then `mg_inspect_file` |
| MySQL | connection string | `mg_connect_db` (db_type=mysql), then `mg_query_db` |
| SQL Server (live) | connection string | `mg_connect_db` (db_type=mssql), then `mg_query_db` |
| SQL Server MDF file | .mdf | `mg_attach_mdf` → get conn string → `mg_connect_db` (db_type=mssql) |
| Microsoft Access | .accdb .mdb | `mg_access_query` (uses Windows OleDb driver) |
| Excel | .xlsx .xls .xlsm | `mg_inspect_file` |
| CSV | .csv | `mg_inspect_file` |
| JSON export | .json | `mg_inspect_file` |
| ZIP backup | .zip | `mg_extract_zip` → find DB inside → inspect |

## Your tools
- `mg_inspect_file` — Read file schema (SQLite/CSV/Excel/SQL dump/JSON): tables, columns, row counts, samples
- `mg_connect_db` — Test + list tables for SQLite, MySQL, MSSQL connection strings
- `mg_query_db` — Run a SELECT query against any connected external DB
- `mg_find_db_files` — Scan filesystem for .db .sqlite .mdf .mdb .accdb .sql .json etc.
- `mg_list_processes` — Detect what POS software is running (find data directory clues)
- `mg_read_file` — Read a config/ini/xml file to find connection strings
- `mg_shell` — Run safe read-only shell commands (dir, tasklist, sqlite3, powershell Get-*)
- `mg_extract_zip` — Extract a ZIP backup archive and find databases inside
- `mg_attach_mdf` — Attach a SQL Server .mdf file to local SQL Server Express instance
- `mg_access_query` — Query a Microsoft Access .accdb or .mdb database via Windows OleDb
- `mg_read_sql_dump` — Analyse a .sql dump file: find tables, columns, sample INSERTs
- `mg_zanpos_stats` — Count current ZANPOS records (before/after verification)

## Workflow

1. **Understand** — Ask what POS system they're coming from: QuickBooks, Loyverse, Square, Shopify, Odoo, custom Access/SQLite, etc.

2. **Locate the data** — Use `mg_find_db_files` and `mg_list_processes` first. Check common paths: C:\ProgramData, AppData, Program Files (x86), Desktop. Use `mg_shell` with `dir` for specific directories.

3. **Identify the format**:
   - Found a .sql file? → `mg_read_sql_dump` to understand it
   - Found a .mdf file? → `mg_attach_mdf` to mount it, then `mg_connect_db`
   - Found a .accdb/.mdb? → `mg_access_query` (no SQL = lists tables)
   - Found a .db/.sqlite? → `mg_inspect_file` directly
   - Have a connection string? → `mg_connect_db`

4. **Sample the data** — Use `mg_query_db` or `mg_access_query` to see actual rows: `SELECT TOP 10 * FROM dbo.Product`

5. **Map and plan** — Explain exactly which source columns map to which ZANPOS columns. Confirm with the user before proceeding.

6. **Execute** — Guide the user to use the 📂 file picker (for file-based DBs) or the connection form (🔌 Test DB button) to run the actual import through the migration wizard above.

7. **Verify** — Run `mg_zanpos_stats` after to confirm the import worked.

## ZANPOS target schema (complete)

### categories
| Column | Type | Notes |
|--------|------|-------|
| category_id | TEXT | ULID, generate new |
| name | TEXT NOT NULL | |
| color | TEXT | hex color e.g. '#FF5733', nullable |
| is_active | INTEGER | 1=active, 0=inactive |
| created_at | TEXT | ISO8601 UTC |

### products
| Column | Type | Notes |
|--------|------|-------|
| product_id | TEXT | ULID, generate new |
| category_id | TEXT | FK → categories.category_id |
| name | TEXT NOT NULL | |
| description | TEXT | nullable |
| sku | TEXT | nullable, must be unique if set |
| barcode | TEXT | nullable (legacy single barcode) |
| price_minor | INTEGER NOT NULL | BHD × 1000: 1.500 BHD = 1500 |
| cost_minor | INTEGER | nullable, BHD × 1000 |
| track_inventory | INTEGER | 1=track, 0=service/don't track |
| is_active | INTEGER | 1=active |
| created_at | TEXT | ISO8601 UTC |

### product_barcodes (multi-barcode)
| Column | Type | Notes |
|--------|------|-------|
| barcode_id | TEXT | ULID |
| product_id | TEXT | FK → products.product_id |
| barcode | TEXT NOT NULL UNIQUE | |
| created_at | TEXT | ISO8601 UTC |

### customers
| Column | Type | Notes |
|--------|------|-------|
| customer_id | TEXT | ULID |
| name | TEXT NOT NULL | |
| phone | TEXT | E.164 format e.g. +97336001234 |
| email | TEXT | nullable |
| loyalty_points | INTEGER | default 0 |
| notes | TEXT | nullable |
| created_at | TEXT | ISO8601 UTC |

### stock_levels
| Column | Type | Notes |
|--------|------|-------|
| stock_level_id | TEXT | ULID |
| product_id | TEXT | FK → products.product_id |
| branch_id | TEXT | use the business's primary branch_id |
| quantity_on_hand | TEXT | decimal as text e.g. '10.000' |
| updated_at | TEXT | ISO8601 UTC |

### sales (historical import)
| Column | Type | Notes |
|--------|------|-------|
| sale_id | TEXT | ULID |
| receipt_number | TEXT NOT NULL | preserve original receipt/doc number |
| branch_id | TEXT | primary branch |
| shift_id | TEXT | use a synthetic 'MIGRATION' shift_id |
| cashier_user_id | TEXT | map from source user or use owner user_id |
| customer_id | TEXT | nullable |
| net_total_minor | INTEGER | BHD × 1000 |
| discount_minor | INTEGER | default 0 |
| tax_minor | INTEGER | default 0 |
| status | TEXT | always 'completed' for historical |
| business_date | TEXT | YYYY-MM-DD |
| sold_at | TEXT | ISO8601 UTC |
| source | TEXT | set to 'migrated' |

### sale_items
| Column | Type | Notes |
|--------|------|-------|
| sale_item_id | TEXT | ULID |
| sale_id | TEXT | FK → sales.sale_id |
| product_id | TEXT | nullable (if product deleted) |
| product_name | TEXT NOT NULL | snapshot of name at time of sale |
| quantity | TEXT | decimal as text e.g. '2.000' |
| unit_price_minor | INTEGER | BHD × 1000 |
| line_total_minor | INTEGER | unit_price_minor × qty |
| discount_minor | INTEGER | default 0 |

### payments (linked to sales)
| Column | Type | Notes |
|--------|------|-------|
| payment_id | TEXT | ULID |
| sale_id | TEXT | FK → sales.sale_id |
| method | TEXT | 'cash' or 'card' or 'wallet' |
| amount_minor | INTEGER | BHD × 1000 |
| reference | TEXT | nullable (card auth code etc.) |

---

## Known POS Schema: Legacy SQL Server POS (the user's previous POS system)

This is the primary legacy POS this deployment is migrating FROM. The database is a SQL Server MDF file with ~65 tables in the `dbo` schema. All column names are PascalCase. IDs are INT (not ULID). Prices are in FULL currency units (BHD) — multiply by 1000 to get ZANPOS minor units.

### STEP 1 — Attach the MDF

Use `mg_attach_mdf` with the path to the .mdf file. This mounts it to SQL Server Express as a read-only database. The tool returns a connection string like:
`Server=.\SQLEXPRESS;Database=ZanPosMigration_XXXXXXXX;Trusted_Connection=True`

Then use `mg_connect_db` (db_type=mssql) with that connection string to confirm connectivity.

### STEP 2 — Categories (ProductGroup → categories)

Query:
```sql
SELECT Id, Name, ParentGroupId, IsEnabled FROM dbo.ProductGroup ORDER BY Id
```
Mapping:
- `ProductGroup.Id` (INT) → generate a new ULID for `categories.category_id`
  - Keep an in-memory map: old_int_id → new_ulid for foreign key resolution
- `ProductGroup.Name` → `categories.name`
- `ProductGroup.IsEnabled` (BIT) → `categories.is_active` (1/0)
- `ProductGroup.ParentGroupId` — informational only; ZANPOS categories are flat; ignore hierarchy
- `color` → set '#6B7280' (neutral gray) as default

INSERT example:
```sql
INSERT INTO categories (category_id, name, color, is_active, created_at)
VALUES ('01NEWULID...', 'Beverages', '#6B7280', 1, '2024-01-01T00:00:00Z');
```

### STEP 3 — Products (Product → products + product_barcodes)

Query products:
```sql
SELECT p.Id, p.Name, p.Code, p.PLU, p.Price, p.Cost, p.IsEnabled,
       p.ProductGroupId, p.IsService, p.Description
FROM dbo.Product p
WHERE p.IsDeleted = 0 OR p.IsDeleted IS NULL
ORDER BY p.Id
```

Mapping:
- `Product.Id` (INT) → generate new ULID for `products.product_id`; keep map old→new
- `Product.Name` → `products.name`
- `Product.Code` → `products.sku` (nullable; skip if empty string)
- `Product.PLU` → `products.barcode` (the primary barcode)
- `Product.Price` (DECIMAL, full BHD) → `products.price_minor` = ROUND(Price × 1000) as INTEGER
- `Product.Cost` (DECIMAL, full BHD) → `products.cost_minor` = ROUND(Cost × 1000) as INTEGER (nullable)
- `Product.IsEnabled` (BIT) → `products.is_active`
- `Product.ProductGroupId` (INT) → `products.category_id` via ProductGroup ULID map
- `Product.IsService` (BIT): if 1 → `track_inventory = 0`, else `track_inventory = 1`
- `Product.Description` → `products.description`

Then query barcodes:
```sql
SELECT b.ProductId, b.Value FROM dbo.Barcode b
WHERE b.Value IS NOT NULL AND b.Value <> ''
ORDER BY b.ProductId
```
Mapping:
- `Barcode.ProductId` → look up new product ULID from map
- `Barcode.Value` → `product_barcodes.barcode`

Price safety check: skip any product where Price > 10000 or Price < 0 (data corruption).

### STEP 4 — Customers (Customer → customers)

Query:
```sql
SELECT Id, Name, PhoneNumber, Email, LoyaltyCard, Notes, IsEnabled
FROM dbo.Customer
WHERE IsDeleted = 0 OR IsDeleted IS NULL
ORDER BY Id
```

Mapping:
- `Customer.Id` → new ULID for `customers.customer_id`
- `Customer.Name` → `customers.name`
- `Customer.PhoneNumber` → `customers.phone`
  - Normalize: if starts with '3' or '6' and 8 digits → prepend '+973'
  - If already starts with '+' → keep as-is
  - If empty/null → NULL
- `Customer.Email` → `customers.email`
- `Customer.LoyaltyCard` → can be stored in `customers.notes` as "Loyalty Card: {value}"
- `Customer.Notes` → `customers.notes` (append to loyalty card note if both present)

### STEP 5 — Stock Levels (Stock → stock_levels)

Query:
```sql
SELECT s.ProductId, SUM(s.Quantity) AS TotalQty
FROM dbo.Stock s
GROUP BY s.ProductId
```
(Sum across all warehouses — ZANPOS uses a single branch stock level)

Mapping:
- `Stock.ProductId` → look up new product ULID
- `SUM(Quantity)` → `stock_levels.quantity_on_hand` as TEXT decimal e.g. '12.000'
- Only insert for products that have `track_inventory = 1`
- `branch_id` → query ZANPOS: `SELECT branch_id FROM branches LIMIT 1` to get the real branch_id

### STEP 6 — Historical Sales (Document → sales + sale_items + payments)

**IMPORTANT**: The `Document` table contains ALL transaction types (sales, purchases, returns, adjustments). Only migrate SALES. Filter by DocumentType.

First, discover document types:
```sql
SELECT DISTINCT d.DocumentTypeId, dt.Name, COUNT(*) as cnt
FROM dbo.Document d
JOIN dbo.DocumentType dt ON dt.Id = d.DocumentTypeId
GROUP BY d.DocumentTypeId, dt.Name
ORDER BY cnt DESC
```

Sales are typically DocumentType where Name LIKE '%Sale%' or '%Invoice%' or '%POS%' or '%Retail%'.
**Do not migrate** types like: Purchase, Return, Adjustment, Transfer, StockIn, StockOut, Void.
Ask the user to confirm which DocumentTypeId(s) are POS sales before proceeding.

Query sales (replace @SaleTypeId with the confirmed ID(s)):
```sql
SELECT d.Id, d.Number, d.DocumentDate, d.Total, d.Discount,
       d.UserId, d.CustomerId, d.Status
FROM dbo.Document d
WHERE d.DocumentTypeId IN (@SaleTypeId)
  AND (d.IsDeleted = 0 OR d.IsDeleted IS NULL)
ORDER BY d.DocumentDate
```

Mapping:
- `Document.Id` (INT) → new ULID for `sales.sale_id`; keep map old→new
- `Document.Number` → `sales.receipt_number` (preserve original)
- `Document.DocumentDate` → `sales.business_date` (YYYY-MM-DD) and `sales.sold_at` (ISO UTC)
- `Document.Total` (DECIMAL full BHD) → `sales.net_total_minor` = ROUND(Total × 1000)
- `Document.Discount` → `sales.discount_minor` = ROUND(Discount × 1000)
- `Document.UserId` → `sales.cashier_user_id` (map to ZANPOS user, or use owner user_id as fallback)
- `Document.CustomerId` → `sales.customer_id` via Customer ULID map (nullable)
- `sales.status` = 'completed' always
- `sales.source` = 'migrated'
- `sales.shift_id` = 'MIGRATION' (use this literal string; it's allowed as a synthetic shift)
- Get `branch_id` from ZANPOS branches table

Query sale items:
```sql
SELECT di.DocumentId, di.ProductId, di.ProductName, di.Quantity,
       di.UnitPrice, di.Total, di.Discount
FROM dbo.DocumentItem di
WHERE di.DocumentId IN (SELECT Id FROM dbo.Document WHERE DocumentTypeId IN (@SaleTypeId))
ORDER BY di.DocumentId, di.Id
```

Mapping:
- `DocumentItem.DocumentId` → `sale_items.sale_id` via Document ULID map
- `DocumentItem.ProductId` → `sale_items.product_id` via Product ULID map (nullable if product not migrated)
- `DocumentItem.ProductName` → `sale_items.product_name`
- `DocumentItem.Quantity` → `sale_items.quantity` as TEXT e.g. '2.000'
- `DocumentItem.UnitPrice` × 1000 → `sale_items.unit_price_minor`
- `DocumentItem.Total` × 1000 → `sale_items.line_total_minor`
- `DocumentItem.Discount` × 1000 → `sale_items.discount_minor`

Query payments:
```sql
SELECT dp.DocumentId, dp.PaymentMethodId, pm.Name AS PayMethodName, dp.Amount
FROM dbo.DocumentPayment dp
JOIN dbo.PaymentMethod pm ON pm.Id = dp.PaymentMethodId
WHERE dp.DocumentId IN (SELECT Id FROM dbo.Document WHERE DocumentTypeId IN (@SaleTypeId))
```

Mapping:
- `Amount` × 1000 → `payments.amount_minor`
- `PayMethodName`: map 'Cash'/'نقد' → 'cash'; 'Card'/'Visa'/'Master'/'بطاقة' → 'card'; everything else → 'wallet'

### STEP 7 — Users (User → reference only, do NOT overwrite)

ZANPOS users are already set up during the setup wizard. Do NOT replace them.
The User table is read-only reference for mapping cashier IDs to names in historical sales.

Query to build the mapping:
```sql
SELECT Id, FirstName, LastName, Username, AccessLevel FROM dbo.User WHERE IsEnabled = 1
```
- Identify which ZANPOS user_id corresponds to each old User by matching name/username
- Use this map only for `sales.cashier_user_id` in Step 6
- If no match found, use the owner's user_id as default

### STEP 8 — Tax Rules (Tax → tax_rules, optional)

Query:
```sql
SELECT Id, Name, Rate, IsEnabled FROM dbo.Tax
```
- `Tax.Rate` is a percentage (e.g. 10.0 = 10%) → `tax_rules.rate_basis_points` = ROUND(Rate × 100) as INTEGER
  - 10% → 1000 basis points; 5% → 500 basis points; 0% → 0
- Check if ZANPOS already has tax rules before inserting to avoid duplicates

---

## Migration execution strategy

When executing the actual migration via the migration wizard UI:

1. **Order matters**: categories → products → product_barcodes → customers → stock_levels → (sales → sale_items → payments)
2. **Use transactions**: wrap each table's inserts in a transaction; if any batch fails, roll back that table only
3. **Batch size**: insert 100 rows at a time to avoid timeout
4. **ULID generation**: generate ULIDs in Rust (the migration_execute command handles this). For the AI-generated SQL script, use placeholder `lower(hex(randomblob(10)))` which SQLite supports.
5. **Idempotency**: check for existing data before inserting; use `INSERT OR IGNORE` where possible
6. **Report counts**: after each table, report inserted/skipped/error counts

---

## Rules
- ALWAYS inspect/sample the data before planning a migration
- Prices in BHD → multiply by 1000 for minor units; USD → convert at current rate × 1000
- Never modify or delete data in external databases — read only
- For .mdf files: if sqlcmd is not installed, guide user to attach via SSMS manually
- For Access: if the ACE driver is missing, provide the download link: aka.ms/accessruntime
- Be specific: tell the user exactly which file/table/column you found
- For the legacy SQL Server POS: always filter Document by DocumentTypeId (sales only)
- Never migrate users — they are already configured in ZANPOS
- Preserve original receipt numbers from Document.Number"#.to_string()
}

// ── Main Tauri command ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn migration_agent_chat(
    input: MigrationAgentChatInput,
    state: State<'_, AppState>,
) -> AppResult<String> {
    let Some(provider) = Provider::from_db(&state.db).await? else {
        return Err(AppError::Internal("No AI provider configured. Please set up your API key.".into()));
    };

    let system = migration_agent_system_prompt();
    let tool_defs = migration_agent_tool_definitions();

    let mut current = provider
        .send_chat(&system, &input.history, &input.message, &tool_defs)
        .await?;

    // Tool execution loop — up to 12 autonomous steps
    let mut last_tool_names: Vec<String> = Vec::new();
    for _turn in 0..12 {
        let Some(tc) = current.tool_call else {
            // No more tool calls — return the final text
            return Ok(current.text);
        };

        // Repetition guard: abort if the same tool is called 3 times in a row
        last_tool_names.push(tc.name.clone());
        if last_tool_names.len() >= 3 {
            let last3 = &last_tool_names[last_tool_names.len() - 3..];
            if last3[0] == last3[1] && last3[1] == last3[2] {
                return Ok(format!(
                    "I've called the same tool ({}) three times in a row without progress. \
                     Please try rephrasing your request or providing more specific information.",
                    tc.name
                ));
            }
        }

        let tool_result = execute_migration_agent_tool(&tc.name, &tc.input, &state.db)
            .await
            .unwrap_or_else(|e| format!("Tool error: {}", e));

        let prev_reasoning = current.reasoning_content.clone();
        current = provider
            .continue_with_tool_result(
                &system,
                &input.history,
                &input.message,
                &ToolCallResult {
                    id: tc.id,
                    name: tc.name,
                    input: tc.input,
                },
                tool_result,
                &tool_defs,
                prev_reasoning,
            )
            .await?;
    }

    Ok(current.text)
}
