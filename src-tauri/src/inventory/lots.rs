use crate::errors::{AppError, AppResult};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use std::str::FromStr;

#[derive(Debug, Serialize)]
pub struct ExpiringLot {
    pub movement_id: String,
    pub product_id: String,
    pub product_name: String,
    pub expiry_date: String,
    pub quantity_remaining: String,
    pub days_until_expiry: i64,
}

pub fn validate_expiry_date(value: Option<&str>) -> AppResult<Option<String>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let parsed = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| AppError::Validation("Expiry date must be a valid YYYY-MM-DD date".into()))?;
    if parsed.format("%Y-%m-%d").to_string() != value {
        return Err(AppError::Validation(
            "Expiry date must use YYYY-MM-DD".into(),
        ));
    }
    Ok(Some(value.to_string()))
}

pub async fn consume_fefo(
    tx: &mut Transaction<'_, Sqlite>,
    product_id: &str,
    branch_id: &str,
    quantity: Decimal,
) -> AppResult<()> {
    if quantity <= Decimal::ZERO {
        return Ok(());
    }
    let rows = sqlx::query(
        "SELECT movement_id, lot_quantity_remaining
         FROM stock_movements
         WHERE product_id = ? AND branch_id = ? AND movement_type = 'receive'
           AND expiry_date IS NOT NULL AND lot_quantity_remaining IS NOT NULL
           AND CAST(lot_quantity_remaining AS REAL) > 0
         ORDER BY expiry_date, created_at, movement_id",
    )
    .bind(product_id)
    .bind(branch_id)
    .fetch_all(&mut **tx)
    .await?;
    let mut unallocated = quantity;
    for row in rows {
        if unallocated <= Decimal::ZERO {
            break;
        }
        let movement_id: String = row.get("movement_id");
        let remaining = Decimal::from_str(row.get::<String, _>("lot_quantity_remaining").as_str())
            .map_err(|_| AppError::Validation("Stored lot quantity is invalid".into()))?;
        let consumed = remaining.min(unallocated);
        let new_remaining = remaining - consumed;
        sqlx::query(
            "UPDATE stock_movements
             SET lot_quantity_remaining = ?, sync_status = 'pending'
             WHERE movement_id = ?",
        )
        .bind(new_remaining.to_string())
        .bind(movement_id)
        .execute(&mut **tx)
        .await?;
        unallocated -= consumed;
    }
    Ok(())
}

pub async fn expiring_lots(
    pool: &SqlitePool,
    branch_id: &str,
    lead_days: i64,
) -> AppResult<Vec<ExpiringLot>> {
    if !(0..=365).contains(&lead_days) {
        return Err(AppError::Validation(
            "Expiry lead days must be between 0 and 365".into(),
        ));
    }
    let rows = sqlx::query(
        "SELECT sm.movement_id, sm.product_id, p.name AS product_name,
                sm.expiry_date, sm.lot_quantity_remaining,
                CAST(julianday(sm.expiry_date) - julianday(date('now')) AS INTEGER)
                  AS days_until_expiry
         FROM stock_movements sm
         JOIN products p ON p.product_id = sm.product_id
         WHERE sm.branch_id = ? AND sm.movement_type = 'receive'
           AND sm.expiry_date IS NOT NULL
           AND sm.lot_quantity_remaining IS NOT NULL
           AND CAST(sm.lot_quantity_remaining AS REAL) > 0
           AND date(sm.expiry_date) <= date('now', '+' || ? || ' days')
         ORDER BY sm.expiry_date, p.name, sm.movement_id",
    )
    .bind(branch_id)
    .bind(lead_days)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| ExpiringLot {
            movement_id: row.get("movement_id"),
            product_id: row.get("product_id"),
            product_name: row.get("product_name"),
            expiry_date: row.get("expiry_date"),
            quantity_remaining: row.get("lot_quantity_remaining"),
            days_until_expiry: row.get("days_until_expiry"),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use sqlx::{sqlite::SqlitePoolOptions, Row};
    use std::str::FromStr;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO categories
             (category_id, name, created_at, updated_at)
             VALUES ('C1', 'Dairy', '2026-07-01', '2026-07-01')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO products
             (product_id, category_id, name, created_at, updated_at)
             VALUES ('P1', ?, 'Fresh Milk', '2026-07-01', '2026-07-01')",
        )
        .bind("C1")
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    async fn insert_lot(pool: &SqlitePool, movement_id: &str, expiry_date: &str, quantity: &str) {
        sqlx::query(
            "INSERT INTO stock_movements
             (movement_id, product_id, branch_id, device_id, movement_type,
              quantity_delta, quantity_after, created_at, expiry_date,
              lot_quantity_received, lot_quantity_remaining)
             VALUES (?, 'P1', 'B1', 'D1', 'receive', ?, ?, '2026-07-01',
                     ?, ?, ?)",
        )
        .bind(movement_id)
        .bind(quantity)
        .bind(quantity)
        .bind(expiry_date)
        .bind(quantity)
        .bind(quantity)
        .execute(pool)
        .await
        .unwrap();
    }

    #[test]
    fn expiry_boundary_accepts_iso_dates_and_rejects_ambiguous_text() {
        assert_eq!(
            validate_expiry_date(Some("2026-08-31")).unwrap(),
            Some("2026-08-31".into())
        );
        assert_eq!(validate_expiry_date(None).unwrap(), None);
        assert!(validate_expiry_date(Some("31/08/2026")).is_err());
        assert!(validate_expiry_date(Some("2026-02-30")).is_err());
    }

    #[tokio::test]
    async fn sale_consumption_uses_first_expired_first_out() {
        let pool = pool().await;
        insert_lot(&pool, "LATE", "2026-09-01", "5").await;
        insert_lot(&pool, "EARLY", "2026-08-01", "3").await;

        let mut tx = pool.begin().await.unwrap();
        consume_fefo(&mut tx, "P1", "B1", Decimal::from_str("4").unwrap())
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let rows = sqlx::query(
            "SELECT movement_id, lot_quantity_remaining
             FROM stock_movements WHERE movement_id IN ('EARLY','LATE')
             ORDER BY movement_id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(rows[0].get::<String, _>("lot_quantity_remaining"), "0");
        assert_eq!(rows[1].get::<String, _>("lot_quantity_remaining"), "4");
    }

    #[tokio::test]
    async fn expiry_report_returns_only_nonempty_lots_inside_the_window() {
        let pool = pool().await;
        let soon = (chrono::Utc::now().date_naive() + chrono::Days::new(2)).to_string();
        let later = (chrono::Utc::now().date_naive() + chrono::Days::new(20)).to_string();
        insert_lot(&pool, "SOON", &soon, "2").await;
        insert_lot(&pool, "LATER", &later, "4").await;
        insert_lot(&pool, "EMPTY", &soon, "0").await;

        let report = expiring_lots(&pool, "B1", 7).await.unwrap();

        assert_eq!(report.len(), 1);
        assert_eq!(report[0].movement_id, "SOON");
        assert_eq!(report[0].quantity_remaining, "2");
    }
}
