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

#[tauri::command]
async fn update_without_rbac(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // ruleid: zanpos-tauri-mutation-requires-rbac
    sqlx::query("UPDATE products SET is_active = 0")
        .execute(pool)
        .await?;
    Ok(())
}

#[tauri::command]
async fn update_with_rbac(pool: &SqlitePool, actor_user_id: &str) -> Result<(), sqlx::Error> {
    rbac::manager_or_owner(pool, actor_user_id).await?;
    // ok: zanpos-tauri-mutation-requires-rbac
    sqlx::query("UPDATE products SET is_active = 1")
        .execute(pool)
        .await?;
    Ok(())
}

#[tauri::command]
async fn update_with_late_rbac(pool: &SqlitePool, actor_user_id: &str) -> Result<(), sqlx::Error> {
    // ok: zanpos-tauri-mutation-requires-rbac
    sqlx::query("UPDATE products SET is_active = 1").execute(pool).await?;
    rbac::manager_or_owner(pool, actor_user_id).await?;
    Ok(())
}

#[tauri::command]
async fn update_with_nested_rbac(pool: &SqlitePool, actor_user_id: &str) -> Result<(), sqlx::Error> {
    if actor_user_id.is_empty() {
        return Err(sqlx::Error::RowNotFound);
    }
    rbac::manager_or_owner(pool, actor_user_id).await?;
    // ok: zanpos-tauri-mutation-requires-rbac
    sqlx::query("UPDATE products SET is_active = 1").execute(pool).await?;
    Ok(())
}

#[tauri::command]
async fn update_mixed_order(pool: &SqlitePool, actor_user_id: &str) -> Result<(), sqlx::Error> {
    // ok: zanpos-tauri-mutation-requires-rbac
    sqlx::query("UPDATE products SET is_active = 0").execute(pool).await?;
    rbac::manager_or_owner(pool, actor_user_id).await?;
    // ok: zanpos-tauri-mutation-requires-rbac
    sqlx::query("UPDATE products SET is_active = 1").execute(pool).await?;
    Ok(())
}

#[tauri::command]
async fn approve_with_repo_without_rbac(pool: &SqlitePool) {
    // ruleid: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::sale_repo::approve(pool, "fixture").await;
}

#[tauri::command]
async fn approve_with_repo_rbac(pool: &SqlitePool, actor_user_id: &str) -> Result<(), sqlx::Error> {
    rbac::manager_or_owner(pool, actor_user_id).await?;
    // ok: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::sale_repo::approve(pool, "fixture").await;
    Ok(())
}

#[tauri::command]
async fn approve_with_repo_nested_rbac(
    pool: &SqlitePool,
    actor_user_id: &str,
) -> Result<(), sqlx::Error> {
    if actor_user_id.is_empty() {
        return Err(sqlx::Error::RowNotFound);
    }
    rbac::manager_or_owner(pool, actor_user_id).await?;
    // ok: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::sale_repo::approve(pool, "fixture").await;
    Ok(())
}

#[tauri::command]
async fn rollback_with_late_rbac(pool: &SqlitePool, actor_user_id: &str) -> Result<(), sqlx::Error> {
    // ok: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::sale_repo::rollback(pool, "fixture").await;
    rbac::manager_or_owner(pool, actor_user_id).await?;
    Ok(())
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
    // ruleid: zanpos-no-owned-let-format-sql
    let sql = format!("SELECT {} FROM products", renamed);
    // ruleid: zanpos-no-dynamic-sql-format
    sqlx::query_as::<_, (String,)>(&sql).fetch_optional(pool).await;
}

async fn raw_sql_owned_inline(pool: &SqlitePool, renamed: String) {
    // ruleid: zanpos-no-owned-inline-format-sql, zanpos-no-dynamic-sql-format
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

async fn raw_sql_owned_string_safe(pool: &SqlitePool, _renamed: String) {
    // ok: zanpos-no-owned-inline-format-sql
    // ok: zanpos-no-owned-let-format-sql
    // ok: zanpos-no-dynamic-sql-format
    sqlx::query_as::<_, (String,)>("SELECT name FROM products").fetch_optional(pool).await;
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
