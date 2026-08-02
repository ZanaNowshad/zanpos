use sqlx::SqlitePool;

async fn audit_examples(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // ruleid: zanpos-discarded-audit-security-result
    let _ = sqlx::query("INSERT INTO audit_logs (event_type) VALUES ('fixture')")
        .execute(pool)
        .await;

    // ruleid: zanpos-discarded-audit-security-result
    sqlx::query("INSERT INTO audit_logs (event_type) VALUES ('fixture')")
        .execute(pool)
        .await
        .ok();

    // ruleid: zanpos-discarded-audit-security-result
    if let Err(error) = sqlx::query("INSERT INTO audit_logs (event_type) VALUES ('fixture')")
        .execute(pool)
        .await
    {
        tracing::error!("discarded audit failure: {error}");
    }

    // ok: zanpos-discarded-audit-security-result
    sqlx::query("INSERT INTO audit_logs (event_type) VALUES ('fixture')")
        .execute(pool)
        .await?;
    Ok(())
}

async fn audit_ext(pool: &SqlitePool) {
    // ok: zanpos-discarded-audit-security-result
    if let Err(error) = sqlx::query("INSERT INTO audit_logs (event_type) VALUES ('baseline')")
        .execute(pool)
        .await
    {
        tracing::error!("deferred executor baseline: {error}");
    }
}

async fn raw_sql_examples(pool: &SqlitePool, unsafe_clause: &str) -> Result<(), sqlx::Error> {
    // ruleid: zanpos-no-dynamic-sql-format
    sqlx::query(&format!("DELETE FROM products WHERE {}", unsafe_clause))
        .execute(pool)
        .await?;

    // ok: zanpos-no-dynamic-sql-format
    sqlx::query("DELETE FROM products WHERE product_id = ?")
        .bind("fixture-id")
        .execute(pool)
        .await?;
    Ok(())
}

async fn raw_sql_scalar_bypass(pool: &SqlitePool, renamed: &str) {
    let sql = format!("SELECT {} FROM products", renamed);
    // ruleid: zanpos-no-dynamic-sql-format
    sqlx::query_scalar(&sql).fetch_optional(pool).await;
}

async fn raw_sql_owned_string_bypass(pool: &SqlitePool, renamed: String) {
    let sql = format!("SELECT {} FROM products", renamed);
    // ruleid: zanpos-no-dynamic-sql-format
    sqlx::query_as::<_, (String,)>(&sql).fetch_optional(pool).await;
}

async fn raw_sql_owned_inline(pool: &SqlitePool, renamed: String) {
    // ruleid: zanpos-no-dynamic-sql-format
    sqlx::query(&format!("SELECT {} FROM products", renamed))
        .execute(pool)
        .await;
}

struct RawSqlRequest {
    table_name: String,
}

async fn raw_sql_struct_field(pool: &SqlitePool, input: RawSqlRequest) {
    // ruleid: zanpos-no-dynamic-sql-format
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}", input.table_name))
        .fetch_optional(pool)
        .await;
}

fn raw_sql_named_capture(pool: &SqlitePool, unsafe_clause: String) {
    let sql = format!("SELECT name FROM products WHERE {clause}", clause = unsafe_clause);
    // nosemgrep: zanpos-no-dynamic-sql-format -- Semgrep CE 1.172 does not propagate named format arguments; owner ZANPOS Maintainers; review 2027-01-31
    sqlx::query_as::<sqlx::Sqlite, (String,)>(&sql);
}

fn raw_sql_generic<T>(pool: &SqlitePool, unsafe_table: T) {
    // ruleid: zanpos-no-dynamic-sql-format
    sqlx::query::<sqlx::Sqlite>(&format!("SELECT * FROM {}", unsafe_table));
}

fn raw_sql_request_builder(pool: &SqlitePool, input: RawSqlRequest) {
    let mut sql = "SELECT * FROM ".to_string();
    sql.push_str(&input.table_name);
    // ruleid: zanpos-no-dynamic-sql-format
    sqlx::query(&sql);
}

fn raw_sql_environment(pool: &SqlitePool) {
    let table = std::env::var("ZANPOS_FIXTURE_TABLE").unwrap_or_default();
    let sql = format!("SELECT * FROM {}", table);
    // ruleid: zanpos-no-dynamic-sql-format
    sqlx::query_scalar::<sqlx::Sqlite, i64>(&sql);
}

async fn raw_sql_database_builder(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    use sqlx::Row;
    let row = sqlx::query("SELECT name FROM fixture_metadata")
        .fetch_one(pool)
        .await?;
    // ruleid: zanpos-no-db-derived-sql-format
    let table: String = row.try_get("name")?;
    let mut sql = "SELECT * FROM ".to_string();
    sql.push_str(&table);
    sqlx::query_as::<sqlx::Sqlite, (String,)>(&sql);
    Ok(())
}

async fn raw_sql_database_unrelated(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    use sqlx::Row;
    let row = sqlx::query("SELECT name FROM fixture_metadata")
        .fetch_one(pool)
        .await?;
    let _unrelated: String = row.try_get("name")?;
    let internal_table = "products";
    let sql = format!("SELECT * FROM {}", internal_table);
    // ok: zanpos-no-db-derived-sql-format
    sqlx::query_as::<sqlx::Sqlite, (String,)>(&sql);
    Ok(())
}

fn raw_sql_ignored_validation(pool: &SqlitePool, unsafe_table: String) {
    let _ = is_safe_sql_identifier(&unsafe_table);
    let sql = format!("SELECT * FROM {}", unsafe_table);
    // ruleid: zanpos-no-dynamic-sql-format
    sqlx::query(&sql);
}

fn raw_sql_guarded_identifier(pool: &SqlitePool, table: String) {
    if !is_safe_sql_identifier(&table) {
        return;
    }
    // nosemgrep: zanpos-no-dynamic-sql-format -- explicit reject-and-return identifier guard; owner ZANPOS Maintainers; review 2027-01-31
    // ok: zanpos-no-dynamic-sql-format
    sqlx::query(&format!("SELECT * FROM {table}"));
}

fn is_safe_sql_identifier(value: &str) -> bool {
    !value.is_empty()
}

async fn raw_sql_owned_string_safe(pool: &SqlitePool, _renamed: String) {
    // ok: zanpos-no-dynamic-sql-format
    sqlx::query_as::<_, (String,)>("SELECT name FROM products").fetch_optional(pool).await;
}

async fn raw_sql_unrelated_owned_and_internal_format(pool: &SqlitePool, _request: String) {
    let internal_table = "products";
    let sql = format!("SELECT name FROM {internal_table}");
    // ok: zanpos-no-dynamic-sql-format
    sqlx::query_as::<_, (String,)>(&sql).fetch_optional(pool).await;
}

async fn raw_sql_allowlisted(pool: &SqlitePool) {
    let table = "products";
    // ok: zanpos-no-dynamic-sql-format
    sqlx::query_scalar(format!("SELECT COUNT(*) FROM {table}")).fetch_optional(pool).await;
}

fn secret_log_examples(api_token: &str) {
    // ruleid: zanpos-no-secret-in-log-message
    tracing::warn!("request token: {api_token}");
    // ok: zanpos-no-secret-in-log-message
    tracing::warn!("request completed");
}

fn wildcard_network_fixture() {
    // ruleid: zanpos-no-wildcard-network-endpoint
    let _endpoint = "http://0.0.0.0:8080";
    // ok: zanpos-no-wildcard-network-endpoint
    let _loopback = "http://127.0.0.1:8080";
}

fn wildcard_listener_fixture() {
    // ruleid: zanpos-no-wildcard-listener-bind
    std::net::TcpListener::bind(std::net::SocketAddr::from(([0, 0, 0, 0], 8080)));
    // ok: zanpos-no-wildcard-listener-bind
    std::net::TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 8080)));
}

fn wildcard_listener_unspecified() {
    // ruleid: zanpos-no-wildcard-listener-bind
    std::net::TcpListener::bind(std::net::Ipv4Addr::UNSPECIFIED);
    // ruleid: zanpos-no-wildcard-listener-bind
    std::net::TcpListener::bind(std::net::SocketAddrV4::new(
        std::net::Ipv4Addr::UNSPECIFIED,
        8080,
    ));
    // ruleid: zanpos-no-wildcard-listener-bind, zanpos-no-wildcard-network-endpoint
    std::net::UdpSocket::bind("0.0.0.0:8080");
    // ok: zanpos-no-wildcard-listener-bind
    std::net::UdpSocket::bind("127.0.0.1:8080");
}

async fn wildcard_listener_derived(port: u16) {
    let wildcard_host = "0.0.0.0";
    // ruleid: zanpos-no-wildcard-listener-bind
    tokio::net::TcpListener::bind((wildcard_host, port)).await;

    let formatted = format!("{}:{port}", "0.0.0.0");
    // ruleid: zanpos-no-wildcard-listener-bind
    tokio::net::UdpSocket::bind(formatted).await;

    let address: std::net::SocketAddr =
        std::net::SocketAddrV4::new(std::net::Ipv4Addr::UNSPECIFIED, port).into();
    let socket = tokio::net::TcpSocket::new_v4().unwrap();
    // ruleid: zanpos-no-wildcard-listener-bind
    socket.bind(address);

    let loopback_host = "127.0.0.1";
    // ok: zanpos-no-wildcard-listener-bind
    tokio::net::TcpListener::bind((loopback_host, port)).await;
}

async fn wildcard_listener_imported_unspecified(port: u16) {
    use std::net::{Ipv4Addr, SocketAddrV4};
    let address = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port);
    // ruleid: zanpos-no-wildcard-listener-bind
    tokio::net::UdpSocket::bind(address).await;
}

pub async fn start_hub(pool: SqlitePool, port: u16) {
    let address = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    // ok: zanpos-no-wildcard-listener-bind
    tokio::net::TcpListener::bind(address).await;
}

pub fn lan_ips() -> Vec<String> {
    // ok: zanpos-no-wildcard-listener-bind
    std::net::UdpSocket::bind("0.0.0.0:0");
    Vec::new()
}

fn shorthand_secret_log(api_token: &str) {
    // ruleid: zanpos-no-secret-log-shorthand
    tracing::debug!(?api_token, "request failed");
    // ok: zanpos-no-secret-log-shorthand
    tracing::debug!(token_redacted = "[REDACTED]", "request failed");
}

fn log_examples(request_token: &str) {
    // ruleid: zanpos-no-secret-in-log-fields
    tracing::info!(request_token = request_token, "request completed");

    // ok: zanpos-no-secret-in-log-fields
    tracing::info!(request_state = "completed", "request completed");
}

#[tauri::command]
async fn raw_ai_dispatch(pool: &SqlitePool, input: &serde_json::Value) {
    // ruleid: zanpos-ai-mutation-needs-confirmation
    crate::ai::tools::execute_mutation_raw(pool, "fixture_mutation", input, 3).await;
}

#[tauri::command]
async fn aliased_raw_ai_dispatch(pool: &SqlitePool, input: &serde_json::Value) {
    // ruleid: zanpos-no-raw-mutation-import-alias
    use crate::ai::tools::execute_mutation_raw as raw;
    // ruleid: zanpos-ai-mutation-needs-confirmation
    raw(pool, "fixture_mutation", input, 3).await;
    // ruleid: zanpos-no-raw-mutation-import-alias
    use crate::ai::tools::execute_mutation_raw as alternate;
    // ruleid: zanpos-ai-mutation-needs-confirmation
    alternate(pool, "fixture_mutation", input, 3).await;
}

#[tauri::command]
async fn confirmed_ai_dispatch(pool: &SqlitePool, input: &serde_json::Value) {
    let context = crate::ai::tool_policy::MutationExecutionContext {
        actor_user_id: "fixture-actor".into(),
        branch_id: "fixture-branch".into(),
    };
    // ok: zanpos-ai-mutation-needs-confirmation
    crate::ai::tool_policy::execute_confirmed_mutation(
        pool,
        &context,
        "fixture_mutation",
        input,
        3,
    )
    .await;
}
