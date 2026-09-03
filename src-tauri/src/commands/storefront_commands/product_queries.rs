use super::{StorefrontProduct, StorefrontProductPage};
use crate::errors::AppResult;
use sqlx::{Row, SqlitePool};

const PRODUCT_SELECT: &str = "
    SELECT p.product_id,p.name,p.description,pp.price_minor,pp.currency,
           sp.name_ar,sp.description_ar,sp.public_image_url,
           COALESCE(sp.is_visible,0) published,COALESCE(sp.featured,0) featured,
           COALESCE(sp.sort_order,0) sort_order,sp.publish_error,
           CASE WHEN sp.is_visible=1 AND
             (sp.last_published_hash IS NULL OR latest.published_at IS NULL OR
              p.updated_at>latest.published_at OR sp.updated_at>latest.published_at OR
              pp.created_at>latest.published_at) THEN 1 ELSE 0 END dirty
    FROM products p
    JOIN categories c ON c.category_id=p.category_id
    LEFT JOIN storefront_products sp ON sp.product_id=p.product_id
    JOIN product_prices pp ON pp.price_id=(
        SELECT candidate.price_id FROM product_prices candidate
        WHERE candidate.product_id=p.product_id AND candidate.price_type='selling'
          AND (candidate.branch_id=? OR candidate.branch_id IS NULL)
          -- Normalised on both sides, matching `sale_repo`. Comparing the raw
          -- text against an RFC3339 'now' happened to work for rows this
          -- application wrote, and not for rows the legacy-POS importer wrote
          -- with datetime('now') — and the raw ORDER BY ranked every 'T' row
          -- above every space row whatever the actual instant. The storefront
          -- could publish a price the till would not charge.
          AND datetime(candidate.effective_from)<=datetime('now')
          AND (candidate.effective_to IS NULL OR
               datetime(candidate.effective_to)>datetime('now'))
        ORDER BY CASE WHEN candidate.branch_id=? THEN 0 ELSE 1 END,
                 datetime(candidate.effective_from) DESC,candidate.price_id DESC LIMIT 1)
    LEFT JOIN (
        SELECT published_at FROM storefront_releases
        WHERE status='published' ORDER BY version DESC LIMIT 1
    ) latest ON 1=1
    WHERE p.is_active=1 AND p.deleted_at IS NULL
      AND c.is_active=1 AND c.deleted_at IS NULL";

async fn branch_id(pool: &SqlitePool) -> AppResult<String> {
    Ok(sqlx::query_scalar(
        "SELECT branch_id FROM branches
         WHERE is_active=1 AND deleted_at IS NULL ORDER BY created_at LIMIT 1",
    )
    .fetch_one(pool)
    .await?)
}

fn product_from_row(row: sqlx::sqlite::SqliteRow) -> StorefrontProduct {
    StorefrontProduct {
        product_id: row.get("product_id"),
        name: row.get("name"),
        name_ar: row.get("name_ar"),
        description: row.get("description"),
        description_ar: row.get("description_ar"),
        price_minor: row.get("price_minor"),
        currency: row.get("currency"),
        image_url: row.get("public_image_url"),
        published: row.get::<i64, _>("published") != 0,
        featured: row.get::<i64, _>("featured") != 0,
        sort_order: row.get("sort_order"),
        dirty: row.get::<i64, _>("dirty") != 0,
        publish_error: row.get("publish_error"),
    }
}

pub(super) async fn products(
    pool: &SqlitePool,
    search: Option<&str>,
) -> AppResult<Vec<StorefrontProduct>> {
    let branch_id = branch_id(pool).await?;
    let pattern = format!("%{}%", search.unwrap_or("").trim());
    let sql = format!(
        "{PRODUCT_SELECT}
         AND (?='%%' OR p.name LIKE ? OR COALESCE(sp.name_ar,'') LIKE ?)
         ORDER BY COALESCE(sp.featured,0) DESC,COALESCE(sp.sort_order,0),
                  LOWER(p.name),p.product_id"
    );
    let rows = sqlx::query(&sql)
        .bind(&branch_id)
        .bind(&branch_id)
        .bind(&pattern)
        .bind(&pattern)
        .bind(&pattern)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(product_from_row).collect())
}

pub(super) async fn products_page(
    pool: &SqlitePool,
    search: Option<&str>,
    requested_offset: i64,
    requested_limit: i64,
    published_only: bool,
) -> AppResult<StorefrontProductPage> {
    let branch_id = branch_id(pool).await?;
    let offset = requested_offset.max(0);
    let limit = requested_limit.clamp(1, 100);
    let pattern = format!("%{}%", search.unwrap_or("").trim());
    let visibility = if published_only {
        " AND COALESCE(sp.is_visible,0)=1"
    } else {
        ""
    };
    let count_sql = format!(
        "SELECT COUNT(*) FROM ({PRODUCT_SELECT}
         AND (?='%%' OR p.name LIKE ? OR COALESCE(sp.name_ar,'') LIKE ?)
         {visibility})"
    );
    let total: i64 = sqlx::query_scalar(&count_sql)
        .bind(&branch_id)
        .bind(&branch_id)
        .bind(&pattern)
        .bind(&pattern)
        .bind(&pattern)
        .fetch_one(pool)
        .await?;
    let page_sql = format!(
        "{PRODUCT_SELECT}
         AND (?='%%' OR p.name LIKE ? OR COALESCE(sp.name_ar,'') LIKE ?)
         {visibility}
         ORDER BY COALESCE(sp.featured,0) DESC,COALESCE(sp.sort_order,0),
                  LOWER(p.name),p.product_id
         LIMIT ? OFFSET ?"
    );
    let rows = sqlx::query(&page_sql)
        .bind(&branch_id)
        .bind(&branch_id)
        .bind(&pattern)
        .bind(&pattern)
        .bind(&pattern)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;
    Ok(StorefrontProductPage {
        items: rows.into_iter().map(product_from_row).collect(),
        total,
        offset,
        limit,
    })
}

pub(super) async fn product_by_id(
    pool: &SqlitePool,
    product_id: &str,
) -> AppResult<Option<StorefrontProduct>> {
    let branch_id = branch_id(pool).await?;
    let sql = format!("{PRODUCT_SELECT} AND p.product_id=?");
    let row = sqlx::query(&sql)
        .bind(&branch_id)
        .bind(&branch_id)
        .bind(product_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(product_from_row))
}
