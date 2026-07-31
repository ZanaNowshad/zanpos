//! Licensing spine: fully offline license verification and grace period.
//!
//! ABSOLUTE RULE (product spec): "A till that stops selling over billing is
//! a lawsuit and a reputation, in that order." [`Entitlement`] is ADVISORY
//! ONLY: never gate a sale, receipt, cash drawer, data read/export,
//! EOD/shift close, or security update on it, even when it is `Lapsed`.
//! Past grace, only the BACK OFFICE degrades — the till never locks.
//!
//! Flow: a vendor issues an offline [`LicenseFile`] (JSON) signed with an
//! Ed25519 keypair that never touches this repo or any store device — no
//! server round-trip needed to sell. [`import_license_file`] verifies the
//! signature and, only on success, mirrors it into the `licenses` table
//! (migration 0040); [`get_entitlement`] reads that mirror and computes
//! tier + state from `expires_at`/`grace_until`, entirely offline.
//!
//! Signature checking uses `ed25519-dalek` (see `Cargo.toml`) through
//! [`verify_ed25519_signature`], which calls `verify_strict` to reject
//! low-order keys and non-canonical encodings. It also refuses the all-zero
//! placeholder [`LICENSE_PUBLIC_KEY_HEX`] outright, so a build shipped
//! without a real vendor key verifies nothing rather than trusting whatever
//! a degenerate key would accept. Every caller treats `Err` as "not
//! verified", so the module fails closed on any doubt.
//!
//! `zanpos.key`/`zanpos.key.pub` already exist here as a **minisign**
//! keypair for the Tauri auto-updater (`tauri.conf.json`'s
//! `plugins.updater.pubkey`, signs release artifacts) — a different trust
//! domain. Do NOT reuse it: one compromise would break both, and its wire
//! format differs from raw ed25519-dalek anyway. [`LICENSE_PUBLIC_KEY_HEX`]
//! is an independent placeholder for a dedicated licensing keypair.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use thiserror::Error;

/// Offline grace window after `expires_at` before a license is `Lapsed`.
pub const GRACE_PERIOD_DAYS: i64 = 30;
/// Placeholder Ed25519 public key (32 bytes, hex), independent from the
/// updater's minisign key (see module docs). Public keys are not secrets;
/// replace this with the real vendor key before shipping. The matching
/// private key must live entirely outside this repo, and must never be
/// logged, persisted by the app, or committed here.
pub const LICENSE_PUBLIC_KEY_HEX: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";
#[derive(Debug, Error)]
pub enum LicenseError {
    #[error("license file is not valid JSON: {0}")]
    Malformed(String),
    #[error("license_key must not be empty")]
    EmptyKey,
    #[error("unknown license tier '{0}' (expected 'core' or 'plus')")]
    InvalidTier(String),
    #[error("issued_at/expires_at is not a valid RFC 3339 timestamp: {0}")]
    InvalidTimestamp(String),
    #[error("signature is not valid base64 or not 64 bytes: {0}")]
    InvalidSignatureEncoding(String),
    #[error("public key is not valid hex or not 32 bytes: {0}")]
    InvalidPublicKeyEncoding(String),
    #[error("signature does not match — license file failed verification")]
    SignatureInvalid,
    #[error(
        "ed25519 verification is not available: no ed25519 crate is present in Cargo.toml yet"
    )]
    VerificationUnavailable,
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LicenseTier {
    Core,
    Plus,
}

impl LicenseTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            LicenseTier::Core => "core",
            LicenseTier::Plus => "plus",
        }
    }
}

impl std::str::FromStr for LicenseTier {
    type Err = LicenseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "core" => Ok(LicenseTier::Core),
            "plus" => Ok(LicenseTier::Plus),
            other => Err(LicenseError::InvalidTier(other.to_string())),
        }
    }
}

/// Entitlement state as reported to the rest of the app. Advisory only —
/// see the ABSOLUTE RULE in the module docs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LicenseState {
    Active,
    InGrace { days_remaining: i64 },
    Lapsed,
}

/// Entitlement query result; `tier: None` means no license was ever imported.
#[derive(Debug, Clone, Serialize)]
pub struct Entitlement {
    pub tier: Option<LicenseTier>,
    #[serde(flatten)]
    pub state: LicenseState,
}

/// The offline license file format a vendor hands to a store (JSON).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseFile {
    pub license_key: String,
    pub tier: LicenseTier,
    pub store_name: Option<String>,
    pub issued_at: String,          // RFC 3339
    pub expires_at: Option<String>, // RFC 3339; None = never expires
    /// Base64 (standard alphabet), 64-byte Ed25519 signature over
    /// [`LicenseFile::signing_payload`].
    pub signature: String,
}

impl LicenseFile {
    /// Deterministic bytes the signature covers. Field order/separators are
    /// fixed — the external tool issuing license files must reproduce this
    /// exact layout before signing.
    pub fn signing_payload(&self) -> Vec<u8> {
        format!(
            "{}\n{}\n{}\n{}\n{}",
            self.license_key,
            self.tier.as_str(),
            self.store_name.as_deref().unwrap_or(""),
            self.issued_at,
            self.expires_at.as_deref().unwrap_or(""),
        )
        .into_bytes()
    }
}

/// Local mirror of the `licenses` table (migration 0040).
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct LicenseRecord {
    pub license_key: String,
    pub tier: String,
    pub store_name: Option<String>,
    pub issued_at: String,
    pub expires_at: Option<String>,
    pub signature: String,
    pub last_validated_at: Option<String>,
    pub grace_until: Option<String>,
}

mod verify;
use verify::parse_rfc3339;
pub use verify::{parse_license_file, verify_license_file};

/// `expires_at + GRACE_PERIOD_DAYS`.
pub fn compute_grace_until(expires_at: DateTime<Utc>) -> DateTime<Utc> {
    expires_at + Duration::days(GRACE_PERIOD_DAYS)
}

/// Pure state computation. No `expires_at` = perpetual license; a missing
/// `grace_until` is recomputed from `expires_at`, never unlimited grace.
pub fn compute_state(
    expires_at: Option<DateTime<Utc>>,
    grace_until: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> LicenseState {
    let Some(expires_at) = expires_at else {
        return LicenseState::Active;
    };
    if now <= expires_at {
        return LicenseState::Active;
    }
    let grace_until = grace_until.unwrap_or_else(|| compute_grace_until(expires_at));
    if now <= grace_until {
        LicenseState::InGrace {
            days_remaining: (grace_until - now).num_days().max(0),
        }
    } else {
        LicenseState::Lapsed
    }
}

/// Verifies and imports a license file, replacing any prior one (a store
/// holds exactly one, no history). Writes nothing unless it verifies
/// (currently always fails closed; see [`verify_ed25519_signature`]).
pub async fn import_license_file(
    pool: &SqlitePool,
    raw_json: &str,
) -> Result<LicenseRecord, LicenseError> {
    let file = parse_license_file(raw_json)?;
    verify_license_file(&file)?;

    let now = Utc::now();
    let expires_at_dt = match &file.expires_at {
        Some(s) => Some(parse_rfc3339(s)?),
        None => None,
    };
    let grace_until = expires_at_dt.map(compute_grace_until);
    let last_validated_at = now.to_rfc3339();
    let grace_until_str = grace_until.map(|d| d.to_rfc3339());

    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM licenses")
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO licenses
            (license_key, tier, store_name, issued_at, expires_at, signature, last_validated_at, grace_until)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&file.license_key)
    .bind(file.tier.as_str())
    .bind(&file.store_name)
    .bind(&file.issued_at)
    .bind(&file.expires_at)
    .bind(&file.signature)
    .bind(&last_validated_at)
    .bind(&grace_until_str)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(LicenseRecord {
        license_key: file.license_key,
        tier: file.tier.as_str().to_string(),
        store_name: file.store_name,
        issued_at: file.issued_at,
        expires_at: file.expires_at,
        signature: file.signature,
        last_validated_at: Some(last_validated_at),
        grace_until: grace_until_str,
    })
}

/// The entitlement query the rest of the app calls — a local read + date
/// computation, never the network. Advisory only; see the ABSOLUTE RULE above.
pub async fn get_entitlement(pool: &SqlitePool) -> Result<Entitlement, LicenseError> {
    let row: Option<LicenseRecord> = sqlx::query_as(
        "SELECT license_key, tier, store_name, issued_at, expires_at, signature, last_validated_at, grace_until
         FROM licenses ORDER BY last_validated_at DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    let Some(record) = row else {
        return Ok(Entitlement {
            tier: None,
            state: LicenseState::Lapsed,
        });
    };

    let tier = record.tier.parse::<LicenseTier>().ok();
    let now = Utc::now();
    let expires_at = record
        .expires_at
        .as_deref()
        .and_then(|s| parse_rfc3339(s).ok());
    let grace_until = record
        .grace_until
        .as_deref()
        .and_then(|s| parse_rfc3339(s).ok());

    Ok(Entitlement {
        tier,
        state: compute_state(expires_at, grace_until, now),
    })
}

/// Read-only entitlement query for the frontend. Advisory only — never gate
/// a sale, receipt, cash drawer, EOD/shift close, data access, or updates on it.
#[tauri::command]
pub async fn license_get_entitlement(
    state: tauri::State<'_, crate::AppState>,
) -> Result<Entitlement, String> {
    get_entitlement(&state.db).await.map_err(|e| e.to_string())
}

/// Installs a license from raw file content (the frontend reads the file
/// via the dialog plugin and passes its text here — see `src/tauri/license.ts`).
#[tauri::command]
pub async fn license_import_file(
    state: tauri::State<'_, crate::AppState>,
    content: String,
) -> Result<LicenseRecord, String> {
    if content.trim().is_empty() {
        return Err("license file content is empty".into());
    }
    import_license_file(&state.db, &content)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use sqlx::sqlite::SqlitePoolOptions;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("memory db");
        sqlx::query(
            "CREATE TABLE licenses (
                license_key TEXT PRIMARY KEY, tier TEXT NOT NULL, store_name TEXT,
                issued_at TEXT NOT NULL, expires_at TEXT, signature TEXT NOT NULL,
                last_validated_at TEXT, grace_until TEXT)",
        )
        .execute(&pool)
        .await
        .expect("licenses table");
        pool
    }

    fn sample_license_json(expires_at: Option<&str>) -> String {
        format!(
            r#"{{"license_key":"ZP-TEST-0001","tier":"plus","store_name":"Test Store","issued_at":"2026-01-01T00:00:00Z","expires_at":{},"signature":"{}"}}"#,
            expires_at
                .map(|s| format!("\"{s}\""))
                .unwrap_or_else(|| "null".into()),
            STANDARD.encode([0u8; 64]),
        )
    }

    #[test]
    fn tier_round_trips_through_db_strings() {
        assert_eq!("core".parse::<LicenseTier>().unwrap(), LicenseTier::Core);
        assert_eq!("plus".parse::<LicenseTier>().unwrap(), LicenseTier::Plus);
        assert!("gold".parse::<LicenseTier>().is_err());
    }

    #[test]
    fn parse_rejects_malformed_input_at_the_boundary() {
        let empty_key = r#"{"license_key":"","tier":"core","store_name":null,"issued_at":"2026-01-01T00:00:00Z","expires_at":null,"signature":""}"#;
        let bad_sig = r#"{"license_key":"K","tier":"core","store_name":null,"issued_at":"2026-01-01T00:00:00Z","expires_at":null,"signature":"not-base64!!"}"#;
        assert!(matches!(
            parse_license_file(""),
            Err(LicenseError::Malformed(_))
        ));
        assert!(matches!(
            parse_license_file("{not json"),
            Err(LicenseError::Malformed(_))
        ));
        assert!(matches!(
            parse_license_file(empty_key),
            Err(LicenseError::EmptyKey)
        ));
        assert!(matches!(
            parse_license_file(bad_sig),
            Err(LicenseError::InvalidSignatureEncoding(_))
        ));
    }

    #[test]
    fn parse_accepts_well_formed_license_and_builds_deterministic_payload() {
        let file = parse_license_file(&sample_license_json(Some("2027-01-01T00:00:00Z"))).unwrap();
        assert_eq!(file.license_key, "ZP-TEST-0001");
        assert_eq!(file.tier, LicenseTier::Plus);
        assert_eq!(
            String::from_utf8(file.signing_payload()).unwrap(),
            "ZP-TEST-0001\nplus\nTest Store\n2026-01-01T00:00:00Z\n2027-01-01T00:00:00Z"
        );
    }

    #[tokio::test]
    async fn import_fails_closed_and_writes_nothing_under_the_placeholder_key() {
        let pool = test_pool().await;
        let result =
            import_license_file(&pool, &sample_license_json(Some("2027-01-01T00:00:00Z"))).await;
        assert!(matches!(result, Err(LicenseError::VerificationUnavailable)));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM licenses")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "an unverified license must never be persisted");
    }

    #[test]
    fn grace_period_state_machine() {
        let now = Utc::now();
        assert_eq!(compute_state(None, None, now), LicenseState::Active);
        assert_eq!(
            compute_state(Some(now + Duration::days(10)), None, now),
            LicenseState::Active
        );
        let expires_at = now - Duration::days(5);
        match compute_state(Some(expires_at), Some(compute_grace_until(expires_at)), now) {
            LicenseState::InGrace { days_remaining } => {
                assert!((24..=25).contains(&days_remaining))
            }
            other => panic!("expected InGrace, got {other:?}"),
        }
        let expires_at = now - Duration::days(40);
        let lapsed = compute_state(Some(expires_at), Some(compute_grace_until(expires_at)), now);
        assert_eq!(lapsed, LicenseState::Lapsed);
        // Missing grace_until is recomputed from expiry, never unlimited.
        assert_eq!(
            compute_state(Some(expires_at), None, now),
            LicenseState::Lapsed
        );
        assert_eq!(
            (compute_grace_until(now) - now).num_days(),
            GRACE_PERIOD_DAYS
        );
    }

    async fn insert_license_row(
        pool: &SqlitePool,
        tier: &str,
        expires_at: Option<DateTime<Utc>>,
        grace_until: Option<DateTime<Utc>>,
    ) {
        // OR REPLACE mirrors `import_license_file`: a store holds exactly one
        // license and importing a new one replaces it, so successive calls here
        // model re-licensing rather than colliding on the primary key.
        sqlx::query(
            "INSERT OR REPLACE INTO licenses (license_key, tier, store_name, issued_at, expires_at, signature, last_validated_at, grace_until)
             VALUES ('K1', ?, 'Store', '2026-01-01T00:00:00Z', ?, 'sig', ?, ?)",
        )
        .bind(tier)
        .bind(expires_at.map(|d| d.to_rfc3339()))
        .bind(Utc::now().to_rfc3339())
        .bind(grace_until.map(|d| d.to_rfc3339()))
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn entitlement_with_no_license_row_is_lapsed_with_no_tier() {
        let pool = test_pool().await;
        let entitlement = get_entitlement(&pool).await.unwrap();
        assert_eq!(entitlement.tier, None);
        assert_eq!(entitlement.state, LicenseState::Lapsed);
    }

    #[tokio::test]
    async fn entitlement_reflects_active_in_grace_and_lapsed_rows() {
        let pool = test_pool().await;
        insert_license_row(&pool, "plus", Some(Utc::now() + Duration::days(10)), None).await;
        let e = get_entitlement(&pool).await.unwrap();
        assert_eq!(e.tier, Some(LicenseTier::Plus));
        assert_eq!(e.state, LicenseState::Active);
        let expires_at = Utc::now() - Duration::days(3);
        insert_license_row(
            &pool,
            "core",
            Some(expires_at),
            Some(compute_grace_until(expires_at)),
        )
        .await;
        let e = get_entitlement(&pool).await.unwrap();
        assert_eq!(e.tier, Some(LicenseTier::Core));
        assert!(matches!(e.state, LicenseState::InGrace { .. }));
        let expires_at = Utc::now() - Duration::days(90);
        insert_license_row(
            &pool,
            "core",
            Some(expires_at),
            Some(compute_grace_until(expires_at)),
        )
        .await;
        let e = get_entitlement(&pool).await.unwrap();
        assert_eq!(e.state, LicenseState::Lapsed);
    }

    #[test]
    fn entitlement_json_shape_matches_active_in_grace_lapsed_contract() {
        let active = serde_json::to_value(Entitlement {
            tier: Some(LicenseTier::Plus),
            state: LicenseState::Active,
        })
        .unwrap();
        assert_eq!(active["tier"], "plus");
        assert_eq!(active["state"], "active");
        let grace = serde_json::to_value(Entitlement {
            tier: Some(LicenseTier::Core),
            state: LicenseState::InGrace { days_remaining: 5 },
        })
        .unwrap();
        assert_eq!(grace["state"], "in_grace");
        assert_eq!(grace["days_remaining"], 5);
        let lapsed = serde_json::to_value(Entitlement {
            tier: None,
            state: LicenseState::Lapsed,
        })
        .unwrap();
        assert_eq!(lapsed["tier"], serde_json::Value::Null);
        assert_eq!(lapsed["state"], "lapsed");
    }
}
