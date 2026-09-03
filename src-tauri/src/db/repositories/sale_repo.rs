use crate::db::repositories::{audit_hash, delivery_repo};
use crate::domain::cart::Cart;
use crate::domain::delivery::DeliveryInput;
use crate::domain::sale::{PaymentInput, PaymentSummary, SaleItemSummary, SaleResult};
use crate::errors::{AppError, AppResult};
use crate::inventory::movements;
use sqlx::{Row, SqlitePool};
use ulid::Ulid;

/// An amount as a cashier reads it — "BHD 0.250", not "250".
///
/// Every money value inside this module is minor units, which is correct for
/// arithmetic and wrong the moment it leaves for a screen. These strings end up
/// in the till's banner, and printing fils raw told a cashier the price had
/// "changed to 250" for an item that now costs a quarter of a dinar. A number
/// that reads as a thousand times its value is worse than no number: it invites
/// someone to believe the catalogue is broken and override it.
///
/// Called only on the error paths, so the successful checkout pays nothing for
/// it. Falls back to the store's own currency being unreadable rather than
/// failing a sale over a label — the caller is already returning an error and a
/// bare number beats a panic.
/// The selling price currently in force for each product, in fils.
///
/// One query, and one definition of "in force". Checkout used to carry this
/// predicate inline with a comment warning that it must stay identical to the
/// one that prices the cart in the first place — because any divergence between
/// them rejects a correct payment. A warning is not a mechanism; a shared
/// function is.
///
/// Every timestamp goes through `datetime()` before comparison. `effective_from`
/// is stored in two formats — `datetime('now')` from the importer and RFC3339
/// from the admin edit — and a raw text compare ranks 'T' above ' ', so an
/// RFC3339 row reads as not-yet-effective while a closed row reads as still
/// open. Both mistakes price a sale wrongly.
/// Void a completed sale: status, stock and audit entry, or none of them.
///
/// There were two implementations of this. The till's was one transaction; the
/// assistant's was five separate statements against the pool, with a permission
/// check *inside* the stock loop that returned an error after the sale had
/// already been marked voided. That path left a sale voided with its stock not
/// returned and no audit row — a reversal that happened to the books but not to
/// the shelf, and which nothing recorded.
///
/// A reversal is the operation an audit looks at hardest, so it is the last one
/// that should exist twice. `reason` is carried into the audit entry rather than
/// dropped, because "who voided this and why" is the question actually asked.
pub async fn void_sale(
    pool: &SqlitePool,
    sale_id: &str,
    voided_by_user_id: &str,
    reason: Option<&str>,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut tx = pool.begin().await?;

    let row = sqlx::query(
        "UPDATE sales SET status = 'voided', updated_at = ?, sync_status = 'pending'
         WHERE sale_id = ? AND status = 'completed'
         RETURNING branch_id, device_id",
    )
    .bind(&now)
    .bind(sale_id)
    .fetch_optional(&mut *tx)
    .await?;

    let (branch_id, device_id) = match row {
        Some(r) => (
            r.get::<String, _>("branch_id"),
            r.get::<String, _>("device_id"),
        ),
        None => {
            return Err(AppError::NotFound(
                "Sale not found or already voided".into(),
            ))
        }
    };

    // Points the sale awarded go back with it.
    //
    // A void reverses the sale entirely, so the loyalty it earned has to reverse
    // entirely too. Nothing did this: a sale could be rung up, its points
    // awarded, then voided minutes later — the stock returned, the sale was
    // marked voided, and the points stayed on the customer for good. Repeated,
    // that is free loyalty at no cost.
    let awarded =
        crate::db::repositories::loyalty_repo::points_awarded_for_sale(&mut tx, sale_id).await?;
    let already_back =
        crate::db::repositories::loyalty_repo::points_reversed_for_sale(&mut tx, sale_id).await?;
    let owed_back = awarded - already_back;
    if owed_back > 0 {
        let customer: Option<String> =
            sqlx::query_scalar("SELECT customer_id FROM sales WHERE sale_id = ?")
                .bind(sale_id)
                .fetch_optional(&mut *tx)
                .await?
                .flatten();
        if let Some(customer_id) = customer {
            crate::db::repositories::loyalty_repo::record_tx(
                &mut tx,
                crate::db::repositories::loyalty_repo::AwardContext {
                    customer_id: &customer_id,
                    branch_id: Some(&branch_id),
                    device_id: Some(&device_id),
                    event: crate::db::repositories::loyalty_repo::LoyaltyEvent::Adjust,
                    points_delta: -owed_back,
                    reference_type: Some("sale_reversal"),
                    reference_id: Some(sale_id),
                    reason: Some("sale voided"),
                    actor_user_id: Some(voided_by_user_id),
                },
            )
            .await?;
        }
    }

    // Sale status, stock restoration, and the required audit event are one
    // transaction. No partially voided sale can survive an audit failure.
    crate::inventory::movements::return_void_sale(
        &mut tx,
        sale_id,
        voided_by_user_id,
        &branch_id,
        &device_id,
    )
    .await?;

    // The audit row is authoritative and commits with the sale/stock changes.
    let audit_id = ulid::Ulid::new().to_string();
    let prev_hash = crate::db::repositories::audit_hash::fetch_last_hash_tx(&mut tx, &device_id)
        .await
        .unwrap_or_default();
    let hash = crate::db::repositories::audit_hash::compute_audit_hash(
        &crate::db::repositories::audit_hash::AuditHashInput {
            audit_log_id: &audit_id,
            event_type: "sale.voided",
            entity_type: "sale",
            entity_id: sale_id,
            actor_user_id: voided_by_user_id,
            actor_type: "user",
            created_at: &now,
            before_json: None,
            after_json: None,
            reason,
            previous_hash: &prev_hash,
        },
    );
    sqlx::query(
        "INSERT INTO audit_logs
           (audit_log_id, event_type, entity_type, entity_id,
            actor_user_id, actor_type, device_id, origin_device_id, branch_id,
            created_at, reason, hash, previous_hash)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&audit_id)
    .bind("sale.voided")
    .bind("sale")
    .bind(sale_id)
    .bind(voided_by_user_id)
    .bind("user")
    .bind(&device_id)
    .bind(&device_id)
    .bind(&branch_id)
    .bind(&now)
    .bind(reason)
    .bind(&hash)
    .bind(if prev_hash.is_empty() {
        None
    } else {
        Some(prev_hash.clone())
    })
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

pub(crate) async fn current_selling_prices(
    pool: &SqlitePool,
    product_ids: &[&str],
) -> AppResult<std::collections::HashMap<String, i64>> {
    if product_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders = product_ids
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");
    let price_in_force = crate::db::repositories::pricing::PRICE_IN_FORCE;
    let sql = format!(
        "SELECT pp.product_id, pp.price_minor
         FROM product_prices pp
         WHERE pp.product_id IN ({placeholders})
           AND {price_in_force}"
    );
    let mut q = sqlx::query(&sql);
    for pid in product_ids {
        q = q.bind(*pid);
    }
    Ok(q.fetch_all(pool)
        .await?
        .into_iter()
        .map(|row: sqlx::sqlite::SqliteRow| {
            let pid: String = row.get("product_id");
            let price: i64 = row.get("price_minor");
            (pid, price)
        })
        .collect())
}

async fn money_for_operator(pool: &SqlitePool, minor: i64) -> String {
    let currency: String =
        sqlx::query_scalar("SELECT currency FROM branches WHERE is_active = 1 LIMIT 1")
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| "BHD".to_string());
    let exponent = crate::domain::money::currency_exponent(&currency).max(0) as u32;
    format!(
        "{currency} {}",
        crate::domain::money::format_minor(minor, exponent)
    )
}

async fn next_receipt_number<'e, E>(
    executor: E,
    branch_code: &str,
    device_code: &str,
    device_id: &str,
) -> AppResult<String>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    // Atomic per-device counter — UPDATE...RETURNING prevents races.
    // The counter persists independently of sale rows; pruning old sales cannot cause collisions.
    //
    // `RETURNING next_receipt_seq - 1` gives the value the column held *before*
    // this call, which is what the column's name promises: the next number to
    // use. Returning the post-increment value instead meant a device's very
    // first receipt was numbered 00000002 and 00000001 was never issued — a gap
    // at the top of every shop's receipt book, and the first thing anyone
    // reconciling a sequence would ask about.
    //
    // Safe on a till already trading: it has issued 2..N and the column holds
    // N+1, so the next receipt becomes N+1 rather than N+2. That number has
    // never been used, so this closes the gap without a collision.
    let seq: i64 = sqlx::query_scalar(
        "UPDATE devices SET next_receipt_seq = next_receipt_seq + 1
         WHERE device_id = ?
         RETURNING next_receipt_seq - 1",
    )
    .bind(device_id)
    .fetch_one(executor)
    .await?;

    Ok(format!("{}-{}-{:08}", branch_code, device_code, seq))
}

/// The receipt number of a sale already recorded under this idempotency key.
///
/// Kept separate so the two callers — the fast path before any work is done,
/// and the race path where a concurrent submit won the insert — ask the same
/// question the same way.
async fn existing_sale_for_key(
    pool: &SqlitePool,
    idempotency_key: &str,
) -> AppResult<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT receipt_number FROM sales WHERE idempotency_key = ?")
            .bind(idempotency_key)
            .fetch_optional(pool)
            .await?,
    )
}

/// Marks the one error the wrapper below turns back into a success.
///
/// Carried as a sentinel rather than by matching on SQLite's message text,
/// which differs between builds and would silently stop matching one day.
const DUPLICATE_SALE_MARKER: &str = "__zanpos_duplicate_idempotency_key__";

/// True when this error is the `sales.idempotency_key` UNIQUE index refusing a
/// second sale for the same submission.
fn is_duplicate_idempotency_key(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(db)
        if db.is_unique_violation() && db.message().contains("idempotency_key"))
}

/// Record a sale, or return the one this submission already made.
pub async fn finalize_sale(
    pool: &SqlitePool,
    cart: &Cart,
    payments: Vec<PaymentInput>,
    idempotency_key: &str,
    customer_id: Option<&str>,
    created_offline: bool,
    delivery: Option<DeliveryInput>,
    allow_negative_stock: bool,
) -> AppResult<SaleResult> {
    // ── Idempotent replay ────────────────────────────────────────────────────
    //
    // The POS sends the cart id as the key, so pressing Charge twice sends the
    // same one. `sales.idempotency_key` is UNIQUE, so the second attempt could
    // never create a second sale — but it failed with a raw constraint error,
    // and the cashier was shown "Sale failed" for a sale that had just
    // succeeded. The dangerous step is the one after that: believing it did not
    // go through, they rebuild the cart, which mints a *new* cart id, and now
    // there genuinely are two sales for one basket.
    //
    // Same shape for a lost response — the app is closed or the till is
    // restarted between commit and the reply. The cart is still on screen, and
    // pressing Charge again is exactly the right instinct.
    //
    // So a repeat of a key that has already been used returns the sale it
    // already made. That is what an idempotency key is for, and it turns the
    // most likely double-ring into a no-op.
    if let Some(existing) = existing_sale_for_key(pool, idempotency_key).await? {
        tracing::info!(
            "Sale {} replayed from idempotency key — returning the original sale",
            existing
        );
        return crate::db::repositories::refund_repo::get_sale_result_by_receipt(pool, &existing)
            .await;
    }

    match finalize_sale_txn(
        pool,
        cart,
        payments,
        idempotency_key,
        customer_id,
        created_offline,
        delivery,
        allow_negative_stock,
    )
    .await
    {
        // A concurrent submit committed first. Its sale is the sale; this one
        // rolled back and never existed.
        Err(AppError::Conflict(marker)) if marker == DUPLICATE_SALE_MARKER => {
            let receipt = existing_sale_for_key(pool, idempotency_key)
                .await?
                .ok_or_else(|| {
                    AppError::Internal(
                        "a duplicate sale key was rejected but no sale holds it".into(),
                    )
                })?;
            crate::db::repositories::refund_repo::get_sale_result_by_receipt(pool, &receipt).await
        }
        other => other,
    }
}

/// The transaction itself. Everything from the receipt number to the commit.
#[allow(clippy::too_many_arguments)]
async fn finalize_sale_txn(
    pool: &SqlitePool,
    cart: &Cart,
    payments: Vec<PaymentInput>,
    idempotency_key: &str,
    customer_id: Option<&str>,
    created_offline: bool,
    delivery: Option<DeliveryInput>,
    allow_negative_stock: bool,
) -> AppResult<SaleResult> {
    // ── Guard: shift must be open ─────────────────────────────────────────────
    let shift_status: Option<String> =
        sqlx::query_scalar("SELECT status FROM shifts WHERE shift_id = ?")
            .bind(&cart.shift_id)
            .fetch_optional(pool)
            .await?;

    match shift_status.as_deref() {
        Some("open") => {}
        Some(_) => {
            return Err(AppError::Validation(
                "Cannot finalize a sale: the shift is closed.".into(),
            ))
        }
        None => {
            return Err(AppError::Validation(
                "Cannot finalize a sale: shift not found.".into(),
            ))
        }
    }

    // ── Guard: cart must have at least one active (non-voided) line ────────────
    let active_line_count = cart.lines.iter().filter(|l| !l.voided).count();
    if active_line_count == 0 {
        return Err(AppError::Validation(
            "Cannot finalize an empty cart.".into(),
        ));
    }

    // ── Guard: validate quantities and totals are within safe ranges ──────────
    // Catches f64 overflow / malicious IPC input before any DB writes.
    cart.validate()?;

    // ── Guard: each cart line must have a positive price ─────────────────────
    // For product-mapped items, verify the price matches the DB to close the
    // price-manipulation attack vector (zero-price, manipulated IPC call).
    // Pre-batch: fetch all current prices in ONE query instead of N per-line queries.
    let active_lines: Vec<_> = cart.lines.iter().filter(|l| !l.voided).collect();
    let price_product_ids: Vec<&str> = active_lines
        .iter()
        .filter_map(|l| l.product_id.as_deref())
        .collect();

    let db_prices = current_selling_prices(pool, &price_product_ids).await?;

    // A changed cart price is accepted only when the manager-only price command
    // recorded an exact server-side match for this cart line. Cart payload fields
    // alone are not authorization because IPC input can be crafted.
    type ApprovedPrice = (Option<String>, i64, Option<String>);
    let approved_price_overrides: std::collections::HashMap<String, ApprovedPrice> = sqlx::query(
        "SELECT cart_line_id, product_id, price_minor, approved_quantity
         FROM pos_price_overrides
         WHERE cart_id = ?",
    )
    .bind(&cart.cart_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row: sqlx::sqlite::SqliteRow| {
        let line_id: String = row.get("cart_line_id");
        let product_id: Option<String> = row.get("product_id");
        let price: i64 = row.get("price_minor");
        let quantity: Option<String> = row.get("approved_quantity");
        (line_id, (product_id, price, quantity))
    })
    .collect();

    // Map from active_lines index → the catalogue price when the cart line is
    // stale, i.e. the cashier scanned before a price change landed. The sale is
    // then priced from the catalogue, which necessarily disagrees with what the
    // till collected, so the payment guard below rejects it. The correction is
    // not silent — it is what makes the sale fail — and the operator is told
    // which item to re-scan rather than being shown a payment arithmetic error.
    let mut price_corrections: Vec<(usize, i64)> = Vec::new();

    for (i, line) in active_lines.iter().enumerate() {
        if line.unit_price_minor <= 0 {
            return Err(AppError::Validation(format!(
                "Invalid price {} — remove '{}' and scan it again.",
                money_for_operator(pool, line.unit_price_minor).await,
                line.product_name
            )));
        }
        if let Some(ref product_id) = line.product_id {
            if let Some(&db_p) = db_prices.get(product_id.as_str()) {
                if line.unit_price_minor != db_p {
                    // The approval is for a price *and* a quantity. A line that
                    // grew after it was approved is priced from the catalogue
                    // again, which fails the payment check below and tells the
                    // cashier to get the new quantity approved. `approved_quantity`
                    // is NULL only on rows written before it was recorded.
                    let override_matches = approved_price_overrides
                        .get(&line.cart_line_id)
                        .is_some_and(|(product_id, price, quantity)| {
                            product_id.as_deref() == line.product_id.as_deref()
                                && *price == line.unit_price_minor
                                && quantity.as_deref().is_none_or(|approved| {
                                    crate::domain::money::cmp_decimal_qty(&line.quantity, approved)
                                        != Some(std::cmp::Ordering::Greater)
                                })
                        });
                    if !override_matches {
                        price_corrections.push((i, db_p));
                    }
                }
            }
        }
    }

    // ── Guard: every discount was approved by the discount command ───────────
    //
    // The price check above exists because a cart arriving over IPC is not
    // authorisation. Discounts arrived on the same payload and were used
    // verbatim, so `pos_apply_bill_discount`'s manager check, its upper bound,
    // its required reason and its audit entry could all be skipped by calling
    // finalize directly with the discount already in the cart. That is money out
    // of the till with no approval and no record of who took it.
    //
    // `pos_discount_authorizations` holds what the discount commands actually
    // approved. A discount that is not there, or is there for a different
    // amount, is refused.
    let approved_discounts: std::collections::HashMap<String, i64> = sqlx::query_as::<
        _,
        (String, i64),
    >(
        "SELECT cart_line_id, discount_minor FROM pos_discount_authorizations WHERE cart_id = ?",
    )
    .bind(&cart.cart_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();

    let unapproved = |line_id: &str, amount: i64| -> bool {
        amount > 0 && approved_discounts.get(line_id).copied() != Some(amount)
    };

    if unapproved("", cart.bill_discount_minor) {
        return Err(AppError::Validation(
            "This bill discount has not been approved. Apply it again from the              discount screen so a manager can authorise it."
                .into(),
        ));
    }
    for line in &active_lines {
        if unapproved(&line.cart_line_id, line.line_discount_minor) {
            return Err(AppError::Validation(format!(
                "The discount on '{}' has not been approved. Apply it again from                  the discount screen so a manager can authorise it.",
                line.product_name
            )));
        }
    }

    // ── Server-side tax recalculation ────────────────────────────────────────
    // Recalculate tax and line totals for every active line using server-side
    // integer arithmetic. This closes the tax-manipulation vector: a compromised
    // frontend could craft arbitrary tax_amount_minor values in the IPC payload.
    // We compute gross, tax, and net from first principles and use those values
    // for all persistence steps, ignoring the frontend-supplied cart totals.
    let mut server_gross: i64 = 0;
    let mut server_line_taxes: Vec<i64> = Vec::with_capacity(active_lines.len());
    let mut server_line_totals: Vec<i64> = Vec::with_capacity(active_lines.len());

    let corrected_price: std::collections::HashMap<usize, i64> =
        price_corrections.into_iter().collect();

    for (i, line) in active_lines.iter().enumerate() {
        let effective_price = corrected_price
            .get(&i)
            .copied()
            .unwrap_or(line.unit_price_minor);
        let subtotal = crate::domain::money::mul_minor_by_qty(effective_price, &line.quantity);
        server_gross += subtotal;
        let discounted = (subtotal - line.line_discount_minor).max(0);
        let tax_amount = if line.tax_inclusive {
            let divisor = 10_000 + line.tax_rate_basis_points;
            (discounted * line.tax_rate_basis_points + divisor / 2) / divisor
        } else {
            crate::domain::money::calc_tax_exclusive(discounted, line.tax_rate_basis_points)
        };
        let line_total = discounted + if line.tax_inclusive { 0 } else { tax_amount };
        server_line_taxes.push(tax_amount);
        server_line_totals.push(line_total);
    }

    let server_tax: i64 = server_line_taxes.iter().sum();
    let server_post_line: i64 = server_line_totals.iter().sum();
    let server_net: i64 = (server_post_line - cart.bill_discount_minor).max(0);
    let server_discount: i64 = cart.discount_total();

    // ── Guard: payment amounts must sum exactly to server_net ───────────────
    // Validated against server-computed net, not frontend-supplied totals.
    // Cash overpayment is captured in tendered_minor/change_minor — NOT in amount_minor.
    let total_paid: i64 = payments.iter().map(|p| p.amount_minor).sum();
    if total_paid != server_net {
        // A repriced line is the likely cause whenever one is present: the
        // catalogue price moved after the item was scanned, so the till is
        // showing a total the server no longer agrees with. Name the item —
        // "payment amounts must sum to net total" sends the cashier hunting
        // through the payment screen for a fault that is in the cart.
        if let Some((&i, &db_price)) = corrected_price.iter().next() {
            let line = &active_lines[i];
            // The numbers and the instruction come first, and the product name
            // last, because this is shown in a single-line banner that drops
            // whatever does not fit. The name was leading it, so on a product
            // like "Rainbow Original Full Cream Evaporated Milk - Preservatives
            // Free, No Added Sugar - 160 ml" the only part a cashier could act
            // on — the new price — was the part that got cut.
            return Err(AppError::Validation(format!(
                "Price changed: {} → {}. Tap Update prices, then take payment. Item: '{}'.",
                money_for_operator(pool, line.unit_price_minor).await,
                money_for_operator(pool, db_price).await,
                line.product_name
            )));
        }
        return Err(AppError::Validation(format!(
            "Payment {} does not match the total {}. Use tendered_minor for cash overpayment.",
            money_for_operator(pool, total_paid).await,
            money_for_operator(pool, server_net).await
        )));
    }

    // Lookup branch + device + cashier in a single round-trip (3 sequential queries → 1 JOIN).
    let ctx_row = sqlx::query(
        "SELECT b.branch_code, b.currency, b.name AS branch_name,
                d.device_code,
                u.display_name AS cashier_name
         FROM branches b
         JOIN devices d ON d.device_id = ?
         JOIN users   u ON u.user_id   = ?
         WHERE b.branch_id = ?",
    )
    .bind(&cart.device_id)
    .bind(&cart.cashier_user_id)
    .bind(&cart.branch_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Branch, device or cashier not found".into()))?;

    let branch_code: String = ctx_row.get("branch_code");
    let currency: String = ctx_row.get("currency");
    let branch_name: String = ctx_row.get("branch_name");
    let device_code: String = ctx_row.get("device_code");
    let cashier_name: String = ctx_row.get("cashier_name");

    let sale_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let business_date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let gross = server_gross;
    let tax = server_tax;
    let discount = server_discount;
    let net = server_net;

    // Open the transaction BEFORE generating the receipt number so that the
    // UPDATE counter and the sale INSERT are atomic. SQLite serialises writers,
    // so no two concurrent transactions can use the same counter value.
    let mut tx = pool.begin().await?;
    let receipt_number =
        next_receipt_number(&mut *tx, &branch_code, &device_code, &cart.device_id).await?;

    sqlx::query(
        "INSERT INTO sales
         (sale_id, receipt_number, branch_id, device_id, origin_device_id, shift_id, cashier_user_id,
          status, gross_total_minor, discount_total_minor, tax_total_minor, net_total_minor,
          currency, business_date, sold_at, created_offline, idempotency_key, sync_status,
          customer_id, is_delivery, created_at, updated_at)
         VALUES (?,?,?,?,?,?,?,'completed',?,?,?,?,?,?,?,?,?,'pending',?,?,?,?)",
    )
    .bind(&sale_id)
    .bind(&receipt_number)
    .bind(&cart.branch_id)
    .bind(&cart.device_id)
    .bind(&cart.device_id)
    .bind(&cart.shift_id)
    .bind(&cart.cashier_user_id)
    .bind(gross)
    .bind(discount)
    .bind(tax)
    .bind(net)
    .bind(&currency)
    .bind(&business_date)
    .bind(&now)
    .bind(created_offline as i64)
    .bind(idempotency_key)
    .bind(customer_id)
    .bind(delivery.is_some() as i64)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await
    // The other half of the replay. The check above runs before the transaction
    // opens, so two submits close enough together can both pass it; the UNIQUE
    // index is what actually decides, and the loser lands here. Rolling back and
    // returning the winner's sale is the same answer, reached a moment later.
    .map_err(|error| {
        if is_duplicate_idempotency_key(&error) {
            AppError::Conflict(DUPLICATE_SALE_MARKER.into())
        } else {
            AppError::from(error)
        }
    })?;

    let mut item_summaries = Vec::new();
    for (i, line) in active_lines.iter().enumerate() {
        let item_id = Ulid::new().to_string();
        let line_tax = server_line_taxes[i];
        let line_total = server_line_totals[i];
        let effective_price = corrected_price
            .get(&i)
            .copied()
            .unwrap_or(line.unit_price_minor);
        let tax_snapshot = serde_json::json!({
            "rule_id": line.tax_rule_id,
            "rate_basis_points": line.tax_rate_basis_points,
            "inclusive": line.tax_inclusive,
        })
        .to_string();
        let cost_minor_snapshot: Option<i64> = match &line.product_id {
            Some(product_id) => {
                sqlx::query_scalar("SELECT cost_minor FROM products WHERE product_id = ?")
                    .bind(product_id)
                    .fetch_optional(&mut *tx)
                    .await?
            }
            None => None,
        };

        sqlx::query(
            "INSERT INTO sale_items
             (sale_item_id, sale_id, origin_device_id, product_id, product_name_snapshot, sku_snapshot,
              barcode_snapshot, quantity, unit_price_minor, line_discount_minor,
              tax_rule_snapshot, tax_amount_minor, line_total_minor, cost_minor_snapshot, note, voided,
              created_at, updated_at)
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,0,?,?)",
        )
        .bind(&item_id)
        .bind(&sale_id)
        .bind(cart.device_id.as_str())
        .bind(&line.product_id)
        .bind(&line.product_name)
        .bind(&line.sku)
        .bind(&line.barcode)
        .bind(&line.quantity)
        .bind(effective_price)
        .bind(line.line_discount_minor)
        .bind(&tax_snapshot)
        .bind(line_tax)
        .bind(line_total)
        .bind(cost_minor_snapshot)
        .bind(&line.note)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;

        item_summaries.push(SaleItemSummary {
            product_name: line.product_name.clone(),
            quantity: line.quantity.clone(),
            unit_price_minor: effective_price,
            line_total_minor: line_total,
            tax_amount_minor: line_tax,
        });
    }

    let mut payment_summaries = Vec::new();
    for payment in &payments {
        if payment.amount_minor <= 0 {
            return Err(AppError::Validation(format!(
                "Payment amount must be positive (got {} for method '{}')",
                payment.amount_minor, payment.method
            )));
        }
        let payment_id = Ulid::new().to_string();
        let change = if payment.method == "cash" {
            let tendered = payment.tendered_minor.unwrap_or(payment.amount_minor);
            if tendered < payment.amount_minor {
                return Err(AppError::Validation(format!(
                    "Cash tendered ({}) is less than payment amount ({})",
                    tendered, payment.amount_minor
                )));
            }
            Some(tendered - payment.amount_minor)
        } else {
            None
        };

        sqlx::query(
            "INSERT INTO payments
             (payment_id, sale_id, origin_device_id, payment_method, amount_minor, currency, status,
              external_reference, tendered_minor, change_minor, recorded_by_user_id, recorded_at,
              created_at, updated_at)
             VALUES (?,?,?,?,?,?,'approved',?,?,?,?,?,?,?)",
        )
        .bind(&payment_id)
        .bind(&sale_id)
        .bind(&cart.device_id)
        .bind(&payment.method)
        .bind(payment.amount_minor)
        .bind(&currency)
        .bind(&payment.external_reference)
        .bind(payment.tendered_minor)
        .bind(change)
        .bind(&cart.cashier_user_id)
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;

        payment_summaries.push(PaymentSummary {
            method: payment.method.clone(),
            amount_minor: payment.amount_minor,
            change_minor: change,
        });
    }

    // ── Atomic stock deduction (within transaction) ───────────────────────────
    // By deducting inside the transaction, we prevent concurrent sales from
    // overselling: if two transactions compete, one will serialize behind the
    // other. The UPDATE's WHERE clause enforces qty >= sold; rows_affected==0
    // means insufficient stock and the whole transaction is rolled back.
    //
    // Pre-batch: collect all product_ids that need tracking in ONE query instead
    // of one SELECT per line (eliminates the N+1 track_inventory check).
    let line_product_ids: Vec<&str> = cart
        .lines
        .iter()
        .filter(|l| !l.voided)
        .filter_map(|l| l.product_id.as_deref())
        .collect();

    let tracked_ids: std::collections::HashSet<String> = if line_product_ids.is_empty() {
        std::collections::HashSet::new()
    } else {
        let placeholders = line_product_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT product_id FROM products WHERE track_inventory = 1 AND product_id IN ({placeholders})"
        );
        let mut q = sqlx::query_scalar::<_, String>(&sql);
        for pid in &line_product_ids {
            q = q.bind(*pid);
        }
        q.fetch_all(&mut *tx).await?.into_iter().collect()
    };

    // Capture post-deduction quantities inside the transaction so movement
    // records (written post-commit) reflect the exact state after this sale.
    let mut captured_qtys: std::collections::HashMap<String, f64> =
        std::collections::HashMap::new();

    for line in cart.lines.iter().filter(|l| !l.voided) {
        let product_id = match &line.product_id {
            Some(id) => id,
            None => continue, // custom items have no product_id — skip stock
        };

        let sold_qty: f64 = line.quantity.parse().unwrap_or(0.0);
        if sold_qty <= 0.0 {
            continue;
        }

        // Use pre-fetched set — no DB round-trip per line.
        if !tracked_ids.contains(product_id.as_str()) {
            continue;
        }

        // Deduct stock atomically within the transaction.
        // When allow_negative_stock is OFF: the WHERE clause requires qty >= sold,
        // so an under-stocked item causes 0 rows_affected and we return an error.
        // When allow_negative_stock is ON: we drop the qty guard so stock can go
        // negative (back-order / temporary oversell scenario).
        let rows = if allow_negative_stock {
            sqlx::query(
                "UPDATE stock_levels
                 SET quantity_on_hand = CAST(CAST(quantity_on_hand AS REAL) - ? AS TEXT),
                     last_movement_at = ?,
                     updated_at       = ?,
                     sync_status      = 'pending'
                 WHERE product_id = ? AND branch_id = ?",
            )
            .bind(sold_qty)
            .bind(&now)
            .bind(&now)
            .bind(product_id)
            .bind(&cart.branch_id)
            .execute(&mut *tx)
            .await?
        } else {
            sqlx::query(
                "UPDATE stock_levels
                 SET quantity_on_hand = CAST(CAST(quantity_on_hand AS REAL) - ? AS TEXT),
                     last_movement_at = ?,
                     updated_at       = ?,
                     sync_status      = 'pending'
                 WHERE product_id = ? AND branch_id = ?
                   AND CAST(quantity_on_hand AS REAL) >= ?",
            )
            .bind(sold_qty)
            .bind(&now)
            .bind(&now)
            .bind(product_id)
            .bind(&cart.branch_id)
            .bind(sold_qty)
            .execute(&mut *tx)
            .await?
        };

        if rows.rows_affected() == 0 {
            // rows_affected == 0 means either:
            //   (a) no stock_level row for this product+branch yet (uninitialized) — allow
            //   (b) allow_negative_stock is OFF and qty was insufficient — block
            let exists: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM stock_levels WHERE product_id = ? AND branch_id = ?",
            )
            .bind(product_id)
            .bind(&cart.branch_id)
            .fetch_optional(&mut *tx)
            .await?;

            if exists.is_some() && !allow_negative_stock {
                return Err(AppError::Validation(format!(
                    "Insufficient stock for '{}'. Please check inventory levels.",
                    line.product_name
                )));
            }
            // No stock record yet (uninitialized product) → allow through.
        }

        // Capture post-deduction quantity while still inside the sale tx
        // so movement records written post-commit use the exact value.
        if tracked_ids.contains(product_id.as_str()) {
            let qty_after: Option<String> = sqlx::query_scalar(
                "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?",
            )
            .bind(product_id)
            .bind(&cart.branch_id)
            .fetch_optional(&mut *tx)
            .await?
            .flatten();
            if let Some(qstr) = qty_after {
                captured_qtys.insert(product_id.clone(), qstr.parse().unwrap_or(0.0));
            }
        }
    }

    // Audit log with SHA-256 hash chain
    let audit_id = Ulid::new().to_string();
    let after_json = serde_json::json!({ "sale_id": &sale_id, "net_total_minor": net }).to_string();
    let prev_hash: String = sqlx::query_scalar(
        "SELECT hash FROM audit_logs
         WHERE device_id = ? AND length(hash) = 64
         ORDER BY created_at DESC, audit_log_id DESC
         LIMIT 1",
    )
    .bind(&cart.device_id)
    .fetch_optional(&mut *tx)
    .await
    .unwrap_or(None)
    .flatten()
    .unwrap_or_default();
    let hash = audit_hash::compute_audit_hash(&audit_hash::AuditHashInput {
        audit_log_id: &audit_id,
        event_type: "sale.created",
        entity_type: "sale",
        entity_id: &sale_id,
        actor_user_id: &cart.cashier_user_id,
        actor_type: "user",
        created_at: &now,
        before_json: None,
        after_json: Some(&after_json),
        reason: None,
        previous_hash: &prev_hash,
    });
    sqlx::query(
        "INSERT INTO audit_logs
         (audit_log_id, event_type, entity_type, entity_id, actor_user_id, actor_type,
          device_id, origin_device_id, branch_id, after_json, created_at, hash, previous_hash)
         VALUES (?,'sale.created','sale',?,?,'user',?,?,?,?,?,?,?)",
    )
    .bind(&audit_id)
    .bind(&sale_id)
    .bind(&cart.cashier_user_id)
    .bind(&cart.device_id)
    .bind(&cart.device_id)
    .bind(&cart.branch_id)
    .bind(&after_json)
    .bind(&now)
    .bind(&hash)
    .bind(if prev_hash.is_empty() {
        None
    } else {
        Some(prev_hash.clone())
    })
    .execute(&mut *tx)
    .await?;

    // ── Optional: create delivery order in same transaction ──────────────────
    let delivery_row = if let Some(ref d_input) = delivery {
        let row = delivery_repo::create_delivery_in_tx(
            &mut tx,
            &sale_id,
            &receipt_number,
            net,
            &currency,
            d_input,
            &cart.cashier_user_id,
            &cart.branch_id,
            &cart.device_id,
            &now,
        )
        .await?;
        Some(row)
    } else {
        None
    };

    // Add loyalty points inside the sale transaction. If this write fails, the
    // sale must roll back too; otherwise customers can permanently lose points
    // after a crash or DB error between sale commit and post-commit update.
    if let Some(cid) = customer_id {
        let points = net / 1000;
        if points > 0 {
            // Recorded as an event rather than added to a counter. Two tills
            // serving the same customer in one shift used to each compute a
            // total from what they could see, and the merge kept the larger —
            // so the smaller award was simply lost. Events add up regardless of
            // the order they arrive in.
            crate::db::repositories::loyalty_repo::record_tx(
                &mut tx,
                crate::db::repositories::loyalty_repo::AwardContext {
                    customer_id: cid,
                    branch_id: Some(&cart.branch_id),
                    device_id: Some(&cart.device_id),
                    event: crate::db::repositories::loyalty_repo::LoyaltyEvent::Earn,
                    points_delta: points,
                    reference_type: Some("sale"),
                    reference_id: Some(&sale_id),
                    reason: None,
                    actor_user_id: Some(&cart.cashier_user_id),
                },
            )
            .await?;
        }
    }

    sqlx::query("DELETE FROM pos_price_overrides WHERE cart_id = ?")
        .bind(&cart.cart_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM pos_discount_authorizations WHERE cart_id = ?")
        .bind(&cart.cart_id)
        .execute(&mut *tx)
        .await?;

    // The ledger entries for what this sale took off the shelf, written in the
    // same transaction as the deduction itself. They used to go in after the
    // commit, in a transaction of their own, so a failure there left the shelf
    // count reduced with nothing in the ledger to explain it — and nothing ever
    // retried. Now the sale and its movements stand or fall together.
    movements::record_sale_movements_tx(
        &mut tx,
        &sale_id,
        &cart.cashier_user_id,
        &cart.branch_id,
        &cart.device_id,
        &captured_qtys,
    )
    .await?;

    tx.commit().await?;
    tracing::info!("Sale finalized: {} ({})", sale_id, receipt_number);

    // Low-stock alerts are for the cashier's screen, not for the books, so they
    // are read after the commit where a failure costs nothing.
    let low_stock_alerts = movements::low_stock_after_sale(pool, &captured_qtys).await;

    Ok(SaleResult {
        sale_id,
        receipt_number,
        net_total_minor: net,
        tax_total_minor: tax,
        discount_total_minor: discount,
        currency,
        payments: payment_summaries,
        items: item_summaries,
        cashier_name,
        branch_name,
        sold_at: now,
        business_date,
        created_offline,
        low_stock_alerts,
        delivery: delivery_row,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Integration tests — run with `cargo test -p zanpos-tauri`
// Uses an in-memory SQLite DB seeded by the real migration chain.
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::cart::{Cart, CartLine};
    use sqlx::sqlite::SqlitePoolOptions;

    // Seed IDs that match 0001_initial.sql
    const BRANCH: &str = "01JBRANCH0000000000000001";
    const DEVICE: &str = "01JDEVICE0000000000000001";
    const CASHIER: &str = "01JUSER000000000000CASH01";
    const TAX_VAT: &str = "01JTAX000000000000VAT001"; // 10% exclusive (1 000 bp)
    const TAX_ZER: &str = "01JTAX000000000000ZERO01"; // 0%

    /// Money that leaves this module for a screen must be readable as money.
    ///
    /// The reported defect: the price-change banner printed `db_price` — fils —
    /// straight into the sentence, so an item that had changed to BHD 0.250 was
    /// announced as having "changed to 250". A cashier reads that as a thousand
    /// times the real price, and the sentence was truncated by the banner
    /// anyway, so what actually reached the screen was "changed to 2…".
    #[tokio::test]
    async fn operator_facing_money_is_formatted_not_raw_fils() {
        let pool = make_pool().await;

        assert_eq!(money_for_operator(&pool, 250).await, "BHD 0.250");
        assert_eq!(money_for_operator(&pool, 2_500).await, "BHD 2.500");
        assert_eq!(money_for_operator(&pool, 0).await, "BHD 0.000");
        // Three decimals is the whole point in Bahrain: 250 fils is a quarter
        // dinar, not two hundred and fifty of anything.
        assert!(!money_for_operator(&pool, 250).await.contains("250 "));
    }

    /// The message has to survive a single-line banner that drops the overflow.
    /// The product that exposed this has a 78-character name, and the price —
    /// the only part a cashier can act on — used to sit after it.
    #[tokio::test]
    async fn the_price_change_message_leads_with_the_numbers() {
        let pool = make_pool().await;
        let long_name = "Rainbow Original Full Cream Evaporated Milk - \
                         Preservatives Free, No Added Sugar - 160 ml";
        let message = format!(
            "Price changed: {} → {}. Tap Update prices, then take payment. Item: '{}'.",
            money_for_operator(&pool, 200).await,
            money_for_operator(&pool, 250).await,
            long_name
        );

        // Both prices are readable within the first 60 characters, which is
        // roughly what fits before the banner truncates.
        let head = &message[..60.min(message.len())];
        assert!(head.contains("BHD 0.200"), "{head}");
        assert!(head.contains("BHD 0.250"), "{head}");
        assert!(
            message.find("Update prices").unwrap() < message.find("Rainbow").unwrap(),
            "the instruction must come before the name"
        );
        // The remedy names the control the cashier can actually press. It used
        // to say "remove it and scan it again", which is busywork the till can
        // do itself — and on a full basket, a lot of it.
        assert!(message.contains("Update prices"), "{message}");
    }

    // Build an in-memory pool and run all migrations.
    async fn make_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations");
        // 0018_remove_demo_data.sql deleted the sample products; re-seed the two
        // products this test module needs so FK constraints are satisfied.
        // Activate the seed device and branch (seeded inactive by 0011_seed.sql)
        sqlx::query(
            "UPDATE devices SET is_active = 1 WHERE device_id = '01JDEVICE0000000000000001'",
        )
        .execute(&pool)
        .await
        .ok();
        sqlx::query(
            "UPDATE branches SET is_active = 1 WHERE branch_id = '01JBRANCH0000000000000001'",
        )
        .execute(&pool)
        .await
        .ok();

        // Seed tax rules needed by the test products
        sqlx::query(
            "INSERT OR IGNORE INTO tax_rules (tax_rule_id, name, rate_basis_points, inclusive, is_active, effective_from, created_at, updated_at, version)
             VALUES
             ('01JTAX000000000000VAT001', 'VAT 10%', 1000, 0, 1, datetime('now'), datetime('now'), datetime('now'), 1),
             ('01JTAX000000000000ZERO01', 'Zero-rated', 0, 0, 1, datetime('now'), datetime('now'), datetime('now'), 1)"
        ).execute(&pool).await.expect("seed test tax rules");

        // Seed cashier user (needed by test sales)
        sqlx::query(
            "INSERT OR IGNORE INTO users (user_id, branch_id, display_name, username, pin_hash, role_id, is_active, created_at, updated_at, version)
             VALUES ('01JUSER000000000000CASH01', '01JBRANCH0000000000000001', 'Test Cashier', 'cashier_test', 'PLAIN:1234', '01JROLES000000000000000003', 1, datetime('now'), datetime('now'), 1)"
        ).execute(&pool).await.expect("seed test cashier");

        sqlx::query(
            "INSERT OR IGNORE INTO categories (category_id, name, sort_order, is_active, created_at, updated_at, version)
             VALUES ('01JCAT000000000000DRINK01', 'Drinks', 1, 1, datetime('now'), datetime('now'), 1)"
        ).execute(&pool).await.expect("seed test category");

        sqlx::query(
            "INSERT OR IGNORE INTO products
             (product_id, category_id, name, sku, barcode, description, track_inventory, allow_decimal_quantity, is_active, tax_rule_id, cost_minor, currency, reorder_point, created_at, updated_at, version)
             VALUES
             ('01JPROD00000000000COLA001', '01JCAT000000000000DRINK01', 'Coca-Cola 330ml', 'COLA-330', '5449000000996', NULL, 1, 0, 1, '01JTAX000000000000VAT001', 100, 'BHD', 0, datetime('now'), datetime('now'), 1),
             ('01JPROD00000000000WATR001', '01JCAT000000000000DRINK01', 'Water 500ml',     'WATR-500', '6281001511222', NULL, 1, 0, 1, '01JTAX000000000000ZERO01', 50,  'BHD', 0, datetime('now'), datetime('now'), 1)"
        ).execute(&pool).await.expect("seed test products");

        sqlx::query(
            "INSERT OR IGNORE INTO stock_levels (stock_level_id, product_id, branch_id, quantity_on_hand, updated_at, created_at, sync_status, sync_attempts)
             VALUES
             ('SL-TEST-COLA', '01JPROD00000000000COLA001', '01JBRANCH0000000000000001', '1000', datetime('now'), datetime('now'), 'synced', 0),
             ('SL-TEST-WATR', '01JPROD00000000000WATR001', '01JBRANCH0000000000000001', '1000', datetime('now'), datetime('now'), 'synced', 0)"
        ).execute(&pool).await.expect("seed test stock");

        pool
    }

    // Insert a minimal open shift so sales FK is satisfied.
    async fn insert_shift(pool: &SqlitePool) -> String {
        let shift_id = ulid::Ulid::new().to_string();
        sqlx::query(
            "INSERT INTO shifts (shift_id, branch_id, device_id, origin_device_id, cashier_user_id, opened_at, status, created_at, updated_at, version, sync_status, sync_attempts)
             VALUES (?, ?, ?, ?, ?, datetime('now'), 'open', datetime('now'), datetime('now'), 1, 'pending', 0)"
        )
        .bind(&shift_id).bind(BRANCH).bind(DEVICE).bind(DEVICE).bind(CASHIER)
        .execute(pool).await.expect("insert shift");
        shift_id
    }

    // Build a CartLine with explicit tax fields already calculated.
    fn cola_line(qty: &str) -> CartLine {
        // Cola: 400 minor, 10% exclusive VAT (1 000 bp)
        // Use integer arithmetic via mul_minor_by_qty — no float round-trips on money.
        let subtotal = crate::domain::money::mul_minor_by_qty(400, qty);
        let tax = subtotal * 1_000 / 10_000;
        CartLine {
            cart_line_id: ulid::Ulid::new().to_string(),
            product_id: Some("01JPROD00000000000COLA001".into()),
            product_name: "Coca-Cola 330ml".into(),
            sku: Some("COLA330".into()),
            barcode: None,
            image_path: None,
            quantity: qty.to_string(),
            unit_price_minor: 400,
            line_discount_minor: 0,
            line_discount_reason: None,
            tax_rule_id: TAX_VAT.to_string(),
            tax_rate_basis_points: 1_000,
            tax_inclusive: false,
            tax_amount_minor: tax,
            line_total_minor: subtotal + tax,
            note: None,
            voided: false,
        }
    }

    fn water_line(qty: &str) -> CartLine {
        // Water: 250 minor, zero-rated (0 bp)
        // Use integer arithmetic via mul_minor_by_qty — no float round-trips on money.
        let subtotal = crate::domain::money::mul_minor_by_qty(250, qty);
        CartLine {
            cart_line_id: ulid::Ulid::new().to_string(),
            product_id: Some("01JPROD00000000000WATR001".into()),
            product_name: "Water 500ml".into(),
            sku: Some("WATR500".into()),
            barcode: None,
            image_path: None,
            quantity: qty.to_string(),
            unit_price_minor: 250,
            line_discount_minor: 0,
            line_discount_reason: None,
            tax_rule_id: TAX_ZER.to_string(),
            tax_rate_basis_points: 0,
            tax_inclusive: false,
            tax_amount_minor: 0,
            line_total_minor: subtotal,
            note: None,
            voided: false,
        }
    }

    // ── 1. Happy path: 2× Cola, 10% VAT, cash with change ────────────────────
    // Cola 400 × 2 = 800 subtotal; tax 80; line_total 880; pay 1 000 → change 120
    #[tokio::test]
    async fn test_finalize_sale_happy_path() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        let mut cart = Cart::new(BRANCH.into(), DEVICE.into(), shift_id, CASHIER.into());
        cart.lines.push(cola_line("2"));

        let payments = vec![PaymentInput {
            method: "cash".into(),
            amount_minor: 880,
            tendered_minor: Some(1_000),
            external_reference: None,
        }];

        let result = finalize_sale(&pool, &cart, payments, "idem-001", None, false, None, false)
            .await
            .expect("finalize_sale");

        assert_eq!(result.net_total_minor, 880);
        assert_eq!(result.tax_total_minor, 80);
        assert_eq!(result.discount_total_minor, 0);
        assert!(!result.sale_id.is_empty());
        assert!(!result.receipt_number.is_empty());
        assert_eq!(result.payments.len(), 1);
        assert_eq!(result.payments[0].change_minor, Some(120));
        assert_eq!(result.items.len(), 1);
    }

    #[tokio::test]
    async fn test_finalize_sale_honors_recorded_price_override() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        sqlx::query(
            "INSERT INTO product_prices
             (price_id, product_id, price_minor, currency, effective_from, created_by_user_id, created_at)
             VALUES ('PRICE-COLA', '01JPROD00000000000COLA001', 400, 'BHD', datetime('now', '-1 minute'), ?, datetime('now'))",
        )
        .bind(CASHIER)
        .execute(&pool)
        .await
        .expect("seed catalogue price");

        let mut cart = Cart::new(BRANCH.into(), DEVICE.into(), shift_id, CASHIER.into());
        let mut line = cola_line("1");
        line.unit_price_minor = 500;
        line.recalculate();

        sqlx::query(
            "INSERT INTO pos_price_overrides
             (cart_id, cart_line_id, product_id, price_minor, authorized_by_user_id, created_at)
             VALUES (?, ?, ?, ?, ?, datetime('now'))",
        )
        .bind(&cart.cart_id)
        .bind(&line.cart_line_id)
        .bind(&line.product_id)
        .bind(line.unit_price_minor)
        .bind(CASHIER)
        .execute(&pool)
        .await
        .expect("record approved override");
        cart.lines.push(line);

        let payments = vec![PaymentInput {
            method: "cash".into(),
            amount_minor: 550,
            tendered_minor: Some(550),
            external_reference: None,
        }];

        let result = finalize_sale(
            &pool,
            &cart,
            payments,
            "idem-price-override",
            None,
            false,
            None,
            false,
        )
        .await
        .expect("approved price override must finalize");

        assert_eq!(result.net_total_minor, 550);
        assert_eq!(result.items[0].unit_price_minor, 500);
        let remaining: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM pos_price_overrides WHERE cart_id = ?")
                .bind(&cart.cart_id)
                .fetch_one(&pool)
                .await
                .expect("count consumed overrides");
        assert_eq!(remaining, 0, "a completed sale consumes its override proof");
    }

    // Regression: an admin price change blocked every subsequent sale of that
    // product with "Payment amounts must sum exactly to net total".
    //
    // product_prices.effective_from is written in two formats by different
    // paths: `datetime('now')` ("2026-08-18 05:33:00") by the importer, and
    // `Utc::now().to_rfc3339()` ("2026-08-18T05:33:00.123+00:00") by the admin
    // edit. Compared as raw text, 'T' (0x54) sorts above ' ' (0x20), so an
    // RFC3339 row never satisfies `effective_from <= datetime('now')` and a
    // closed row's RFC3339 effective_to still looks open. The till priced the
    // cart through product_repo, which wraps both sides in datetime(), so the
    // two disagreed and the guard rejected a correct payment. Both sides must
    // normalise before comparing.
    #[tokio::test]
    async fn test_finalize_sale_accepts_price_changed_through_admin() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        // Original price, written the way the catalogue importer writes it.
        sqlx::query(
            "INSERT INTO product_prices
             (price_id, product_id, price_type, price_minor, currency, effective_from, created_by_user_id, created_at)
             VALUES ('PRICE-COLA-OLD', '01JPROD00000000000COLA001', 'selling', 400, 'BHD', datetime('now', '-1 day'), ?, datetime('now', '-1 day'))",
        )
        .bind(CASHIER)
        .execute(&pool)
        .await
        .expect("seed original catalogue price");

        // Admin raises the price: close the old row, open a new one — both
        // stamped RFC3339, exactly as admin_commands::product_update does.
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE product_prices SET effective_to = ?
             WHERE product_id = '01JPROD00000000000COLA001' AND effective_to IS NULL",
        )
        .bind(&now)
        .execute(&pool)
        .await
        .expect("close old price");
        sqlx::query(
            "INSERT INTO product_prices
             (price_id, product_id, price_type, price_minor, currency, effective_from, created_by_user_id, created_at)
             VALUES ('PRICE-COLA-NEW', '01JPROD00000000000COLA001', 'selling', 500, 'BHD', ?, ?, ?)",
        )
        .bind(&now)
        .bind(CASHIER)
        .bind(&now)
        .execute(&pool)
        .await
        .expect("insert new catalogue price");

        // Cashier scans the item after the change; the till reads 500.
        let mut cart = Cart::new(BRANCH.into(), DEVICE.into(), shift_id, CASHIER.into());
        let mut line = cola_line("1");
        line.unit_price_minor = 500;
        line.recalculate();
        cart.lines.push(line);

        let payments = vec![PaymentInput {
            method: "cash".into(),
            amount_minor: 550, // 500 + 10% VAT
            tendered_minor: Some(550),
            external_reference: None,
        }];

        let result = finalize_sale(
            &pool,
            &cart,
            payments,
            "idem-price-change",
            None,
            false,
            None,
            false,
        )
        .await
        .expect("a sale at the newly set catalogue price must finalize");

        assert_eq!(result.net_total_minor, 550);
        assert_eq!(result.items[0].unit_price_minor, 500);
    }

    // The closed row must lose even when it sorts above the open one as raw
    // text — importer format ("2026-08-18 06:00:00") beats RFC3339 of the same
    // instant, so picking the newest price by string comparison picks the
    // superseded one.
    #[tokio::test]
    async fn test_finalize_sale_ignores_superseded_price_row() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        sqlx::query(
            "INSERT INTO product_prices
             (price_id, product_id, price_type, price_minor, currency, effective_from, effective_to, created_by_user_id, created_at)
             VALUES ('PRICE-COLA-OLD', '01JPROD00000000000COLA001', 'selling', 400, 'BHD',
                     datetime('now', '-1 hour'), datetime('now', '-30 minutes'), ?, datetime('now', '-1 hour'))",
        )
        .bind(CASHIER)
        .execute(&pool)
        .await
        .expect("seed superseded price");
        sqlx::query(
            "INSERT INTO product_prices
             (price_id, product_id, price_type, price_minor, currency, effective_from, created_by_user_id, created_at)
             VALUES ('PRICE-COLA-NEW', '01JPROD00000000000COLA001', 'selling', 500, 'BHD', ?, ?, ?)",
        )
        .bind(
            (chrono::Utc::now() - chrono::Duration::minutes(30))
                .to_rfc3339(),
        )
        .bind(CASHIER)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .expect("seed current price");

        let mut cart = Cart::new(BRANCH.into(), DEVICE.into(), shift_id, CASHIER.into());
        let mut line = cola_line("1");
        line.unit_price_minor = 500;
        line.recalculate();
        cart.lines.push(line);

        let payments = vec![PaymentInput {
            method: "cash".into(),
            amount_minor: 550,
            tendered_minor: Some(550),
            external_reference: None,
        }];

        let result = finalize_sale(
            &pool,
            &cart,
            payments,
            "idem-superseded-price",
            None,
            false,
            None,
            false,
        )
        .await
        .expect("the open price row wins over the closed one");

        assert_eq!(result.items[0].unit_price_minor, 500);
    }

    // A price that genuinely moves while the item sits in the cart still has to
    // be rejected, but the cashier is told which item to re-scan.
    #[tokio::test]
    async fn test_finalize_sale_names_the_repriced_item() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        sqlx::query(
            "INSERT INTO product_prices
             (price_id, product_id, price_type, price_minor, currency, effective_from, created_by_user_id, created_at)
             VALUES ('PRICE-COLA', '01JPROD00000000000COLA001', 'selling', 450, 'BHD', datetime('now', '-1 minute'), ?, datetime('now'))",
        )
        .bind(CASHIER)
        .execute(&pool)
        .await
        .expect("seed catalogue price");

        // Cart still holds the pre-change 400 with no manager override.
        let mut cart = Cart::new(BRANCH.into(), DEVICE.into(), shift_id, CASHIER.into());
        cart.lines.push(cola_line("1"));

        let payments = vec![PaymentInput {
            method: "cash".into(),
            amount_minor: 440,
            tendered_minor: Some(440),
            external_reference: None,
        }];

        let err = finalize_sale(
            &pool,
            &cart,
            payments,
            "idem-stale-price",
            None,
            false,
            None,
            false,
        )
        .await
        .expect_err("a stale cart price must not finalize");

        let msg = err.to_string();
        assert!(
            msg.contains("Coca-Cola 330ml"),
            "message names the item: {msg}"
        );
        assert!(msg.contains("450"), "message gives the new price: {msg}");
    }

    // A 'cost' row must never be mistaken for the shelf price.
    #[tokio::test]
    async fn test_finalize_sale_ignores_non_selling_price_rows() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        sqlx::query(
            "INSERT INTO product_prices
             (price_id, product_id, price_type, price_minor, currency, effective_from, created_by_user_id, created_at)
             VALUES
             ('PRICE-COLA-SELL', '01JPROD00000000000COLA001', 'selling', 400, 'BHD', datetime('now', '-1 hour'), ?, datetime('now')),
             ('PRICE-COLA-COST', '01JPROD00000000000COLA001', 'cost',    100, 'BHD', datetime('now'),            ?, datetime('now'))",
        )
        .bind(CASHIER)
        .bind(CASHIER)
        .execute(&pool)
        .await
        .expect("seed selling and cost rows");

        let mut cart = Cart::new(BRANCH.into(), DEVICE.into(), shift_id, CASHIER.into());
        cart.lines.push(cola_line("1"));

        let payments = vec![PaymentInput {
            method: "cash".into(),
            amount_minor: 440,
            tendered_minor: Some(440),
            external_reference: None,
        }];

        let result = finalize_sale(
            &pool,
            &cart,
            payments,
            "idem-cost-row",
            None,
            false,
            None,
            false,
        )
        .await
        .expect("the cost row must not price the sale");

        assert_eq!(result.items[0].unit_price_minor, 400);
    }

    #[tokio::test]
    async fn test_finalize_sale_snapshots_product_cost() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        let mut cart = Cart::new(BRANCH.into(), DEVICE.into(), shift_id, CASHIER.into());
        cart.lines.push(cola_line("2"));

        let payments = vec![PaymentInput {
            method: "cash".into(),
            amount_minor: 880,
            tendered_minor: Some(880),
            external_reference: None,
        }];

        let result = finalize_sale(
            &pool,
            &cart,
            payments,
            "idem-cost-snapshot",
            None,
            false,
            None,
            false,
        )
        .await
        .expect("finalize sale");

        sqlx::query("UPDATE products SET cost_minor = 999 WHERE product_id = ?")
            .bind("01JPROD00000000000COLA001")
            .execute(&pool)
            .await
            .expect("update current product cost");

        let snapshot: Option<i64> =
            sqlx::query_scalar("SELECT cost_minor_snapshot FROM sale_items WHERE sale_id = ?")
                .bind(&result.sale_id)
                .fetch_one(&pool)
                .await
                .expect("sale item cost snapshot");

        assert_eq!(
            snapshot,
            Some(100),
            "sale item must preserve COGS from the moment of sale"
        );
    }

    // ── 2. Under-payment is rejected before any DB write ─────────────────────
    #[tokio::test]
    async fn test_finalize_sale_underpay_rejected() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        let mut cart = Cart::new(BRANCH.into(), DEVICE.into(), shift_id, CASHIER.into());
        cart.lines.push(cola_line("1")); // net = 440

        let payments = vec![PaymentInput {
            method: "cash".into(),
            amount_minor: 400, // < 440 → underpay
            tendered_minor: Some(400),
            external_reference: None,
        }];

        let err = finalize_sale(
            &pool,
            &cart,
            payments,
            "idem-underpay",
            None,
            false,
            None,
            false,
        )
        .await
        .unwrap_err();

        assert!(
            matches!(err, AppError::Validation(_)),
            "expected Validation error, got {err:?}"
        );

        // Confirm no sale was written
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sales")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "no sale should be persisted on underpay");
    }

    // ── 3. A repeated idempotency key replays the original sale ───────────────
    //
    // This asserted a `Database` unique-constraint error, which is what the code
    // used to do and is the wrong answer for an idempotency key. The POS sends
    // the cart id, so a double-pressed Charge button sent the same key twice and
    // the cashier was shown "Sale failed" for a sale that had just gone through.
    // Rebuilding the cart to try again mints a new id, and that is how one basket
    // becomes two sales. The second call now returns the first sale.
    #[tokio::test]
    async fn test_finalize_sale_idempotency_key_replays() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        let payments = || {
            vec![PaymentInput {
                method: "cash".into(),
                amount_minor: 440,
                tendered_minor: Some(440),
                external_reference: None,
            }]
        };

        let mut cart1 = Cart::new(
            BRANCH.into(),
            DEVICE.into(),
            shift_id.clone(),
            CASHIER.into(),
        );
        cart1.lines.push(cola_line("1"));
        let first = finalize_sale(
            &pool,
            &cart1,
            payments(),
            "idem-dup",
            None,
            false,
            None,
            false,
        )
        .await
        .expect("first sale");

        let mut cart2 = Cart::new(BRANCH.into(), DEVICE.into(), shift_id, CASHIER.into());
        cart2.lines.push(cola_line("1"));
        let replayed = finalize_sale(
            &pool,
            &cart2,
            payments(),
            "idem-dup",
            None,
            false,
            None,
            false,
        )
        .await
        .expect("a repeated key must return the sale it already made");

        assert_eq!(
            replayed.receipt_number, first.receipt_number,
            "the replay issued a second receipt number"
        );
        assert_eq!(replayed.sale_id, first.sale_id);

        // And exactly one sale exists, whatever the second call returned.
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sales")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1, "the repeated key created a second sale");
    }

    #[tokio::test]
    async fn test_finalize_sale_rolls_back_when_loyalty_update_fails() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;
        let customer_id = "01JCUST000000000000000001";

        sqlx::query(
            "INSERT INTO customers
             (customer_id, branch_id, origin_device_id, name, loyalty_points, created_at, updated_at, sync_status, sync_attempts)
             VALUES (?, ?, ?, 'Loyalty Customer', 0, datetime('now'), datetime('now'), 'synced', 0)",
        )
        .bind(customer_id)
        .bind(BRANCH)
        .bind(DEVICE)
        .execute(&pool)
        .await
        .expect("seed customer");

        sqlx::query(
            "CREATE TRIGGER fail_loyalty_update
             BEFORE UPDATE OF loyalty_points ON customers
             BEGIN
               SELECT RAISE(FAIL, 'loyalty update blocked');
             END",
        )
        .execute(&pool)
        .await
        .expect("create loyalty failure trigger");

        let mut cart = Cart::new(BRANCH.into(), DEVICE.into(), shift_id, CASHIER.into());
        cart.lines.push(water_line("4")); // 1.000 BHD = 1 loyalty point

        let payments = vec![PaymentInput {
            method: "cash".into(),
            amount_minor: 1_000,
            tendered_minor: Some(1_000),
            external_reference: None,
        }];

        let err = finalize_sale(
            &pool,
            &cart,
            payments,
            "idem-loyalty-rollback",
            Some(customer_id),
            false,
            None,
            false,
        )
        .await
        .expect_err("loyalty failure must fail the sale transaction");

        assert!(
            matches!(err, AppError::Database(_)),
            "expected database error from loyalty trigger, got {err:?}"
        );

        let sale_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sales WHERE idempotency_key = 'idem-loyalty-rollback'",
        )
        .fetch_one(&pool)
        .await
        .expect("sale count");
        assert_eq!(sale_count, 0, "sale must roll back with loyalty update");
    }

    // ── 4. Zero-tax (zero-rated) item: no tax charged ─────────────────────────
    #[tokio::test]
    async fn test_finalize_sale_zero_tax_item() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        let mut cart = Cart::new(BRANCH.into(), DEVICE.into(), shift_id, CASHIER.into());
        cart.lines.push(water_line("4")); // 4 × 250 = 1 000, zero-rated

        let payments = vec![PaymentInput {
            method: "cash".into(),
            amount_minor: 1_000,
            tendered_minor: Some(1_000),
            external_reference: None,
        }];

        let result = finalize_sale(
            &pool,
            &cart,
            payments,
            "idem-water",
            None,
            false,
            None,
            false,
        )
        .await
        .expect("zero-tax sale");

        assert_eq!(result.net_total_minor, 1_000);
        assert_eq!(result.tax_total_minor, 0, "zero-rated items carry no tax");
    }

    /// Stand-in for a manager approving a discount at the till.
    ///
    /// `finalize_sale` refuses a discount with no row in
    /// `pos_discount_authorizations`: a discount arriving on the cart is not
    /// authorisation, which is the whole point of the table. Tests that exercise
    /// discount arithmetic record the approval the way `pos_apply_*_discount`
    /// does.
    async fn approve_discount(pool: &SqlitePool, cart_id: &str, line_id: &str, minor: i64) {
        sqlx::query(
            "INSERT INTO pos_discount_authorizations
               (cart_id, cart_line_id, discount_minor, reason, authorized_by_user_id, created_at)
             VALUES (?, ?, ?, 'test', ?, datetime('now'))
             ON CONFLICT(cart_id, cart_line_id) DO UPDATE SET
               discount_minor = excluded.discount_minor",
        )
        .bind(cart_id)
        .bind(line_id)
        .bind(minor)
        .bind(CASHIER)
        .execute(pool)
        .await
        .expect("record the manager approval");
    }

    // ── 5. Line discount reduces net total ────────────────────────────────────
    #[tokio::test]
    async fn test_finalize_sale_with_line_discount() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        let mut cart = Cart::new(BRANCH.into(), DEVICE.into(), shift_id, CASHIER.into());
        let mut line = cola_line("1"); // 400 minor, no tax for simplicity — override to zero-rated
        line.tax_rule_id = TAX_ZER.to_string();
        line.tax_rate_basis_points = 0;
        line.tax_amount_minor = 0;
        line.line_total_minor = 400;
        line.line_discount_minor = 100; // discount 100 minor
        line.line_total_minor = 300; // after discount
        let line_id = line.cart_line_id.clone();
        cart.lines.push(line);
        approve_discount(&pool, &cart.cart_id, &line_id, 100).await;

        let payments = vec![PaymentInput {
            method: "cash".into(),
            amount_minor: 300,
            tendered_minor: Some(300),
            external_reference: None,
        }];

        let result = finalize_sale(
            &pool,
            &cart,
            payments,
            "idem-discount",
            None,
            false,
            None,
            false,
        )
        .await
        .expect("discounted sale");

        assert_eq!(result.net_total_minor, 300);
        assert_eq!(result.discount_total_minor, 100);
    }

    // ── 6. Split payment (cash + card) ────────────────────────────────────────
    #[tokio::test]
    async fn test_finalize_sale_split_payment() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        let mut cart = Cart::new(BRANCH.into(), DEVICE.into(), shift_id, CASHIER.into());
        // 2× Cola (880) + 2× Water (500) = 1 380 total; tax = 80
        cart.lines.push(cola_line("2"));
        cart.lines.push(water_line("2"));

        let payments = vec![
            PaymentInput {
                method: "cash".into(),
                amount_minor: 880,
                tendered_minor: Some(880),
                external_reference: None,
            },
            PaymentInput {
                method: "card".into(),
                amount_minor: 500,
                tendered_minor: None,
                external_reference: Some("TXN-999".into()),
            },
        ];

        let result = finalize_sale(
            &pool,
            &cart,
            payments,
            "idem-split",
            None,
            false,
            None,
            false,
        )
        .await
        .expect("split-payment sale");

        assert_eq!(result.net_total_minor, 1_380);
        assert_eq!(result.payments.len(), 2);

        let cash_pay = result.payments.iter().find(|p| p.method == "cash").unwrap();
        let card_pay = result.payments.iter().find(|p| p.method == "card").unwrap();
        assert_eq!(cash_pay.change_minor, Some(0));
        assert!(card_pay.change_minor.is_none());
    }

    // ── T8. Concurrent sales for qty=1 item: only one succeeds ──────────────
    // Seeds stock_level = 1, then issues two sequential sales (simulating
    // near-concurrent access in a single-threaded test). The atomic UPDATE in
    // finalize_sale ensures the second sale cannot deplete stock below zero.
    #[tokio::test]
    async fn test_stock_prevents_oversell() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;
        let product_id = "01JPROD00000000000COLA001";
        let branch_id = BRANCH;

        // Seed exactly 1 unit in stock
        sqlx::query(
            "INSERT INTO stock_levels
             (stock_level_id, product_id, branch_id, quantity_on_hand, created_at, updated_at)
             VALUES ('SL-COLA', ?, ?, '1', datetime('now'), datetime('now'))
             ON CONFLICT(product_id, branch_id)
             DO UPDATE SET quantity_on_hand = '1', updated_at = datetime('now')",
        )
        .bind(product_id)
        .bind(branch_id)
        .execute(&pool)
        .await
        .expect("seed stock");

        let payment_for_cola = || {
            vec![PaymentInput {
                method: "cash".into(),
                amount_minor: 440,
                tendered_minor: Some(440),
                external_reference: None,
            }]
        };

        // First sale — should succeed (stock 1 → 0)
        let mut cart1 = Cart::new(
            BRANCH.into(),
            DEVICE.into(),
            shift_id.clone(),
            CASHIER.into(),
        );
        cart1.lines.push(cola_line("1"));
        let r1 = finalize_sale(
            &pool,
            &cart1,
            payment_for_cola(),
            "idem-t8-first",
            None,
            false,
            None,
            false,
        )
        .await;
        assert!(r1.is_ok(), "first sale must succeed with stock=1: {r1:?}");

        // Second sale — must fail; stock is now 0
        let mut cart2 = Cart::new(
            BRANCH.into(),
            DEVICE.into(),
            shift_id.clone(),
            CASHIER.into(),
        );
        cart2.lines.push(cola_line("1"));
        let r2 = finalize_sale(
            &pool,
            &cart2,
            payment_for_cola(),
            "idem-t8-second",
            None,
            false,
            None,
            false,
        )
        .await;
        assert!(
            matches!(r2, Err(AppError::Validation(_))),
            "second sale must be rejected when stock=0: got {r2:?}"
        );
    }

    // ── 7. allow_negative_stock=true lets a sale proceed below zero ────────────
    #[tokio::test]
    async fn test_allow_negative_stock_sells_through_zero() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;
        let product_id = "01JPROD00000000000COLA001";
        let branch_id = BRANCH;

        // Seed stock at exactly 0 — would normally block a sale
        sqlx::query(
            "INSERT INTO stock_levels
             (stock_level_id, product_id, branch_id, quantity_on_hand, created_at, updated_at)
             VALUES ('SL-COLA-NEG', ?, ?, '0', datetime('now'), datetime('now'))
             ON CONFLICT(product_id, branch_id)
             DO UPDATE SET quantity_on_hand = '0', updated_at = datetime('now')",
        )
        .bind(product_id)
        .bind(branch_id)
        .execute(&pool)
        .await
        .expect("seed zero stock");

        let payment = vec![PaymentInput {
            method: "cash".into(),
            amount_minor: 440,
            tendered_minor: Some(440),
            external_reference: None,
        }];

        let mut cart = Cart::new(
            BRANCH.into(),
            DEVICE.into(),
            shift_id.clone(),
            CASHIER.into(),
        );
        cart.lines.push(cola_line("1"));

        // With allow_negative_stock=true the sale must succeed even at stock=0
        let result = finalize_sale(
            &pool,
            &cart,
            payment,
            "idem-neg-stock",
            None,
            false,
            None,
            true,
        )
        .await;
        assert!(
            result.is_ok(),
            "sale must succeed with allow_negative_stock=true even when stock=0: {result:?}"
        );

        // Stock should now be -1
        let qty: String = sqlx::query_scalar(
            "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?",
        )
        .bind(product_id)
        .bind(branch_id)
        .fetch_one(&pool)
        .await
        .expect("stock row must exist");

        let qty_f: f64 = qty.parse().expect("quantity_on_hand must be numeric");
        assert!(
            qty_f < 0.0,
            "stock must be negative after oversell with flag ON, got: {qty}"
        );
    }

    // ── 8. Receipt number is sequential per device/branch ────────────────────
    #[tokio::test]
    async fn test_receipt_number_sequential() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        let single_water_payment = || {
            vec![PaymentInput {
                method: "cash".into(),
                amount_minor: 250,
                tendered_minor: Some(250),
                external_reference: None,
            }]
        };

        let mut c1 = Cart::new(
            BRANCH.into(),
            DEVICE.into(),
            shift_id.clone(),
            CASHIER.into(),
        );
        c1.lines.push(water_line("1"));
        let r1 = finalize_sale(
            &pool,
            &c1,
            single_water_payment(),
            "idem-seq1",
            None,
            false,
            None,
            false,
        )
        .await
        .unwrap();

        let mut c2 = Cart::new(
            BRANCH.into(),
            DEVICE.into(),
            shift_id.clone(),
            CASHIER.into(),
        );
        c2.lines.push(water_line("1"));
        let r2 = finalize_sale(
            &pool,
            &c2,
            single_water_payment(),
            "idem-seq2",
            None,
            false,
            None,
            false,
        )
        .await
        .unwrap();

        // Numbers should differ and second > first (lexicographic on zero-padded counter)
        assert_ne!(r1.receipt_number, r2.receipt_number);
        assert!(
            r2.receipt_number > r1.receipt_number,
            "{} > {}",
            r2.receipt_number,
            r1.receipt_number
        );
    }
}
