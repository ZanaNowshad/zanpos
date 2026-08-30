//! One answer to "which terminal is this".
//!
//! Identity is authoritative in `app_config['device_id']` and mirrored into the
//! Windows Credential Manager, so a reinstall rejoins as the same terminal
//! instead of orphaning its hub pairing and restarting its receipt counter.
//!
//! Every fresh database is seeded with one device row so a single-till shop
//! works out of the box. That row is byte-identical on every install, which
//! makes it unusable as a live identity once a second terminal exists:
//! `receipt_number` is `{branch_code}-{device_code}-{seq:08}` counted per
//! device, so two terminals both seeded as `POS01` mint the *same* receipt
//! numbers from their own counters. `receipt_number` is UNIQUE, so the second
//! one to reach the hub is a duplicate and is rejected. Re-keying the seeded
//! identity at startup is what prevents that.
//!
//! Deliberately *not* resolved by `SELECT ... FROM devices ORDER BY device_code`:
//! `devices` is a synced table, so once a terminal pulls its siblings' rows,
//! ordering by a column they also populate can resolve to *their* identity.

use crate::errors::{AppError, AppResult};
use sqlx::SqlitePool;
use ulid::Ulid;

/// Credential Manager entry. Survives uninstall/reinstall.
const SECURE_KEY: &str = "device_identity";

/// The device seeded by `0001_initial.sql`, shared by every install.
pub const SEED_DEVICE_ID: &str = "01JDEVICE0000000000000001";
/// The device_code seeded alongside it — the half that collides receipts.
pub const SEED_DEVICE_CODE: &str = "POS01";

/// Every column naming this terminal, rewritten when a seeded install is
/// re-keyed. Derived from the schema — extend it when a new table stores a
/// device id, or that table's rows will keep pointing at the retired identity.
const IDENTITY_COLUMNS: &[(&str, &str)] = &[
    ("stock_movements", "origin_device_id"),
    ("stock_movements", "device_id"),
    ("customers", "origin_device_id"),
    ("sales", "origin_device_id"),
    ("sales", "device_id"),
    ("sale_items", "origin_device_id"),
    ("payments", "origin_device_id"),
    ("refunds", "origin_device_id"),
    ("refund_items", "origin_device_id"),
    ("delivery_orders", "origin_device_id"),
    ("delivery_orders", "device_id"),
    ("shifts", "origin_device_id"),
    ("shifts", "device_id"),
    ("cash_events", "origin_device_id"),
    ("cash_events", "device_id"),
    ("audit_logs", "origin_device_id"),
    ("audit_logs", "device_id"),
    ("held_carts", "device_id"),
    ("no_sale_events", "device_id"),
    ("diagnostics", "device_id"),
    ("onboarding_state", "device_id"),
    ("riders", "origin_device_id"),
];

/// A short, human-readable till code derived from the identity, so a receipt
/// still reads like `MAIN-POS-K3F7Q-00000001` rather than carrying a ULID.
fn device_code_for(device_id: &str) -> String {
    let tail: String = device_id
        .chars()
        .rev()
        .take(5)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("POS-{tail}")
}

async fn read_config(pool: &SqlitePool) -> Option<String> {
    sqlx::query_scalar::<_, String>("SELECT value FROM app_config WHERE key = 'device_id'")
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .filter(|v| !v.is_empty())
}

/// This terminal's `device_id`.
///
/// `ensure` runs at startup, so the config key is normally present. The
/// fallback covers a database read before that (tests, first launch mid-setup)
/// and orders by `created_at`, which a pulled sibling row cannot win.
pub async fn current(pool: &SqlitePool) -> AppResult<String> {
    if let Some(id) = read_config(pool).await {
        return Ok(id);
    }
    sqlx::query_scalar::<_, String>(
        "SELECT device_id FROM devices WHERE is_active = 1 ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("No active device configured".into()))
}

/// Same as [`current`], but yields `"unknown"` instead of failing — for audit
/// and telemetry paths that must not abort a business operation.
pub async fn current_or_unknown(pool: &SqlitePool) -> String {
    current(pool)
        .await
        .unwrap_or_else(|_| "unknown".to_string())
}

/// What resolving identity against the database alone concluded.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Identity {
    /// Already unique to this install; nothing was written.
    Existing(String),
    /// The seeded (or absent) identity was replaced by this one.
    Rekeyed(String),
}

impl Identity {
    pub(crate) fn id(&self) -> &str {
        match self {
            Identity::Existing(id) | Identity::Rekeyed(id) => id,
        }
    }
}

/// Establish this terminal's identity. Idempotent; call once at startup.
///
/// Credential Manager I/O is confined to this wrapper so [`ensure_in_db`] stays
/// pure and testable — a test that wrote the real keyring would leak a
/// credential onto the machine and then recover it on the next run, quietly
/// turning "mints a fresh identity" into a no-op.
pub async fn ensure(pool: &SqlitePool) -> AppResult<String> {
    let recovered = crate::secure_store::get_secret(SECURE_KEY)
        .filter(|v| !v.is_empty() && v != SEED_DEVICE_ID);

    let outcome = ensure_in_db(pool, recovered).await?;
    match &outcome {
        Identity::Existing(id) => {
            // Backfill the off-machine copy so a reinstall can recover it.
            if crate::secure_store::get_secret(SECURE_KEY).is_none() {
                crate::secure_store::set_secret(SECURE_KEY, id);
            }
        }
        Identity::Rekeyed(id) => {
            crate::secure_store::set_secret(SECURE_KEY, id);
            tracing::info!(
                device_id = %id,
                "Device identity established (replaced seeded {SEED_DEVICE_ID})"
            );
        }
    }
    Ok(outcome.id().to_string())
}

/// The database half of [`ensure`]. `recovered` is an identity retrieved from
/// off-machine storage, adopted in preference to minting a new one so that a
/// reinstalled terminal keeps its hub pairing and receipt sequence.
pub(crate) async fn ensure_in_db(
    pool: &SqlitePool,
    recovered: Option<String>,
) -> AppResult<Identity> {
    if let Some(id) = read_config(pool).await {
        if id != SEED_DEVICE_ID {
            // A unique id is not enough on its own: the printed receipt number
            // embeds device_code, and hub join takes that code as free text, so
            // a terminal can hold a unique id behind a still-colliding code.
            repair_seeded_code(pool, &id).await?;
            return Ok(Identity::Existing(id));
        }
    }

    let new_id = recovered
        .filter(|v| !v.is_empty() && v != SEED_DEVICE_ID)
        .unwrap_or_else(|| Ulid::new().to_string());

    rekey(pool, &new_id).await?;
    Ok(Identity::Rekeyed(new_id))
}

/// Replace a still-seeded `device_code` on an otherwise-unique terminal.
///
/// Only the code changes — the id is already this terminal's own, so no rows
/// need rewriting and the receipt counter is untouched.
async fn repair_seeded_code(pool: &SqlitePool, device_id: &str) -> AppResult<()> {
    let code: Option<String> =
        sqlx::query_scalar("SELECT device_code FROM devices WHERE device_id = ?")
            .bind(device_id)
            .fetch_optional(pool)
            .await?;

    if code.as_deref() != Some(SEED_DEVICE_CODE) {
        return Ok(());
    }

    let new_code = device_code_for(device_id);
    sqlx::query(
        "UPDATE devices SET device_code = ?, updated_at = ?, sync_status = 'pending'
          WHERE device_id = ?",
    )
    .bind(&new_code)
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(device_id)
    .execute(pool)
    .await?;

    tracing::info!(
        device_id = %device_id,
        "Replaced seeded device_code {SEED_DEVICE_CODE} with {new_code} — it namespaces receipt numbers"
    );
    Ok(())
}

/// Move this install off the seeded identity and onto `new_id`, rewriting every
/// local row that still names the seed.
///
/// One transaction with deferred FK checks: `devices.device_id` is a primary key
/// that the rewritten tables reference, so the constraint can only hold once
/// every statement has run.
async fn rekey(pool: &SqlitePool, new_id: &str) -> AppResult<()> {
    let new_code = device_code_for(new_id);
    let now = chrono::Utc::now().to_rfc3339();

    let mut tx = pool.begin().await?;
    sqlx::query("PRAGMA defer_foreign_keys = ON")
        .execute(&mut *tx)
        .await?;

    let seeded_row: Option<String> =
        sqlx::query_scalar("SELECT device_id FROM devices WHERE device_id = ?")
            .bind(SEED_DEVICE_ID)
            .fetch_optional(&mut *tx)
            .await?;

    if seeded_row.is_some() {
        sqlx::query(
            "UPDATE devices
                SET device_id = ?, device_code = ?, updated_at = ?, sync_status = 'pending'
              WHERE device_id = ?",
        )
        .bind(new_id)
        .bind(&new_code)
        .bind(&now)
        .bind(SEED_DEVICE_ID)
        .execute(&mut *tx)
        .await?;
    } else {
        insert_device_row(&mut tx, new_id, &new_code, &now).await?;
    }

    for (table, column) in IDENTITY_COLUMNS {
        // Tables arrive across migrations; a database mid-upgrade may not have
        // them all yet, and a missing table must not abort the re-key.
        let sql = format!("UPDATE {table} SET {column} = ? WHERE {column} = ?");
        if let Err(e) = sqlx::query(&sql)
            .bind(new_id)
            .bind(SEED_DEVICE_ID)
            .execute(&mut *tx)
            .await
        {
            tracing::warn!("Re-key skipped {table}.{column}: {e}");
        }
    }

    sqlx::query(
        "INSERT INTO app_config(key, value, updated_at) VALUES ('device_id', ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(new_id)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

/// Create this terminal's device row when the seeded one is already gone —
/// e.g. an install recovering its identity from the Credential Manager after a
/// wipe, or a database whose seed row was replaced by a hub pull.
async fn insert_device_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    new_id: &str,
    new_code: &str,
    now: &str,
) -> AppResult<()> {
    let branch_id: Option<String> = sqlx::query_scalar(
        "SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(&mut **tx)
    .await?;
    let Some(branch_id) = branch_id else {
        // Pre-setup: no branch exists yet, so there is nothing to attach a
        // device to. Setup will call `ensure` again once it has made one.
        return Ok(());
    };

    sqlx::query(
        "INSERT OR IGNORE INTO devices
           (device_id, branch_id, device_code, name, status, is_active, created_at, updated_at)
         VALUES (?,?,?,?,'online',1,?,?)",
    )
    .bind(new_id)
    .bind(&branch_id)
    .bind(new_code)
    .bind("This Terminal")
    .bind(now)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Re-issue this terminal's identity with a fresh ULID.
///
/// The recovery path for a database cloned onto a second PC (backup restore):
/// both machines then share one `device_id`, which makes their heartbeats
/// collide, their receipt counters mint the same numbers, and — because the
/// pull filter skips a device's own `origin_device_id` — each side's rows
/// invisible to the other. A fresh identity separates all three.
///
/// Every local row naming the old identity is rewritten and marked pending so
/// it re-pushes under the new origin; the heartbeat counter and the sync
/// watermarks reset so the new identity starts from a clean slate. Rows that
/// arrived from other terminals keep their origin and are untouched.
///
/// Returns `(old_id, new_id)`.
pub async fn rekey_to_fresh(pool: &SqlitePool) -> AppResult<(String, String)> {
    let old_id = current(pool).await?;
    if old_id == SEED_DEVICE_ID {
        // The startup ensure() should already have moved off the seed; if it
        // somehow did not, that path is the right one to take.
        let new_id = ensure(pool).await?;
        return Ok((old_id, new_id));
    }

    let new_id = Ulid::new().to_string();
    let new_code = device_code_for(&new_id);
    let now = chrono::Utc::now().to_rfc3339();

    let mut tx = pool.begin().await?;
    sqlx::query("PRAGMA defer_foreign_keys = ON")
        .execute(&mut *tx)
        .await?;

    // The device row itself: capture what identifies it to people, retire the
    // old id locally, and register the new one under a fresh receipt-namespace
    // code so future receipts cannot collide with the sibling's.
    let old_row: Option<(String, String, String)> =
        sqlx::query_as("SELECT branch_id, name, created_at FROM devices WHERE device_id = ?")
            .bind(&old_id)
            .fetch_optional(&mut *tx)
            .await?;

    sqlx::query("DELETE FROM devices WHERE device_id = ?")
        .bind(&old_id)
        .execute(&mut *tx)
        .await?;

    if let Some((branch_id, name, created_at)) = old_row {
        sqlx::query(
            "INSERT INTO devices
               (device_id, branch_id, device_code, name, status, is_active,
                created_at, updated_at)
             VALUES (?, ?, ?, ?, 'online', 1, ?, ?)",
        )
        .bind(&new_id)
        .bind(&branch_id)
        .bind(&new_code)
        .bind(&name)
        .bind(created_at)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }

    for (table, column) in IDENTITY_COLUMNS {
        // Tables arrive across migrations; a database mid-upgrade may not have
        // them all yet, and a missing table must not abort the re-key.
        let sql = format!("UPDATE {table} SET {column} = ? WHERE {column} = ?");
        if let Err(e) = sqlx::query(&sql)
            .bind(&new_id)
            .bind(&old_id)
            .execute(&mut *tx)
            .await
        {
            tracing::warn!("Re-key skipped {table}.{column}: {e}");
        }
    }
    // The rewritten rows exist on the hub under the old origin; under the new
    // one they must be offered again. Rows whose origin is another terminal
    // are left alone — the hub already holds them.
    for table in crate::sync_v2::apply::SYNC_TABLES
        .iter()
        .filter(|t| crate::sync_v2::apply::has_origin_device_id(t))
    {
        let sql = format!(
            "UPDATE {table} SET sync_status = 'pending', sync_attempts = 0
              WHERE origin_device_id = ?"
        );
        if let Err(e) = sqlx::query(&sql).bind(&new_id).execute(&mut *tx).await {
            tracing::warn!("Re-key pending-mark skipped {table}: {e}");
        }
    }

    sqlx::query(
        "INSERT INTO app_config(key, value, updated_at) VALUES ('device_id', ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(&new_id)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    // The heartbeat sequence and the per-table watermarks are facts of the old
    // identity. Keep the hub_url and everything else.
    sqlx::query("DELETE FROM app_config WHERE key = 'heartbeat_seq'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM app_config WHERE key LIKE 'sync_v2_watermark_%'")
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    crate::secure_store::set_secret(SECURE_KEY, &new_id);
    tracing::info!(
        old_device_id = %old_id,
        device_id = %new_id,
        "Device identity re-issued — this terminal was sharing {old_id} with another install"
    );
    Ok((old_id, new_id))
}

#[cfg(test)]
mod tests;
