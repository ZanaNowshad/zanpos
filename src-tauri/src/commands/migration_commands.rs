// ─── Imports ──────────────────────────────────────────────────────────────────
use crate::ai::client::{
    AnthropicMessage, ContentBlock, ToolDef, extract_text, extract_tool_use,
};
use crate::ai::openai_client::{
    OpenAIMessage, OpenAIToolCallResult, assistant_msg, assistant_tool_call_msg,
    tool_result_msg, user_msg,
};
use crate::ai::provider::Provider;
use crate::domain::ai_admin::ChatMessage;
use crate::errors::{AppError, AppResult};
use crate::AppState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::State;
use ulid::Ulid;

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
pub struct MigrationContext {
    pub db_type: Option<String>,
    pub path_or_connstr: Option<String>,
    pub attached_file_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MigrationChatInput {
    pub history: Vec<ChatMessage>,
    pub message: String,
    pub user_id: String,
    pub context: MigrationContext,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum MigrationChatResponse {
    Message { content: String },
    PendingMigration { script: String, description: String, preview: MigrationPreview },
    NoApiKey,
}

#[derive(Debug, Serialize, Clone)]
pub struct MigrationPreview {
    pub tables: Vec<TableMigrationSummary>,
    pub total_rows: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct TableMigrationSummary {
    pub target_table: String,
    pub rows: usize,
    pub skipped: usize,
}

// ─── Tool definitions ─────────────────────────────────────────────────────────

fn migration_tool_definitions() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "mg_list_processes".into(),
            description: "List running processes on this Windows machine. Use to find if the old POS software is currently running.".into(),
            input_schema: json!({"type":"object","properties":{"filter":{"type":"string","description":"Optional name filter (case-insensitive substring)"}},"required":[]}),
        },
        ToolDef {
            name: "mg_find_db_files".into(),
            description: "Search the filesystem for database files (.db, .sqlite, .mdf, .mdb, .fdb). Searches AppData, Program Files, Desktop.".into(),
            input_schema: json!({"type":"object","properties":{"extra_paths":{"type":"array","items":{"type":"string"},"description":"Additional paths to search"}},"required":[]}),
        },
        ToolDef {
            name: "mg_read_text_file".into(),
            description: "Read a text file (config, INI, XML, JSON) to find database connection details.".into(),
            input_schema: json!({"type":"object","properties":{"path":{"type":"string"},"max_chars":{"type":"integer","default":4000}},"required":["path"]}),
        },
        ToolDef {
            name: "mg_connect_test".into(),
            description: "Test connectivity to an external database. Returns success/failure + server version.".into(),
            input_schema: json!({"type":"object","properties":{"db_type":{"type":"string","enum":["sqlite","mysql","mssql","csv"]},"path_or_connstr":{"type":"string"}},"required":["db_type","path_or_connstr"]}),
        },
        ToolDef {
            name: "mg_get_schema".into(),
            description: "Get all table names, column names/types, and row counts from the external database.".into(),
            input_schema: json!({"type":"object","properties":{"db_type":{"type":"string"},"path_or_connstr":{"type":"string"}},"required":["db_type","path_or_connstr"]}),
        },
        ToolDef {
            name: "mg_sample_table".into(),
            description: "Read the first N rows of a table for data preview.".into(),
            input_schema: json!({"type":"object","properties":{"db_type":{"type":"string"},"path_or_connstr":{"type":"string"},"table":{"type":"string"},"limit":{"type":"integer","default":5}},"required":["db_type","path_or_connstr","table"]}),
        },
        ToolDef {
            name: "mg_get_zanpos_schema".into(),
            description: "Get a human-readable description of all ZANPOS target tables and their required fields.".into(),
            input_schema: json!({"type":"object","properties":{},"required":[]}),
        },
        ToolDef {
            name: "mg_execute_migration".into(),
            description: "MUTATION: Execute the migration SQL script inside a transaction against the ZANPOS database. Call ONLY after the user has explicitly typed CONFIRM. The script must contain only INSERT statements. Use ulid_new() as a placeholder where a ULID is needed — it will be replaced with a real ULID at execution time.".into(),
            input_schema: json!({"type":"object","properties":{"script":{"type":"string","description":"Pure SQL INSERT statements; use ulid_new() where a new ULID ID is needed"},"description":{"type":"string","description":"Human-readable summary of what will be migrated"}},"required":["script","description"]}),
        },
    ]
}

// ─── System prompt ────────────────────────────────────────────────────────────

fn build_migration_system_prompt() -> String {
    r#"You are the ZANPOS Migration Agent — a specialized AI assistant with temporary elevated access to help business owners import their existing POS data into ZANPOS.

## Your Persona
You are professional, thorough, and safety-conscious. You work step-by-step and always explain what you're doing.

## 7-Phase Workflow
1. **Discovery** — Find the old POS software (check running processes with mg_list_processes, find DB files with mg_find_db_files)
2. **Connect** — Test connectivity to the database with mg_connect_test
3. **Schema Analysis** — Read the full schema with mg_get_schema; also get ZANPOS target schema with mg_get_zanpos_schema
4. **Mapping** — Map old tables → ZANPOS tables; present the mapping clearly to the user; confirm with user before proceeding
5. **Preview** — Generate the complete migration SQL INSERT script; show counts per table
6. **CONFIRM** — Present the preview and wait for the user to explicitly type "CONFIRM" (all caps); do NOT call mg_execute_migration until CONFIRM is received
7. **Execute** — Call mg_execute_migration with the script only after CONFIRM

## Security Rules (CRITICAL)
1. NEVER execute DELETE, DROP, UPDATE, ALTER, or CREATE on any database
2. External DB: READ ONLY — only SELECT queries and schema inspection
3. ZANPOS DB: INSERT ONLY — never modify existing data
4. NEVER call mg_execute_migration unless the user has explicitly typed "CONFIRM"
5. The script passed to mg_execute_migration must contain ONLY INSERT statements

## ZANPOS Data Formats
- **IDs**: Use `ulid_new()` placeholder in INSERT statements — it gets replaced with a real ULID at execution time
- **Prices**: Store as INTEGER minor units (1.500 BHD = 1500, $9.99 = 999)
- **Dates**: business_date as YYYY-MM-DD, timestamps as ISO 8601 UTC
- **Phone numbers**: E.164 format (+97312345678); skip or normalize if invalid

## Migration Script Format
Write pure SQL INSERT statements. For foreign key consistency, use a subquery approach:
```sql
-- Categories first
INSERT INTO categories (category_id, name, is_active) VALUES (ulid_new(), 'Beverages', 1);
-- Products referencing category above via subquery
INSERT INTO products (product_id, category_id, name, price_minor, is_active)
  SELECT ulid_new(), category_id, 'Coffee', 1500, 1 FROM categories WHERE name = 'Beverages' LIMIT 1;
```

## Error Handling
- If old DB not found: guide user to provide path/connection string manually
- If schema doesn't match ZANPOS: clearly state what can't be migrated and why
- Always show warnings (e.g., "15 phone numbers not in E.164 format — will be skipped")
- Prefer to migrate partial data over failing entirely"#.to_string()
}

// ─── Tool dispatcher ──────────────────────────────────────────────────────────

async fn execute_migration_tool(
    name: &str,
    input: &Value,
    _zanpos_db: &SqlitePool,
    context: &MigrationContext,
) -> String {
    match name {
        "mg_list_processes" => {
            let filter = input
                .get("filter")
                .and_then(|v| v.as_str())
                .map(|s| s.to_lowercase());
            exec_list_processes(filter).await
        }
        "mg_find_db_files" => {
            let extra: Vec<String> = input
                .get("extra_paths")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            exec_find_db_files(extra).await
        }
        "mg_read_text_file" => {
            let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let max_chars = input
                .get("max_chars")
                .and_then(|v| v.as_u64())
                .unwrap_or(4000) as usize;
            exec_read_text_file(path, max_chars)
        }
        "mg_connect_test" => {
            let db_type = input
                .get("db_type")
                .and_then(|v| v.as_str())
                .or(context.db_type.as_deref())
                .unwrap_or("");
            let conn = input
                .get("path_or_connstr")
                .and_then(|v| v.as_str())
                .or(context.path_or_connstr.as_deref())
                .unwrap_or("");
            exec_connect_test(db_type, conn).await
        }
        "mg_get_schema" => {
            let db_type = input
                .get("db_type")
                .and_then(|v| v.as_str())
                .or(context.db_type.as_deref())
                .unwrap_or("");
            let conn = input
                .get("path_or_connstr")
                .and_then(|v| v.as_str())
                .or(context.path_or_connstr.as_deref())
                .or(context.attached_file_path.as_deref())
                .unwrap_or("");
            exec_get_schema(db_type, conn).await
        }
        "mg_sample_table" => {
            let db_type = input
                .get("db_type")
                .and_then(|v| v.as_str())
                .or(context.db_type.as_deref())
                .unwrap_or("");
            let conn = input
                .get("path_or_connstr")
                .and_then(|v| v.as_str())
                .or(context.path_or_connstr.as_deref())
                .unwrap_or("");
            let table = input.get("table").and_then(|v| v.as_str()).unwrap_or("");
            let limit = input
                .get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(5) as usize;
            exec_sample_table(db_type, conn, table, limit).await
        }
        "mg_get_zanpos_schema" => exec_get_zanpos_schema(),
        _ => format!("Unknown tool: {name}"),
    }
}

// ─── Tool implementations ─────────────────────────────────────────────────────

async fn exec_list_processes(filter: Option<String>) -> String {
    let result = tokio::task::spawn_blocking(move || {
        std::process::Command::new("tasklist")
            .args(["/FO", "CSV"])
            .output()
    })
    .await;

    match result {
        Ok(Ok(output)) => {
            let raw = String::from_utf8_lossy(&output.stdout).to_string();
            let lines: Vec<&str> = raw.lines().collect();
            let filtered: Vec<&str> = if let Some(ref f) = filter {
                lines
                    .iter()
                    .filter(|l| l.to_lowercase().contains(f.as_str()))
                    .cloned()
                    .collect()
            } else {
                lines.iter().take(50).cloned().collect()
            };
            if filtered.is_empty() {
                "No matching processes found.".to_string()
            } else {
                format!(
                    "Running processes{}:\n{}",
                    filter
                        .map(|f| format!(" (filter: '{}')", f))
                        .unwrap_or_default(),
                    filtered.join("\n")
                )
            }
        }
        _ => "Could not list processes — tasklist unavailable.".to_string(),
    }
}

async fn exec_find_db_files(extra_paths: Vec<String>) -> String {
    tokio::task::spawn_blocking(move || {
        let extensions = ["db", "sqlite", "sqlite3", "mdf", "mdb", "fdb", "accdb"];

        let mut search_roots: Vec<PathBuf> = Vec::new();

        if let Ok(appdata) = std::env::var("APPDATA") {
            search_roots.push(PathBuf::from(appdata));
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            search_roots.push(PathBuf::from(local));
        }
        if let Ok(userprofile) = std::env::var("USERPROFILE") {
            let p = PathBuf::from(&userprofile);
            search_roots.push(p.join("Desktop"));
            search_roots.push(p.join("Documents"));
        }
        search_roots.push(PathBuf::from("C:\\Program Files"));
        search_roots.push(PathBuf::from("C:\\Program Files (x86)"));

        for ep in &extra_paths {
            search_roots.push(PathBuf::from(ep));
        }

        let mut found: Vec<String> = Vec::new();
        for root in &search_roots {
            walk_for_db_files(root, &extensions, 4, &mut found);
            if found.len() > 100 {
                break;
            }
        }

        if found.is_empty() {
            "No database files found in common locations. Try providing the path directly."
                .to_string()
        } else {
            format!("Found {} database file(s):\n{}", found.len(), found.join("\n"))
        }
    })
    .await
    .unwrap_or_else(|_| "File search failed.".to_string())
}

fn walk_for_db_files(dir: &Path, exts: &[&str], max_depth: u32, results: &mut Vec<String>) {
    if max_depth == 0 || results.len() >= 100 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_for_db_files(&path, exts, max_depth - 1, results);
        } else if let Some(ext) = path.extension() {
            let ext_lc = ext.to_string_lossy().to_lowercase();
            if exts.contains(&ext_lc.as_str()) {
                let size = path.metadata().map(|m| m.len()).unwrap_or(0);
                results.push(format!("{} ({} KB)", path.display(), size / 1024));
            }
        }
    }
}

fn exec_read_text_file(path: &str, max_chars: usize) -> String {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            if content.len() > max_chars {
                format!(
                    "{}\n[...truncated at {} chars]",
                    &content[..max_chars],
                    max_chars
                )
            } else {
                content
            }
        }
        Err(e) => format!("Could not read file '{}': {}", path, e),
    }
}

async fn exec_connect_test(db_type: &str, conn: &str) -> String {
    if conn.is_empty() {
        return "No connection string provided.".to_string();
    }
    match db_type {
        "sqlite" => {
            let path = conn.to_string();
            match tokio::task::spawn_blocking(move || {
                std::fs::metadata(&path)
                    .map(|m| format!("SQLite file OK — {} KB", m.len() / 1024))
                    .map_err(|e| e.to_string())
            })
            .await
            {
                Ok(Ok(msg)) => msg,
                Ok(Err(e)) => format!("SQLite error: {}", e),
                Err(_) => "SQLite check failed.".to_string(),
            }
        }
        "mysql" => {
            let conn = conn.to_string();
            match tokio::task::spawn_blocking(move || {
                use mysql::prelude::Queryable;
                let pool = mysql::Pool::new(conn.as_str())?;
                let mut c = pool.get_conn()?;
                let version: Option<String> = c.query_first("SELECT VERSION()")?;
                Ok::<_, mysql::Error>(format!(
                    "MySQL {} — connected OK",
                    version.unwrap_or_default()
                ))
            })
            .await
            {
                Ok(Ok(msg)) => msg,
                Ok(Err(e)) => format!("MySQL error: {}", e),
                Err(_) => "MySQL check panicked.".to_string(),
            }
        }
        "mssql" => {
            let conn = conn.to_string();
            match exec_mssql_connect_test(&conn).await {
                Ok(msg) => msg,
                Err(e) => format!("SQL Server error: {}", e),
            }
        }
        "csv" => {
            let path = conn.to_string();
            match tokio::task::spawn_blocking(move || {
                if path.to_lowercase().ends_with(".csv") {
                    let mut rdr = csv::ReaderBuilder::new()
                        .has_headers(true)
                        .from_path(&path)?;
                    let headers = rdr.headers()?.clone();
                    let count = rdr.records().count();
                    Ok::<_, Box<dyn std::error::Error + Send + Sync>>(format!(
                        "CSV file OK — {} columns, {} data rows. Headers: {}",
                        headers.len(),
                        count,
                        headers.iter().collect::<Vec<_>>().join(", ")
                    ))
                } else {
                    use calamine::Reader;
                    let mut wb = calamine::open_workbook_auto(&path)
                        .map_err(|e| format!("Could not open Excel: {}", e))?;
                    let sheets = wb.sheet_names().to_vec();
                    Ok(format!(
                        "Excel file OK — {} sheet(s): {}",
                        sheets.len(),
                        sheets.join(", ")
                    ))
                }
            })
            .await
            {
                Ok(Ok(msg)) => msg,
                Ok(Err(e)) => format!("CSV/Excel error: {}", e),
                Err(_) => "CSV/Excel check failed.".to_string(),
            }
        }
        _ => format!(
            "Unknown db_type: '{}'. Use sqlite, mysql, mssql, or csv.",
            db_type
        ),
    }
}

async fn exec_mssql_connect_test(conn_str: &str) -> Result<String, String> {
    use tokio_util::compat::TokioAsyncWriteCompatExt;
    let config =
        tiberius::Config::from_ado_string(conn_str).map_err(|e| e.to_string())?;
    let tcp = tokio::net::TcpStream::connect(config.get_addr())
        .await
        .map_err(|e| e.to_string())?;
    tcp.set_nodelay(true).map_err(|e| e.to_string())?;
    let mut client = tiberius::Client::connect(config, tcp.compat_write())
        .await
        .map_err(|e| e.to_string())?;
    let row = client
        .query("SELECT @@VERSION", &[])
        .await
        .map_err(|e| e.to_string())?
        .into_first_result()
        .await
        .map_err(|e| e.to_string())?;
    let version = row
        .first()
        .and_then(|r| r.get::<&str, _>(0))
        .unwrap_or("(unknown)")
        .lines()
        .next()
        .unwrap_or("")
        .to_string();
    Ok(format!("SQL Server — {}", version))
}

async fn exec_get_schema(db_type: &str, conn: &str) -> String {
    match db_type {
        "sqlite" => exec_sqlite_schema(conn).await,
        "mysql" => exec_mysql_schema(conn).await,
        "mssql" => exec_mssql_schema(conn).await,
        "csv" => exec_csv_schema(conn).await,
        _ => format!("Unknown db_type: '{}'", db_type),
    }
}

async fn exec_sqlite_schema(path: &str) -> String {
    if path.is_empty() {
        return "No path provided.".to_string();
    }
    let url = format!("sqlite:{}?mode=ro", path);
    match sqlx::SqlitePool::connect(&url).await {
        Err(e) => format!("Could not open SQLite DB: {}", e),
        Ok(pool) => {
            let tables: Vec<String> = match sqlx::query_scalar(
                "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name",
            )
            .fetch_all(&pool)
            .await
            {
                Ok(t) => t,
                Err(e) => return format!("Could not list tables: {}", e),
            };

            let mut result = format!("SQLite database: {} tables\n\n", tables.len());
            for table in &tables {
                let cols: Vec<(String, String)> = match sqlx::query_as::<_, (String, String)>(
                    &format!("SELECT name, type FROM pragma_table_info('{}')", table),
                )
                .fetch_all(&pool)
                .await
                {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let count: i64 =
                    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM \"{}\"", table))
                        .fetch_one(&pool)
                        .await
                        .unwrap_or(0);

                result.push_str(&format!("Table: {} ({} rows)\n", table, count));
                for (col_name, col_type) in &cols {
                    result.push_str(&format!("   - {} ({})\n", col_name, col_type));
                }
                result.push('\n');
            }
            pool.close().await;
            result
        }
    }
}

async fn exec_mysql_schema(conn: &str) -> String {
    let conn = conn.to_string();
    let result = tokio::task::spawn_blocking(move || {
        use mysql::prelude::Queryable;
        let pool = mysql::Pool::new(conn.as_str())?;
        let mut c = pool.get_conn()?;
        let db_name: Option<String> = c.query_first("SELECT DATABASE()")?;
        let db_name = db_name.unwrap_or_default();

        let tables: Vec<String> = c.query(format!(
            "SELECT table_name FROM information_schema.tables WHERE table_schema = '{}'",
            db_name
        ))?;

        let mut result = format!(
            "MySQL database '{}': {} tables\n\n",
            db_name,
            tables.len()
        );
        for table in &tables {
            let cols: Vec<(String, String)> = c.query(format!(
                "SELECT column_name, column_type FROM information_schema.columns \
                 WHERE table_schema = '{}' AND table_name = '{}' ORDER BY ordinal_position",
                db_name, table
            ))?;
            let count: Option<i64> = c
                .query_first(format!("SELECT COUNT(*) FROM `{}`", table))
                .ok()
                .flatten();
            result.push_str(&format!(
                "Table: {} ({} rows)\n",
                table,
                count.unwrap_or(0)
            ));
            for (col_name, col_type) in &cols {
                result.push_str(&format!("   - {} ({})\n", col_name, col_type));
            }
            result.push('\n');
        }
        Ok::<_, mysql::Error>(result)
    })
    .await;

    match result {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => format!("MySQL error: {}", e),
        Err(_) => "MySQL schema read panicked.".to_string(),
    }
}

async fn exec_mssql_schema(conn_str: &str) -> String {
    use tokio_util::compat::TokioAsyncWriteCompatExt;
    let config = match tiberius::Config::from_ado_string(conn_str) {
        Ok(c) => c,
        Err(e) => return format!("Invalid connection string: {}", e),
    };
    let tcp = match tokio::net::TcpStream::connect(config.get_addr()).await {
        Ok(t) => t,
        Err(e) => return format!("Could not connect: {}", e),
    };
    let _ = tcp.set_nodelay(true);
    let mut client = match tiberius::Client::connect(config, tcp.compat_write()).await {
        Ok(c) => c,
        Err(e) => return format!("SQL Server auth failed: {}", e),
    };

    let query = "SELECT t.name, c.name, tp.name, c.is_nullable \
                 FROM sys.tables t \
                 JOIN sys.columns c ON t.object_id = c.object_id \
                 JOIN sys.types tp ON c.user_type_id = tp.user_type_id \
                 ORDER BY t.name, c.column_id";

    let rows = match client.query(query, &[]).await {
        Ok(r) => r,
        Err(e) => return format!("Query failed: {}", e),
    };
    let rows = match rows.into_first_result().await {
        Ok(r) => r,
        Err(e) => return format!("Result failed: {}", e),
    };

    let mut tables: HashMap<String, Vec<String>> = HashMap::new();
    for row in &rows {
        let table: &str = row.get(0).unwrap_or("");
        let col: &str = row.get(1).unwrap_or("");
        let typ: &str = row.get(2).unwrap_or("");
        tables
            .entry(table.to_string())
            .or_default()
            .push(format!("{} ({})", col, typ));
    }

    let mut result = format!("SQL Server: {} tables\n\n", tables.len());
    for (table, cols) in &tables {
        result.push_str(&format!("Table: {}\n", table));
        for col in cols {
            result.push_str(&format!("   - {}\n", col));
        }
        result.push('\n');
    }
    result
}

async fn exec_csv_schema(path: &str) -> String {
    let path = path.to_string();
    let result = tokio::task::spawn_blocking(move || {
        if path.to_lowercase().ends_with(".csv") {
            let mut rdr = csv::ReaderBuilder::new()
                .has_headers(true)
                .from_path(&path)
                .map_err(|e| e.to_string())?;
            let headers = rdr.headers().map_err(|e| e.to_string())?.clone();
            let count = rdr.records().count();
            Ok::<_, String>(format!(
                "CSV file: {} column(s), {} data row(s)\nColumns: {}\n\n\
                 (Treat this as a single flat table — map columns to ZANPOS fields manually)",
                headers.len(),
                count,
                headers.iter().collect::<Vec<_>>().join(", ")
            ))
        } else {
            use calamine::Reader;
            let mut wb = calamine::open_workbook_auto(&path).map_err(|e| e.to_string())?;
            let sheets = wb.sheet_names().to_vec();
            let mut result = format!("Excel file: {} sheet(s)\n\n", sheets.len());
            for sheet_name in &sheets {
                if let Ok(range) = wb.worksheet_range(sheet_name) {
                    let rows = range.rows().collect::<Vec<_>>();
                    if let Some(header_row) = rows.first() {
                        let headers: Vec<String> =
                            header_row.iter().map(|c| c.to_string()).collect();
                        result.push_str(&format!(
                            "Sheet '{}': {} columns, {} data rows\nColumns: {}\n\n",
                            sheet_name,
                            headers.len(),
                            rows.len().saturating_sub(1),
                            headers.join(", ")
                        ));
                    }
                }
            }
            Ok(result)
        }
    })
    .await;

    match result {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => format!("CSV/Excel error: {}", e),
        Err(_) => "CSV/Excel schema read failed.".to_string(),
    }
}

async fn exec_sample_table(db_type: &str, conn: &str, table: &str, limit: usize) -> String {
    match db_type {
        "sqlite" => {
            let url = format!("sqlite:{}?mode=ro", conn);
            match sqlx::SqlitePool::connect(&url).await {
                Err(e) => format!("Could not open SQLite: {}", e),
                Ok(pool) => {
                    let query = format!("SELECT * FROM \"{}\" LIMIT {}", table, limit);
                    match sqlx::query(&query).fetch_all(&pool).await {
                        Ok(rows) => {
                            use sqlx::Row;
                            if rows.is_empty() {
                                return "Table is empty.".to_string();
                            }
                            let mut out =
                                format!("First {} rows of '{}':\n", rows.len(), table);
                            for row in &rows {
                                let cols: Vec<String> = (0..row.len())
                                    .map(|i| {
                                        row.try_get::<String, _>(i)
                                            .or_else(|_| {
                                                row.try_get::<i64, _>(i).map(|v| v.to_string())
                                            })
                                            .or_else(|_| {
                                                row.try_get::<f64, _>(i).map(|v| v.to_string())
                                            })
                                            .unwrap_or_else(|_| "NULL".to_string())
                                    })
                                    .collect();
                                out.push_str(&format!("  {}\n", cols.join(" | ")));
                            }
                            pool.close().await;
                            out
                        }
                        Err(e) => format!("Query error: {}", e),
                    }
                }
            }
        }
        "mysql" => {
            let conn = conn.to_string();
            let table = table.to_string();
            let result = tokio::task::spawn_blocking(move || {
                use mysql::prelude::Queryable;
                let pool = mysql::Pool::new(conn.as_str())?;
                let mut c = pool.get_conn()?;
                let rows: Vec<mysql::Row> =
                    c.query(format!("SELECT * FROM `{}` LIMIT {}", table, limit))?;
                let mut out = format!("First {} rows of '{}':\n", rows.len(), table);
                for row in &rows {
                    let values: Vec<String> = (0..row.len())
                        .map(|i| {
                            row.get::<String, _>(i)
                                .unwrap_or_else(|| "NULL".to_string())
                        })
                        .collect();
                    out.push_str(&format!("  {}\n", values.join(" | ")));
                }
                Ok::<_, mysql::Error>(out)
            })
            .await;
            match result {
                Ok(Ok(s)) => s,
                Ok(Err(e)) => format!("MySQL error: {}", e),
                Err(_) => "MySQL sample failed.".to_string(),
            }
        }
        _ => format!("Sample not implemented for db_type '{}'", db_type),
    }
}

fn exec_get_zanpos_schema() -> String {
    r#"ZANPOS target tables and required fields:

PRODUCTS
  Required: product_id (ULID), category_id (ULID -> categories.category_id), name (TEXT), price_minor (INTEGER, e.g. 1500 = BHD 1.500)
  Optional: barcode (TEXT), sku (TEXT), cost_minor (INTEGER), track_inventory (0/1, default 0), is_active (0/1, default 1)

CATEGORIES
  Required: category_id (ULID), name (TEXT), is_active (0/1, default 1)

CUSTOMERS
  Required: customer_id (ULID), name (TEXT)
  Optional: phone (E.164 format e.g. +97312345678), email (TEXT), loyalty_points (INTEGER, default 0), notes (TEXT)

SALES
  Required: sale_id (ULID), receipt_number (TEXT, unique), branch_id (TEXT), shift_id (TEXT), cashier_user_id (TEXT), status ('completed'), net_total_minor (INTEGER), business_date (YYYY-MM-DD), sold_at (UTC ISO8601)
  Note: For historical imports, use shift_id = 'IMPORT_SHIFT' and cashier_user_id = the admin user_id

SALE_ITEMS
  Required: sale_item_id (ULID), sale_id (ULID -> sales.sale_id), product_id (ULID), product_name (TEXT), qty (TEXT decimal e.g. '1'), unit_price_minor (INTEGER), line_total_minor (INTEGER)

PAYMENTS
  Required: payment_id (ULID), sale_id (ULID -> sales.sale_id), method ('cash'/'card'/'wallet'), amount_minor (INTEGER)

STOCK_LEVELS
  Required: stock_level_id (ULID), product_id (ULID -> products.product_id), branch_id (TEXT), quantity_on_hand (TEXT decimal e.g. '10.00')

IMPORTANT NOTES:
- Insert categories BEFORE products (foreign key dependency)
- Insert products BEFORE sale_items and stock_levels
- Insert sales BEFORE sale_items and payments
- Get active branch_id with: SELECT branch_id FROM branches WHERE is_active = 1 LIMIT 1
- Get admin user_id with: SELECT user_id FROM users WHERE role = 'owner' LIMIT 1
- Use ulid_new() for every new ULID — it gets replaced at execution time
"#
    .to_string()
}

// ─── Migration preview parser ─────────────────────────────────────────────────

fn parse_migration_preview(script: &str) -> MigrationPreview {
    let mut table_counts: HashMap<String, usize> = HashMap::new();
    let mut warnings: Vec<String> = Vec::new();

    for line in script.lines() {
        let line = line.trim();
        if line.to_uppercase().starts_with("INSERT INTO") {
            let parts: Vec<&str> = line.splitn(4, ' ').collect();
            if parts.len() >= 3 {
                let table = parts[2]
                    .trim_matches(|c| c == '"' || c == '`' || c == '[' || c == ']');
                *table_counts.entry(table.to_string()).or_insert(0) += 1;
            }
        }
    }

    if script.contains("-- WARNING:") || script.contains("-- SKIPPED:") {
        warnings.push("Some rows were skipped — see comments in script.".to_string());
    }

    let tables: Vec<TableMigrationSummary> = table_counts
        .into_iter()
        .map(|(target_table, rows)| TableMigrationSummary {
            target_table,
            rows,
            skipped: 0,
        })
        .collect();
    let total_rows = tables.iter().map(|t| t.rows).sum();

    MigrationPreview {
        tables,
        total_rows,
        warnings,
    }
}

// ─── ULID replacement ─────────────────────────────────────────────────────────

fn replace_ulid_placeholders(script: &str) -> String {
    let mut result = script.to_string();
    while result.contains("ulid_new()") {
        let new_id = Ulid::new().to_string();
        result = result.replacen("ulid_new()", &format!("'{}'", new_id), 1);
    }
    result
}

// ─── Migration execution ──────────────────────────────────────────────────────

async fn exec_execute_migration_inner(pool: &SqlitePool, script: &str) -> AppResult<String> {
    // 1. Validate: only INSERT/SELECT/WITH/-- allowed
    let cleaned = script
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n");

    for stmt in cleaned.split(';') {
        let trimmed = stmt.trim();
        if trimmed.is_empty() {
            continue;
        }
        let upper = trimmed.to_uppercase();
        let ok = upper.starts_with("INSERT")
            || upper.starts_with("SELECT")
            || upper.starts_with("WITH");
        if !ok {
            return Err(AppError::Validation(format!(
                "Rejected: non-INSERT statement: {}",
                &trimmed[..trimmed.len().min(80)]
            )));
        }
    }

    // 2. Replace ulid_new() placeholders
    let processed = replace_ulid_placeholders(script);

    // 3. Run in transaction
    let mut tx = pool.begin().await?;
    let mut table_counts: HashMap<String, usize> = HashMap::new();
    let mut total = 0usize;

    for stmt in processed.split(';') {
        let stmt = stmt.trim();
        if stmt.is_empty() || stmt.starts_with("--") {
            continue;
        }

        let upper = stmt.to_uppercase();
        let table_name = if upper.starts_with("INSERT INTO") {
            stmt.split_whitespace()
                .nth(2)
                .map(|t| {
                    t.trim_matches(|c: char| !c.is_alphanumeric() && c != '_')
                        .to_lowercase()
                })
                .unwrap_or_else(|| "unknown".to_string())
        } else {
            "other".to_string()
        };

        sqlx::query(stmt)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                AppError::Validation(format!(
                    "SQL error in statement '{}...': {}",
                    &stmt[..stmt.len().min(60)],
                    e
                ))
            })?;

        *table_counts.entry(table_name).or_insert(0) += 1;
        total += 1;
    }

    tx.commit().await?;

    let summary: Vec<String> = table_counts
        .iter()
        .map(|(t, n)| format!("  - {} — {} rows", t, n))
        .collect();

    Ok(format!(
        "Migration complete! {} statements executed.\n\n{}\n\nYou can now close this window and start using ZANPOS.",
        total,
        summary.join("\n")
    ))
}

// ─── Main multi-turn loop ─────────────────────────────────────────────────────

async fn run_migration_loop(
    provider: &Provider,
    system: &str,
    history: &[ChatMessage],
    user_message: &str,
    tool_defs: &[ToolDef],
    zanpos_db: &SqlitePool,
    context: &MigrationContext,
) -> AppResult<MigrationChatResponse> {
    match provider {
        Provider::Anthropic(client) => {
            let mut msgs: Vec<AnthropicMessage> = history
                .iter()
                .map(|m| {
                    if m.role == "user" {
                        AnthropicMessage::user_text(&m.content)
                    } else {
                        AnthropicMessage::assistant_text(&m.content)
                    }
                })
                .collect();
            msgs.push(AnthropicMessage::user_text(user_message));

            for _ in 0..12 {
                let resp = client.send(system, msgs.clone(), tool_defs.to_vec()).await?;

                if let Some((id, name, input)) = extract_tool_use(&resp.content) {
                    if name == "mg_execute_migration" {
                        let script = input
                            .get("script")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let description = input
                            .get("description")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let preview = parse_migration_preview(&script);
                        return Ok(MigrationChatResponse::PendingMigration {
                            script,
                            description,
                            preview,
                        });
                    }

                    let tool_result =
                        execute_migration_tool(&name, &input, zanpos_db, context).await;

                    msgs.push(AnthropicMessage {
                        role: "assistant".into(),
                        content: resp.content.clone(),
                    });
                    msgs.push(AnthropicMessage {
                        role: "user".into(),
                        content: vec![ContentBlock::ToolResult {
                            tool_use_id: id,
                            content: tool_result,
                        }],
                    });
                } else {
                    return Ok(MigrationChatResponse::Message {
                        content: extract_text(&resp.content),
                    });
                }
            }
            Ok(MigrationChatResponse::Message {
                content: "The migration agent reached the maximum number of steps. Please continue the conversation.".into(),
            })
        }

        Provider::OpenAI(client) => {
            let mut msgs: Vec<OpenAIMessage> = history
                .iter()
                .map(|m| {
                    if m.role == "user" {
                        user_msg(&m.content)
                    } else {
                        assistant_msg(&m.content)
                    }
                })
                .collect();
            msgs.push(user_msg(user_message));

            for _ in 0..12 {
                let resp = client.send(system, msgs.clone(), tool_defs).await?;

                if let Some(tc) = resp.tool_call {
                    if tc.name == "mg_execute_migration" {
                        let script = tc
                            .input
                            .get("script")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let description = tc
                            .input
                            .get("description")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let preview = parse_migration_preview(&script);
                        return Ok(MigrationChatResponse::PendingMigration {
                            script,
                            description,
                            preview,
                        });
                    }

                    let tool_result =
                        execute_migration_tool(&tc.name, &tc.input, zanpos_db, context).await;

                    msgs.push(assistant_tool_call_msg(
                        if resp.text.is_empty() {
                            None
                        } else {
                            Some(resp.text)
                        },
                        &OpenAIToolCallResult {
                            id: tc.id.clone(),
                            name: tc.name.clone(),
                            input: tc.input.clone(),
                        },
                    ));
                    msgs.push(tool_result_msg(&tc.id, tool_result));
                } else {
                    return Ok(MigrationChatResponse::Message { content: resp.text });
                }
            }
            Ok(MigrationChatResponse::Message {
                content: "The migration agent reached the maximum number of steps.".into(),
            })
        }
    }
}

// ─── Tauri commands ───────────────────────────────────────────────────────────

#[tauri::command]
pub async fn migration_agent_chat(
    input: MigrationChatInput,
    state: State<'_, AppState>,
) -> AppResult<MigrationChatResponse> {
    let provider = match Provider::from_db(&state.db).await? {
        Some(p) => p,
        None => return Ok(MigrationChatResponse::NoApiKey),
    };

    let system = build_migration_system_prompt();
    let tool_defs = migration_tool_definitions();

    run_migration_loop(
        &provider,
        &system,
        &input.history,
        &input.message,
        &tool_defs,
        &state.db,
        &input.context,
    )
    .await
}

#[tauri::command]
pub async fn migration_confirm_execute(
    script: String,
    user_id: String,
    state: State<'_, AppState>,
) -> AppResult<String> {
    crate::commands::rbac::manager_or_owner(&state.db, &user_id).await?;
    exec_execute_migration_inner(&state.db, &script).await
}
