//! Finding *which* rows differ between a terminal and the hub.
//!
//! `consistency` already answers whether a table matches: one SHA-256 over
//! every row, compared against the hub's. That is the right shape for "is this
//! terminal in step", and it is the wrong shape for the question that follows.
//! When `products` comes back `checksum_mismatch` on a 28,010-row catalogue, the
//! operator learns that something, somewhere, is different — and the only remedy
//! the product offers is Pull Hub Truth, a full resync that fixes the one bad
//! row by re-downloading the other 28,009.
//!
//! This narrows it. Rows are dealt into buckets by a hash of their primary key;
//! each bucket gets its own count and checksum. Comparing 64 buckets finds the
//! handful that differ, and only those are expanded into per-row digests. A
//! catalogue-wide divergence costs two small requests and one bucket's worth of
//! ids, instead of shipping the catalogue twice.
//!
//! Two things make it trustworthy rather than merely fast:
//!
//! **The same definition of equality.** Row bytes come from
//! `consistency::row_fingerprint`, the function the table checksum itself uses.
//! A second, parallel notion of "equal" would eventually disagree with the first,
//! and the drill-down would start naming rows the table checksum thinks are
//! fine.
//!
//! **A hash that cannot drift.** Bucketing must land a given key in the same
//! bucket on both machines, across builds and platforms. `DefaultHasher` is
//! explicitly not stable across Rust releases, so FNV-1a is spelled out here.

use crate::errors::AppResult;
use crate::sync_v2::apply::{pk_for_table, ALLOWED_CONFIG_KEYS};
use crate::sync_v2::consistency::row_fingerprint;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};

/// Enough that a single divergent row lands in a bucket of a few hundred on a
/// 28,000-row catalogue, few enough that the bucket list is one small response.
pub const DEFAULT_BUCKETS: u32 = 64;

/// A drill-down returns ids, not rows. Past this many the answer is "resync",
/// and listing them only makes the reply expensive to send and useless to read.
pub const MAX_REPORTED_ROWS: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BucketDigest {
    pub bucket: u32,
    pub count: i64,
    pub checksum: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RowDigest {
    pub pk: String,
    pub checksum: String,
}

/// Which side holds what, for one primary key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Divergence {
    /// The hub has it and this terminal does not — a pull that never landed.
    MissingLocally,
    /// This terminal has it and the hub does not — a push still queued, or lost.
    MissingOnHub,
    /// Both hold it and the contents differ. The interesting one: last-writer
    /// -wins is row-level, so a rename here and a cost edit there leaves each
    /// side holding the other's stale copy of one field.
    Different,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DivergentRow {
    pub pk: String,
    pub divergence: Divergence,
}

/// FNV-1a, written out rather than borrowed from the standard library.
///
/// The bucket a key falls into has to be identical on the terminal and the hub.
/// `std::collections::hash_map::DefaultHasher` gives no such guarantee — it is
/// documented as free to change between Rust releases — so two machines on
/// different toolchains would silently bucket the same key differently and
/// every bucket would appear to mismatch.
pub fn bucket_of(key: &str, buckets: u32) -> u32 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in key.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (hash % buckets.max(1) as u64) as u32
}

fn select_sql(table: &str) -> String {
    let pk = pk_for_table(table);
    let mut sql = format!("SELECT * FROM {table}");
    if table == "app_config" {
        // Mirrors `consistency`: only the keys that are allowed to sync are
        // part of the comparison, or a device-local setting reads as drift.
        let list = ALLOWED_CONFIG_KEYS
            .iter()
            .map(|k| format!("'{k}'"))
            .collect::<Vec<_>>()
            .join(",");
        sql.push_str(&format!(" WHERE key IN ({list})"));
    }
    sql.push_str(&format!(" ORDER BY {pk} ASC"));
    sql
}

fn pk_value(row: &sqlx::sqlite::SqliteRow, pk: &str) -> String {
    row.try_get::<String, _>(pk)
        .or_else(|_| row.try_get::<i64, _>(pk).map(|v| v.to_string()))
        .unwrap_or_default()
}

/// Count and checksum per bucket for one table.
pub async fn bucket_digests(
    pool: &SqlitePool,
    table: &str,
    buckets: u32,
) -> AppResult<Vec<BucketDigest>> {
    let pk = pk_for_table(table);
    let rows = sqlx::query(&select_sql(table)).fetch_all(pool).await?;

    let buckets = buckets.max(1);
    let mut hashers: Vec<Sha256> = (0..buckets).map(|_| Sha256::new()).collect();
    let mut counts = vec![0_i64; buckets as usize];

    for row in &rows {
        // Rows arrive ordered by primary key, so each bucket's hasher is fed in
        // a deterministic order on both machines without sorting per bucket.
        let index = bucket_of(&pk_value(row, pk), buckets) as usize;
        counts[index] += 1;
        hashers[index].update(row_fingerprint(table, row).as_bytes());
        hashers[index].update(b"\n");
    }

    Ok(hashers
        .into_iter()
        .enumerate()
        .map(|(index, hasher)| BucketDigest {
            bucket: index as u32,
            count: counts[index],
            checksum: hex::encode(hasher.finalize()),
        })
        .collect())
}

/// Per-row digests for one bucket — the second and final round trip.
pub async fn row_digests(
    pool: &SqlitePool,
    table: &str,
    bucket: u32,
    buckets: u32,
) -> AppResult<Vec<RowDigest>> {
    let pk = pk_for_table(table);
    let rows = sqlx::query(&select_sql(table)).fetch_all(pool).await?;
    let buckets = buckets.max(1);

    Ok(rows
        .iter()
        .filter_map(|row| {
            let key = pk_value(row, pk);
            if bucket_of(&key, buckets) != bucket {
                return None;
            }
            let mut hasher = Sha256::new();
            hasher.update(row_fingerprint(table, row).as_bytes());
            Some(RowDigest {
                pk: key,
                checksum: hex::encode(hasher.finalize()),
            })
        })
        .collect())
}

/// Buckets whose count or checksum differ, and buckets one side does not have.
pub fn mismatched_buckets(local: &[BucketDigest], hub: &[BucketDigest]) -> Vec<u32> {
    let mut out = Vec::new();
    for local_bucket in local {
        match hub.iter().find(|b| b.bucket == local_bucket.bucket) {
            // A hub that answered with fewer buckets than we asked for is on a
            // different setting, and comparing across it would be meaningless.
            None => out.push(local_bucket.bucket),
            Some(hub_bucket) => {
                if hub_bucket.count != local_bucket.count
                    || hub_bucket.checksum != local_bucket.checksum
                {
                    out.push(local_bucket.bucket);
                }
            }
        }
    }
    out
}

/// The exact rows that differ inside one bucket.
pub fn diff_rows(local: &[RowDigest], hub: &[RowDigest]) -> Vec<DivergentRow> {
    let mut out = Vec::new();
    for local_row in local {
        match hub.iter().find(|r| r.pk == local_row.pk) {
            None => out.push(DivergentRow {
                pk: local_row.pk.clone(),
                divergence: Divergence::MissingOnHub,
            }),
            Some(hub_row) if hub_row.checksum != local_row.checksum => out.push(DivergentRow {
                pk: local_row.pk.clone(),
                divergence: Divergence::Different,
            }),
            Some(_) => {}
        }
    }
    for hub_row in hub {
        if !local.iter().any(|r| r.pk == hub_row.pk) {
            out.push(DivergentRow {
                pk: hub_row.pk.clone(),
                divergence: Divergence::MissingLocally,
            });
        }
    }
    out.sort_by(|a, b| a.pk.cmp(&b.pk));
    out
}

#[cfg(test)]
mod tests;
