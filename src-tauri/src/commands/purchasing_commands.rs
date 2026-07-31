use crate::commands::{rbac, sync_commands};
use crate::errors::{AppError, AppResult};
use crate::AppState;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use std::str::FromStr;
use tauri::State;
use ulid::Ulid;

#[derive(Debug, Serialize)]
pub struct SupplierRow {
    pub supplier_id: String,
    pub name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub contact_name: Option<String>,
    pub address: Option<String>,
    pub notes: Option<String>,
    pub is_active: bool,
    pub product_count: i64,
    pub open_po_count: i64,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct SupplierUpsertInput {
    pub supplier_id: Option<String>,
    pub name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub contact_name: Option<String>,
    pub address: Option<String>,
    pub notes: Option<String>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct PurchaseOrderRow {
    pub po_id: String,
    pub supplier_id: Option<String>,
    pub supplier_name: Option<String>,
    pub status: String,
    pub expected_date: Option<String>,
    pub received_date: Option<String>,
    pub notes: Option<String>,
    pub line_count: i64,
    pub ordered_total_minor: i64,
    pub received_total_minor: i64,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct PurchaseOrderLineRow {
    pub po_line_id: String,
    pub product_id: Option<String>,
    pub product_name: String,
    pub ordered_qty: f64,
    pub received_qty: f64,
    pub unit_cost_minor: i64,
}

#[derive(Debug, Serialize)]
pub struct PurchaseOrderDetail {
    pub order: PurchaseOrderRow,
    pub lines: Vec<PurchaseOrderLineRow>,
}

#[derive(Debug, Deserialize)]
pub struct PurchaseOrderLineInput {
    pub product_id: Option<String>,
    pub product_name: String,
    pub ordered_qty: String,
    pub unit_cost_minor: i64,
}

#[derive(Debug, Deserialize)]
pub struct PurchaseOrderCreateInput {
    pub supplier_id: Option<String>,
    pub expected_date: Option<String>,
    pub notes: Option<String>,
    pub created_by: String,
    pub lines: Vec<PurchaseOrderLineInput>,
}

#[derive(Debug, Deserialize)]
pub struct ReceivePurchaseOrderLineInput {
    pub po_line_id: String,
    pub received_qty: String,
    pub expiry_date: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReceivePurchaseOrderInput {
    pub po_id: String,
    pub actor_user_id: String,
    pub lines: Option<Vec<ReceivePurchaseOrderLineInput>>,
}

#[derive(Debug, Serialize)]
pub struct ReceivePurchaseOrderResult {
    pub po_id: String,
    pub status: String,
    pub lines_received: i64,
    pub units_received: String,
    pub cost_updates: i64,
}

#[tauri::command]
pub async fn supplier_list(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<SupplierRow>, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    supplier_list_inner(&state.db).await
}

#[tauri::command]
pub async fn supplier_upsert(
    actor_user_id: String,
    input: SupplierUpsertInput,
    state: State<'_, AppState>,
) -> Result<SupplierRow, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let row = supplier_upsert_inner(&state.db, input).await?;
    sync_commands::schedule_immediate_sync(&state);
    Ok(row)
}

#[tauri::command]
pub async fn supplier_delete(
    actor_user_id: String,
    supplier_id: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    supplier_delete_inner(&state.db, &supplier_id).await?;
    sync_commands::schedule_immediate_sync(&state);
    Ok(())
}

#[tauri::command]
pub async fn po_list(
    actor_user_id: String,
    status: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<PurchaseOrderRow>, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    po_list_inner(&state.db, status.as_deref()).await
}

#[tauri::command]
pub async fn po_get(
    actor_user_id: String,
    po_id: String,
    state: State<'_, AppState>,
) -> Result<PurchaseOrderDetail, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    po_get_inner(&state.db, &po_id).await
}

#[tauri::command]
pub async fn po_create(
    actor_user_id: String,
    input: PurchaseOrderCreateInput,
    state: State<'_, AppState>,
) -> Result<PurchaseOrderDetail, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let detail = po_create_inner(&state.db, input).await?;
    sync_commands::schedule_immediate_sync(&state);
    Ok(detail)
}

#[tauri::command]
pub async fn po_receive(
    input: ReceivePurchaseOrderInput,
    state: State<'_, AppState>,
) -> Result<ReceivePurchaseOrderResult, AppError> {
    rbac::manager_or_owner(&state.db, &input.actor_user_id).await?;
    let branch_id = active_branch_id(&state.db).await?;
    let device_id = active_device_id(&state.db).await?;
    let result = po_receive_inner(&state.db, input, &branch_id, &device_id).await?;
    sync_commands::schedule_immediate_sync(&state);
    Ok(result)
}

#[tauri::command]
pub async fn po_cancel(
    actor_user_id: String,
    po_id: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    po_cancel_inner(&state.db, &po_id).await?;
    sync_commands::schedule_immediate_sync(&state);
    Ok(())
}

pub(crate) async fn active_branch_id(pool: &SqlitePool) -> AppResult<String> {
    let row = sqlx::query(
        "SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("No active branch configured".into()))?;
    Ok(row.get("branch_id"))
}

pub(crate) async fn active_device_id(pool: &SqlitePool) -> AppResult<String> {
    if let Ok(Some(id)) =
        sqlx::query_scalar::<_, String>("SELECT value FROM app_config WHERE key = 'device_id'")
            .fetch_optional(pool)
            .await
    {
        if !id.is_empty() {
            return Ok(id);
        }
    }
    let row = sqlx::query(
        "SELECT device_id FROM devices WHERE is_active = 1 ORDER BY device_code LIMIT 1",
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("No active device configured".into()))?;
    Ok(row.get("device_id"))
}

async fn supplier_list_inner(pool: &SqlitePool) -> Result<Vec<SupplierRow>, AppError> {
    let rows = sqlx::query(
        "SELECT s.supplier_id, s.name, s.phone, s.email, s.contact_name, s.address, s.notes,
                s.is_active, s.updated_at,
                (SELECT COUNT(*) FROM products p WHERE p.default_supplier_id = s.supplier_id) AS product_count,
                (SELECT COUNT(*) FROM purchase_orders po WHERE po.supplier_id = s.supplier_id AND po.status IN ('draft','ordered','partial')) AS open_po_count
         FROM suppliers s
         ORDER BY s.is_active DESC, s.name",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(supplier_from_row).collect())
}

async fn supplier_upsert_inner(
    pool: &SqlitePool,
    input: SupplierUpsertInput,
) -> Result<SupplierRow, AppError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::Validation("Supplier name is required".into()));
    }
    let now = chrono::Utc::now().to_rfc3339();
    let supplier_id = input.supplier_id.unwrap_or_else(|| Ulid::new().to_string());
    sqlx::query(
        "INSERT INTO suppliers
         (supplier_id, name, phone, email, contact_name, address, notes, is_active, created_at, updated_at, sync_status)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending')
         ON CONFLICT(supplier_id) DO UPDATE SET
           name = excluded.name,
           phone = excluded.phone,
           email = excluded.email,
           contact_name = excluded.contact_name,
           address = excluded.address,
           notes = excluded.notes,
           is_active = excluded.is_active,
           updated_at = excluded.updated_at,
           sync_status = 'pending'",
    )
    .bind(&supplier_id)
    .bind(name)
    .bind(input.phone)
    .bind(input.email)
    .bind(input.contact_name)
    .bind(input.address)
    .bind(input.notes)
    .bind(input.is_active.unwrap_or(true) as i64)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    supplier_by_id(pool, &supplier_id).await
}

async fn supplier_delete_inner(pool: &SqlitePool, supplier_id: &str) -> Result<(), AppError> {
    let linked_po: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM purchase_orders WHERE supplier_id = ?")
            .bind(supplier_id)
            .fetch_one(pool)
            .await?;
    if linked_po > 0 {
        return Err(AppError::Validation(
            "Supplier has purchase orders. Deactivate it instead.".into(),
        ));
    }
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE suppliers SET is_active = 0, updated_at = ?, sync_status = 'pending' WHERE supplier_id = ?",
    )
    .bind(&now)
    .bind(supplier_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn supplier_by_id(pool: &SqlitePool, supplier_id: &str) -> Result<SupplierRow, AppError> {
    let row = sqlx::query(
        "SELECT s.supplier_id, s.name, s.phone, s.email, s.contact_name, s.address, s.notes,
                s.is_active, s.updated_at,
                (SELECT COUNT(*) FROM products p WHERE p.default_supplier_id = s.supplier_id) AS product_count,
                (SELECT COUNT(*) FROM purchase_orders po WHERE po.supplier_id = s.supplier_id AND po.status IN ('draft','ordered','partial')) AS open_po_count
         FROM suppliers s WHERE s.supplier_id = ?",
    )
    .bind(supplier_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Supplier {supplier_id} not found")))?;
    Ok(supplier_from_row(&row))
}

fn supplier_from_row(row: &sqlx::sqlite::SqliteRow) -> SupplierRow {
    SupplierRow {
        supplier_id: row.get("supplier_id"),
        name: row.get("name"),
        phone: row.get("phone"),
        email: row.get("email"),
        contact_name: row.get("contact_name"),
        address: row.get("address"),
        notes: row.get("notes"),
        is_active: row.get::<i64, _>("is_active") != 0,
        product_count: row.get("product_count"),
        open_po_count: row.get("open_po_count"),
        updated_at: row.get("updated_at"),
    }
}

async fn po_list_inner(
    pool: &SqlitePool,
    status: Option<&str>,
) -> Result<Vec<PurchaseOrderRow>, AppError> {
    let status_filter = status.unwrap_or("%");
    let rows = sqlx::query(
        "SELECT po.po_id, po.supplier_id, s.name AS supplier_name, po.status, po.expected_date,
                po.received_date, po.notes, po.updated_at,
                COUNT(pol.po_line_id) AS line_count,
                COALESCE(SUM(CAST(pol.ordered_qty AS REAL) * pol.unit_cost_minor), 0) AS ordered_total_minor,
                COALESCE(SUM(CAST(pol.received_qty AS REAL) * pol.unit_cost_minor), 0) AS received_total_minor
         FROM purchase_orders po
         LEFT JOIN suppliers s ON s.supplier_id = po.supplier_id
         LEFT JOIN purchase_order_lines pol ON pol.po_id = po.po_id
         WHERE po.status LIKE ?
         GROUP BY po.po_id
         ORDER BY po.updated_at DESC",
    )
    .bind(status_filter)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(po_from_row).collect())
}

async fn po_get_inner(pool: &SqlitePool, po_id: &str) -> Result<PurchaseOrderDetail, AppError> {
    let order = po_list_inner(pool, None)
        .await?
        .into_iter()
        .find(|po| po.po_id == po_id)
        .ok_or_else(|| AppError::NotFound(format!("PO {po_id} not found")))?;
    let lines = sqlx::query(
        "SELECT po_line_id, product_id, product_name, ordered_qty, received_qty, unit_cost_minor
         FROM purchase_order_lines WHERE po_id = ? ORDER BY created_at, po_line_id",
    )
    .bind(po_id)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|r| PurchaseOrderLineRow {
        po_line_id: r.get("po_line_id"),
        product_id: r.get("product_id"),
        product_name: r.get("product_name"),
        ordered_qty: r.get("ordered_qty"),
        received_qty: r.get("received_qty"),
        unit_cost_minor: r.get("unit_cost_minor"),
    })
    .collect();
    Ok(PurchaseOrderDetail { order, lines })
}

fn po_from_row(row: &sqlx::sqlite::SqliteRow) -> PurchaseOrderRow {
    PurchaseOrderRow {
        po_id: row.get("po_id"),
        supplier_id: row.get("supplier_id"),
        supplier_name: row.get("supplier_name"),
        status: row.get("status"),
        expected_date: row.get("expected_date"),
        received_date: row.get("received_date"),
        notes: row.get("notes"),
        line_count: row.get("line_count"),
        ordered_total_minor: row.get::<f64, _>("ordered_total_minor").round() as i64,
        received_total_minor: row.get::<f64, _>("received_total_minor").round() as i64,
        updated_at: row.get("updated_at"),
    }
}

async fn po_create_inner(
    pool: &SqlitePool,
    input: PurchaseOrderCreateInput,
) -> Result<PurchaseOrderDetail, AppError> {
    if input.lines.is_empty() {
        return Err(AppError::Validation(
            "Purchase order needs at least one line".into(),
        ));
    }
    let now = chrono::Utc::now().to_rfc3339();
    let po_id = Ulid::new().to_string();
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO purchase_orders
         (po_id, supplier_id, status, expected_date, notes, created_by, created_at, updated_at, sync_status)
         VALUES (?, ?, 'draft', ?, ?, ?, ?, ?, 'pending')",
    )
    .bind(&po_id)
    .bind(input.supplier_id.as_deref())
    .bind(input.expected_date.as_deref())
    .bind(input.notes.as_deref())
    .bind(&input.created_by)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    for line in input.lines {
        if line.product_name.trim().is_empty() {
            return Err(AppError::Validation(
                "PO line product name is required".into(),
            ));
        }
        if line.unit_cost_minor < 0 {
            return Err(AppError::Validation(
                "PO line cost cannot be negative".into(),
            ));
        }
        let qty = parse_qty(&line.ordered_qty)?;
        if qty <= Decimal::ZERO {
            return Err(AppError::Validation(
                "PO line quantity must be positive".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO purchase_order_lines
             (po_line_id, po_id, product_id, product_name, ordered_qty, received_qty, unit_cost_minor, created_at, updated_at, sync_status)
             VALUES (?, ?, ?, ?, ?, 0, ?, ?, ?, 'pending')",
        )
        .bind(Ulid::new().to_string())
        .bind(&po_id)
        .bind(line.product_id.as_deref())
        .bind(line.product_name.trim())
        .bind(qty.to_f64().unwrap_or(0.0))
        .bind(line.unit_cost_minor)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    po_get_inner(pool, &po_id).await
}

async fn po_cancel_inner(pool: &SqlitePool, po_id: &str) -> Result<(), AppError> {
    let received: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(received_qty), 0) FROM purchase_order_lines WHERE po_id = ?",
    )
    .bind(po_id)
    .fetch_one(pool)
    .await?;
    if received > 0.0 {
        return Err(AppError::Validation(
            "Cannot cancel a purchase order after stock has been received".into(),
        ));
    }
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE purchase_orders SET status = 'cancelled', updated_at = ?, sync_status = 'pending' WHERE po_id = ?",
    )
    .bind(&now)
    .bind(po_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn po_receive_inner(
    pool: &SqlitePool,
    input: ReceivePurchaseOrderInput,
    branch_id: &str,
    device_id: &str,
) -> Result<ReceivePurchaseOrderResult, AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut tx = pool.begin().await?;
    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM purchase_orders WHERE po_id = ?")
            .bind(&input.po_id)
            .fetch_optional(&mut *tx)
            .await?;
    match status.as_deref() {
        None => return Err(AppError::NotFound(format!("PO {} not found", input.po_id))),
        Some("cancelled") => {
            return Err(AppError::Validation("Cannot receive a cancelled PO".into()))
        }
        Some("received") => {
            return Err(AppError::Validation("PO is already fully received".into()))
        }
        _ => {}
    }

    let rows = sqlx::query(
        "SELECT po_line_id, product_id, product_name, ordered_qty, received_qty, unit_cost_minor
         FROM purchase_order_lines WHERE po_id = ? ORDER BY created_at, po_line_id",
    )
    .bind(&input.po_id)
    .fetch_all(&mut *tx)
    .await?;
    let requested = input.lines.unwrap_or_default();
    let receive_all = requested.is_empty();
    let mut lines_received = 0;
    let mut units_received = Decimal::ZERO;
    let mut cost_updates = 0;

    for row in rows {
        let po_line_id: String = row.get("po_line_id");
        let product_id: Option<String> = row.get("product_id");
        let ordered =
            Decimal::from_f64_retain(row.get::<f64, _>("ordered_qty")).unwrap_or(Decimal::ZERO);
        let received =
            Decimal::from_f64_retain(row.get::<f64, _>("received_qty")).unwrap_or(Decimal::ZERO);
        let unit_cost: i64 = row.get("unit_cost_minor");
        let (qty, expiry_date) = if receive_all {
            (ordered - received, None)
        } else if let Some(line) = requested.iter().find(|l| l.po_line_id == po_line_id) {
            (
                parse_qty(&line.received_qty)?,
                crate::inventory::lots::validate_expiry_date(line.expiry_date.as_deref())?,
            )
        } else {
            continue;
        };
        if qty <= Decimal::ZERO {
            continue;
        }
        if received + qty > ordered {
            return Err(AppError::Validation(format!(
                "Received quantity exceeds ordered quantity for line {po_line_id}"
            )));
        }

        sqlx::query(
            "UPDATE purchase_order_lines
             SET received_qty = received_qty + ?, updated_at = ?, sync_status = 'pending'
             WHERE po_line_id = ?",
        )
        .bind(qty.to_f64().unwrap_or(0.0))
        .bind(&now)
        .bind(&po_line_id)
        .execute(&mut *tx)
        .await?;

        if let Some(pid) = product_id {
            let old_qty_str: Option<String> = sqlx::query_scalar(
                "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?",
            )
            .bind(&pid)
            .bind(branch_id)
            .fetch_optional(&mut *tx)
            .await?;
            let old_qty = old_qty_str
                .as_deref()
                .and_then(|s| Decimal::from_str(s).ok())
                .unwrap_or(Decimal::ZERO);
            let new_qty = old_qty + qty;
            let stock_level_id = format!("SL-{pid}-{branch_id}");
            sqlx::query(
                "INSERT INTO stock_levels
                 (stock_level_id, product_id, branch_id, quantity_on_hand, last_movement_at, created_at, updated_at, sync_status)
                 VALUES (?, ?, ?, ?, ?, ?, ?, 'pending')
                 ON CONFLICT(product_id, branch_id) DO UPDATE SET
                   quantity_on_hand = excluded.quantity_on_hand,
                   last_movement_at = excluded.last_movement_at,
                   updated_at = excluded.updated_at,
                   sync_status = 'pending'",
            )
            .bind(&stock_level_id)
            .bind(&pid)
            .bind(branch_id)
            .bind(new_qty.to_string())
            .bind(&now)
            .bind(&now)
            .bind(&now)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                "INSERT INTO stock_movements
                 (movement_id, product_id, branch_id, device_id, origin_device_id, movement_type,
                  quantity_delta, quantity_after, reference_type, reference_id, notes, created_by_user_id,
                  created_at, updated_at, sync_status, expiry_date,
                  lot_quantity_received, lot_quantity_remaining)
                 VALUES (?, ?, ?, ?, ?, 'receive', ?, ?, 'purchase_order', ?, ?, ?, ?, ?, 'pending',
                         ?, ?, ?)",
            )
            .bind(Ulid::new().to_string())
            .bind(&pid)
            .bind(branch_id)
            .bind(device_id)
            .bind(device_id)
            .bind(qty.to_string())
            .bind(new_qty.to_string())
            .bind(&input.po_id)
            .bind(format!("PO receipt line {po_line_id}"))
            .bind(&input.actor_user_id)
            .bind(&now)
            .bind(&now)
            .bind(expiry_date)
            .bind(qty.to_string())
            .bind(qty.to_string())
            .execute(&mut *tx)
            .await?;

            let old_cost: Option<i64> =
                sqlx::query_scalar("SELECT cost_minor FROM products WHERE product_id = ?")
                    .bind(&pid)
                    .fetch_optional(&mut *tx)
                    .await?;
            let new_cost = weighted_average_cost(old_cost, old_qty, unit_cost, qty);
            if Some(new_cost) != old_cost {
                sqlx::query(
                    "UPDATE products SET cost_minor = ?, updated_at = ?, sync_status = 'pending' WHERE product_id = ?",
                )
                .bind(new_cost)
                .bind(&now)
                .bind(&pid)
                .execute(&mut *tx)
                .await?;
                sqlx::query(
                    "INSERT INTO product_cost_history
                     (cost_history_id, product_id, old_cost_minor, new_cost_minor, supplier_id, source, actor_user_id, created_at, updated_at, sync_status)
                     VALUES (?, ?, ?, ?, (SELECT supplier_id FROM purchase_orders WHERE po_id = ?), 'purchase_order_receive', ?, ?, ?, 'pending')",
                )
                .bind(Ulid::new().to_string())
                .bind(&pid)
                .bind(old_cost)
                .bind(new_cost)
                .bind(&input.po_id)
                .bind(&input.actor_user_id)
                .bind(&now)
                .bind(&now)
                .execute(&mut *tx)
                .await?;
                cost_updates += 1;
            }
        }
        lines_received += 1;
        units_received += qty;
    }

    let (total_ordered, total_received): (f64, f64) = sqlx::query_as(
        "SELECT COALESCE(SUM(ordered_qty),0), COALESCE(SUM(received_qty),0)
         FROM purchase_order_lines WHERE po_id = ?",
    )
    .bind(&input.po_id)
    .fetch_one(&mut *tx)
    .await?;
    let new_status = if total_received >= total_ordered && total_ordered > 0.0 {
        "received"
    } else if total_received > 0.0 {
        "partial"
    } else {
        "ordered"
    };
    sqlx::query(
        "UPDATE purchase_orders
         SET status = ?, received_date = CASE WHEN ? = 'received' THEN ? ELSE received_date END,
             updated_at = ?, sync_status = 'pending'
         WHERE po_id = ?",
    )
    .bind(new_status)
    .bind(new_status)
    .bind(&now)
    .bind(&now)
    .bind(&input.po_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(ReceivePurchaseOrderResult {
        po_id: input.po_id,
        status: new_status.to_string(),
        lines_received,
        units_received: units_received.to_string(),
        cost_updates,
    })
}

fn parse_qty(qty: &str) -> Result<Decimal, AppError> {
    Decimal::from_str(qty.trim())
        .map_err(|_| AppError::Validation(format!("Invalid quantity '{qty}'")))
}

fn weighted_average_cost(
    old_cost_minor: Option<i64>,
    old_qty: Decimal,
    received_cost_minor: i64,
    received_qty: Decimal,
) -> i64 {
    if received_cost_minor <= 0 {
        return old_cost_minor.unwrap_or(0);
    }
    if old_qty <= Decimal::ZERO {
        return received_cost_minor;
    }
    let old_cost = Decimal::from(old_cost_minor.unwrap_or(received_cost_minor));
    let received_cost = Decimal::from(received_cost_minor);
    let total_qty = old_qty + received_qty;
    if total_qty <= Decimal::ZERO {
        return received_cost_minor;
    }
    ((old_cost * old_qty + received_cost * received_qty) / total_qty)
        .round()
        .to_i64()
        .unwrap_or(received_cost_minor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations");
        sqlx::query("UPDATE devices SET is_active=1 WHERE device_id='01JDEVICE0000000000000001'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE branches SET is_active=1 WHERE branch_id='01JBRANCH0000000000000001'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT OR IGNORE INTO categories (category_id, name, sort_order, is_active, created_at, updated_at, version)
             VALUES ('CAT1', 'Stock', 1, 1, datetime('now'), datetime('now'), 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT OR IGNORE INTO products
             (product_id, category_id, name, track_inventory, is_active, cost_minor, currency, created_at, updated_at, version)
             VALUES ('P1', 'CAT1', 'Syringe', 1, 1, 100, 'BHD', datetime('now'), datetime('now'), 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT OR IGNORE INTO stock_levels
             (stock_level_id, product_id, branch_id, quantity_on_hand, created_at, updated_at, sync_status, sync_attempts)
             VALUES ('SL-P1', 'P1', '01JBRANCH0000000000000001', '10', datetime('now'), datetime('now'), 'synced', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[test]
    fn weighted_average_cost_rounds_integer_minor_units() {
        let cost = weighted_average_cost(Some(100), Decimal::from(10), 200, Decimal::from(5));
        assert_eq!(cost, 133);
    }

    #[test]
    fn purchasing_tables_participate_in_sync() {
        for table in ["suppliers", "purchase_orders", "purchase_order_lines"] {
            assert!(crate::commands::sync_commands::SYNC_TABLES.contains(&table));
            assert!(crate::sync_v2::apply::SYNC_TABLES.contains(&table));
        }
        assert_eq!(
            crate::sync_v2::apply::pk_for_table("suppliers"),
            "supplier_id"
        );
        assert_eq!(
            crate::sync_v2::apply::pk_for_table("purchase_orders"),
            "po_id"
        );
        assert_eq!(
            crate::sync_v2::apply::pk_for_table("purchase_order_lines"),
            "po_line_id"
        );
    }

    #[tokio::test]
    async fn po_receive_updates_stock_movement_weighted_cost_and_history() {
        let pool = pool().await;
        let supplier = supplier_upsert_inner(
            &pool,
            SupplierUpsertInput {
                supplier_id: Some("S1".into()),
                name: "Main Supplier".into(),
                phone: None,
                email: None,
                contact_name: None,
                address: None,
                notes: None,
                is_active: Some(true),
            },
        )
        .await
        .unwrap();
        let po = po_create_inner(
            &pool,
            PurchaseOrderCreateInput {
                supplier_id: Some(supplier.supplier_id),
                expected_date: None,
                notes: None,
                created_by: "01JUSER000000000000ADMIN1".into(),
                lines: vec![PurchaseOrderLineInput {
                    product_id: Some("P1".into()),
                    product_name: "Syringe".into(),
                    ordered_qty: "5".into(),
                    unit_cost_minor: 200,
                }],
            },
        )
        .await
        .unwrap();

        let result = po_receive_inner(
            &pool,
            ReceivePurchaseOrderInput {
                po_id: po.order.po_id.clone(),
                actor_user_id: "01JUSER000000000000ADMIN1".into(),
                lines: Some(vec![ReceivePurchaseOrderLineInput {
                    po_line_id: po.lines[0].po_line_id.clone(),
                    received_qty: "5".into(),
                    expiry_date: Some("2026-12-31".into()),
                }]),
            },
            "01JBRANCH0000000000000001",
            "01JDEVICE0000000000000001",
        )
        .await
        .unwrap();

        let stock: String =
            sqlx::query_scalar("SELECT quantity_on_hand FROM stock_levels WHERE product_id='P1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let movement_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM stock_movements WHERE product_id='P1' AND reference_type='purchase_order'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let expiry: Option<String> = sqlx::query_scalar(
            "SELECT expiry_date FROM stock_movements
             WHERE product_id='P1' AND reference_type='purchase_order'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let cost: i64 = sqlx::query_scalar("SELECT cost_minor FROM products WHERE product_id='P1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        let history_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM product_cost_history WHERE product_id='P1' AND source='purchase_order_receive'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(result.status, "received");
        assert_eq!(stock, "15");
        assert_eq!(movement_count, 1);
        assert_eq!(expiry.as_deref(), Some("2026-12-31"));
        assert_eq!(cost, 133);
        assert_eq!(history_count, 1);
    }
}
