use crate::commands::rbac;
use crate::errors::{AppError, AppResult};
use crate::AppState;
use serde::Serialize;
use tauri::State;
use ulid::Ulid;
use sqlx::FromRow;

// ── Output types ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, FromRow)]
pub struct GhostBarcode {
    pub id: String,
    pub barcode: String,
    pub scan_count: i64,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
    pub status: String,
    pub product_name: Option<String>,
    pub brand: Option<String>,
    pub category: Option<String>,
    pub image_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GhostSummary {
    pub pending: i64,
    pub found: i64,
    pub not_found: i64,
}

#[derive(Debug, Serialize)]
pub struct ProductPrefill {
    pub name: String,
    pub barcode: String,
    pub brand: Option<String>,
    pub category: Option<String>,
    pub image_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ResolveResult {
    pub resolved: i64,
    pub not_found: i64,
}

// ── Helper: current time as Unix milliseconds ─────────────────────────────────

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Called by the POS frontend whenever a barcode scan fails.
/// No RBAC — any authenticated user (cashier) may call this.
/// Fire-and-forget from the frontend: returns Ok(()) always.
#[tauri::command]
pub async fn ghost_record(
    barcode: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    if barcode.trim().is_empty() {
        return Ok(());
    }
    let id = Ulid::new().to_string();
    let now = now_ms();
    // The WHERE status = 'pending' guard is intentional: once a barcode is
    // resolved ('found'/'not_found') or dismissed, re-scanning it should NOT
    // reset its resolved data or increment a stale count. Silently no-op.
    sqlx::query(
        "INSERT INTO unknown_barcodes (id, barcode, scan_count, first_seen_at, last_seen_at)
         VALUES (?, ?, 1, ?, ?)
         ON CONFLICT(barcode) DO UPDATE
           SET scan_count   = scan_count + 1,
               last_seen_at = excluded.last_seen_at
         WHERE status = 'pending'",
    )
    .bind(&id)
    .bind(&barcode)
    .bind(now)
    .bind(now)
    .execute(&state.db)
    .await?;
    Ok(())
}

/// Returns counts of pending/found/not_found barcodes.
/// Used by BackOffice nav badge. Manager/owner only.
#[tauri::command]
pub async fn ghost_summary(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<GhostSummary> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT status, COUNT(*) as cnt
         FROM unknown_barcodes
         WHERE status != 'dismissed'
         GROUP BY status",
    )
    .fetch_all(&state.db)
    .await?;

    let mut summary = GhostSummary { pending: 0, found: 0, not_found: 0 };
    for (status, cnt) in rows {
        match status.as_str() {
            "pending"   => summary.pending   = cnt,
            "found"     => summary.found     = cnt,
            "not_found" => summary.not_found = cnt,
            _ => {}
        }
    }
    Ok(summary)
}

/// Full list of non-dismissed barcodes, ordered by scan_count DESC.
/// Manager/owner only.
#[tauri::command]
pub async fn ghost_list(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<GhostBarcode>> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let rows: Vec<GhostBarcode> = sqlx::query_as(
        "SELECT id, barcode, scan_count, first_seen_at, last_seen_at,
                status, product_name, brand, category, image_url
         FROM unknown_barcodes
         WHERE status != 'dismissed'
         ORDER BY scan_count DESC, last_seen_at DESC",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(rows)
}

/// Set a ghost barcode to 'dismissed' — removes it from the panel.
/// Manager/owner only.
#[tauri::command]
pub async fn ghost_dismiss(
    id: String,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    // Intentionally idempotent: if the row was already dismissed (or never
    // existed), this is a no-op rather than an error.  The manager panel will
    // have removed the card client-side already, so a 404 here is noise.
    sqlx::query("UPDATE unknown_barcodes SET status = 'dismissed' WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;
    Ok(())
}

/// Returns product data pre-filled from a 'found' ghost barcode row.
/// Used by "Create Product" button to pre-fill the product form.
/// Manager/owner only.
#[tauri::command]
pub async fn ghost_prefill(
    id: String,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<ProductPrefill> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let row: Option<(String, String, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as(
            "SELECT product_name, barcode, brand, category, image_url
             FROM unknown_barcodes
             WHERE id = ? AND status = 'found'",
        )
        .bind(&id)
        .fetch_optional(&state.db)
        .await?;

    let (name, barcode, brand, category, image_url) =
        row.ok_or_else(|| AppError::NotFound("Ghost barcode not found or not resolved".into()))?;

    Ok(ProductPrefill { name, barcode, brand, category, image_url })
}
