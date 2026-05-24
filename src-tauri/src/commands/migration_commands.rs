// ─── Imports ──────────────────────────────────────────────────────────────────
use crate::ai::provider::Provider;
use crate::errors::{AppError, AppResult};
use crate::AppState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
    {
        inspect_sqlite(path).await
    } else {
        // Try excel first, then sqlite
        let path2 = path.clone();
        match inspect_excel(path).await {
            Ok(s) => Ok(s),
            Err(_) => inspect_sqlite(path2).await,
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
        AppError::Internal(format!(
            "AI returned invalid JSON: {}. Raw: {}",
            e,
            &result.text[..result.text.len().min(500)]
        ))
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
    on_event: tauri::ipc::Channel<MigrationProgress>,
    state: State<'_, AppState>,
) -> AppResult<()> {
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
                &on_event,
            )
            .await
            .unwrap_or_else(|e| {
                let _ = on_event.send(MigrationProgress::Error {
                    message: format!("Sheet '{}' error: {}", sheet_mapping.source_sheet, e),
                });
                (0, total_rows)
            });

            let _ = on_event.send(MigrationProgress::SheetDone {
                sheet: sheet_mapping.source_sheet.clone(),
                target: target.to_string(),
                inserted,
                skipped,
            });
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
            "sales" | "sale_items" => {
                // Basic handling — skip if FK not found
                Ok(false)
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
            let f: f64 = value.trim().replace(',', ".").parse().unwrap_or(0.0);
            let minor = (f * 10f64.powi(currency_exponent as i32)).round() as i64;
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
    // Try YYYY-MM-DD
    if v.len() == 10 && &v[4..5] == "-" && &v[7..8] == "-" {
        return v.to_string();
    }
    // Try DD/MM/YYYY
    if v.len() == 10 && &v[2..3] == "/" && &v[5..6] == "/" {
        let parts: Vec<&str> = v.split('/').collect();
        if parts.len() == 3 {
            return format!("{}-{}-{}", parts[2], parts[1], parts[0]);
        }
    }
    // Try MM/DD/YYYY
    if v.len() == 10 && &v[2..3] == "/" && &v[5..6] == "/" {
        let parts: Vec<&str> = v.split('/').collect();
        if parts.len() == 3 {
            return format!("{}-{}-{}", parts[2], parts[0], parts[1]);
        }
    }
    // Try DD-MM-YYYY
    if v.len() == 10 && &v[2..3] == "-" && &v[5..6] == "-" {
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

    sqlx::query(
        "INSERT OR IGNORE INTO categories (category_id, name, sort_order, is_active, created_at, updated_at) VALUES (?, ?, ?, ?, datetime('now'), datetime('now'))"
    )
    .bind(&category_id)
    .bind(&name)
    .bind(sort_order)
    .bind(is_active)
    .execute(&mut **tx)
    .await?;

    category_cache.insert(name.trim().to_lowercase(), category_id);
    Ok(true)
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

    // Fallback: if no category, try to get first available category from DB
    let category_id = match category_id {
        Some(id) if !id.is_empty() => id,
        _ => {
            // Try to get any category from DB
            sqlx::query_scalar::<_, String>("SELECT category_id FROM categories LIMIT 1")
                .fetch_optional(pool)
                .await
                .unwrap_or_default()
                .unwrap_or_else(|| "01JCAT000000000000000001".to_string())
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

    sqlx::query(
        "INSERT OR IGNORE INTO products (product_id, category_id, name, barcode, sku, track_inventory, is_active, tax_rule_id, currency, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, '01JTAX000000000000ZERO01', 'BHD', datetime('now'), datetime('now'))"
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

    // Insert price if present
    let price_minor = get_mapped_value(sheet_mapping, row, "price_minor", currency_exponent)
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);

    let price_id = Ulid::new().to_string();
    sqlx::query(
        "INSERT OR IGNORE INTO product_prices (price_id, product_id, price_minor, currency, is_default, effective_from, created_at) VALUES (?, ?, ?, 'BHD', 1, date('now'), datetime('now'))"
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

    // Get branch_id from app_config
    let branch_id = "01JBRANCH0000000000000001";

    sqlx::query(
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

    Ok(true)
}

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
    let branch_id = get_mapped_value(sheet_mapping, row, "branch_id", currency_exponent)
        .unwrap_or_else(|| "01JBRANCH0000000000000001".to_string());
    let quantity_on_hand = get_mapped_value(sheet_mapping, row, "quantity_on_hand", currency_exponent)
        .unwrap_or_else(|| "0".to_string());

    sqlx::query(
        "INSERT OR IGNORE INTO stock_levels (stock_level_id, product_id, branch_id, quantity_on_hand, updated_at) VALUES (?, ?, ?, ?, datetime('now'))"
    )
    .bind(&stock_level_id)
    .bind(&product_id)
    .bind(&branch_id)
    .bind(&quantity_on_hand)
    .execute(&mut **tx)
    .await?;

    Ok(true)
}
