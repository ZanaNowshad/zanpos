//! Off-site encrypted database backup.
//!
//! Covers the "store PC dies" failure case: the local `db_backup` command
//! writes a copy to the same machine, which is no help when the disk is the
//! thing that failed. This module snapshots the database, encrypts it on the
//! till, and uploads the ciphertext to the store's object storage. The worker
//! never sees plaintext and holds no key.
//!
//! Key handling. The encryption key is derived with Argon2id from a secret
//! plus the branch id as salt. The secret is the license key when one has been
//! imported, so a vendor who issued the license can help a store recover after
//! a total disk loss. With no license the secret is a random value generated
//! once and kept in the OS credential manager — which restores fine on a
//! reinstall, but is lost with the machine. That difference is reported by
//! [`recovery_outlook`] rather than hidden, because a backup you cannot
//! decrypt is worse than no backup: it looks like safety and isn't.
//!
//! Nothing here runs on the sale path. The loop sleeps before its first cycle
//! so launch is untouched, and every failure is logged and swallowed.

use chacha20poly1305::aead::{Aead, KeyInit, OsRng};
use chacha20poly1305::{AeadCore, XChaCha20Poly1305, XNonce};
use sqlx::SqlitePool;

/// Argon2id output length and the AEAD key length.
const KEY_LEN: usize = 32;
/// Credential-manager entry holding the fallback secret (no license imported).
const FALLBACK_SECRET_KEY: &str = "backup_secret";
const BACKUP_PATH: &str = "/api/backup";
/// One cycle a day; the loop also sleeps this long before its first run.
const INTERVAL_SECS: u64 = 24 * 60 * 60;
/// Refuse to upload a snapshot larger than this. A till database far past this
/// means something is wrong, and silently pushing gigabytes over a shop's
/// connection is its own outage. Matches MAX_BACKUP_BYTES in the worker, so a
/// snapshot that would be rejected there is never sent in the first place.
const MAX_SNAPSHOT_BYTES: u64 = 64 * 1024 * 1024;

/// Whether a backup taken now could still be decrypted after the machine dies.
#[derive(Debug, PartialEq, Eq)]
pub enum RecoveryOutlook {
    /// Key derives from the license — recoverable with vendor help.
    RecoverableFromLicense,
    /// Key lives only in this machine's credential manager.
    LocalSecretOnly,
}

pub fn recovery_outlook(license_key: Option<&str>) -> RecoveryOutlook {
    match license_key {
        Some(key) if !key.trim().is_empty() => RecoveryOutlook::RecoverableFromLicense,
        _ => RecoveryOutlook::LocalSecretOnly,
    }
}

/// Derives the backup key. Argon2id with the branch id as salt, so two stores
/// sharing a secret still get different keys.
fn derive_key(secret: &str, branch_id: &str) -> Result<[u8; KEY_LEN], String> {
    use argon2::Argon2;
    // Argon2 requires a salt of at least 8 bytes; branch ids are ULIDs, but a
    // short or empty one must not silently weaken the derivation.
    let mut salt = branch_id.as_bytes().to_vec();
    if salt.len() < 8 {
        salt.resize(8, b'z');
    }
    let mut key = [0u8; KEY_LEN];
    Argon2::default()
        .hash_password_into(secret.as_bytes(), &salt, &mut key)
        .map_err(|e| format!("key derivation failed: {e}"))?;
    Ok(key)
}

/// Encrypts `plaintext`, returning `nonce || ciphertext`. The nonce is random
/// and 192-bit (XChaCha20), so it needs no persisted counter to stay unique.
fn encrypt(key: &[u8; KEY_LEN], plaintext: &[u8]) -> Result<Vec<u8>, String> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|_| "encryption failed".to_string())?;
    let mut out = Vec::with_capacity(nonce.len() + ciphertext.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Inverse of [`encrypt`], for the restore path and for tests.
pub fn decrypt(key: &[u8; KEY_LEN], blob: &[u8]) -> Result<Vec<u8>, String> {
    const NONCE_LEN: usize = 24;
    if blob.len() <= NONCE_LEN {
        return Err("backup blob is truncated".into());
    }
    let (nonce_bytes, ciphertext) = blob.split_at(NONCE_LEN);
    let cipher = XChaCha20Poly1305::new(key.into());
    cipher
        .decrypt(XNonce::from_slice(nonce_bytes), ciphertext)
        .map_err(|_| "decryption failed — wrong key or corrupted backup".to_string())
}

/// The secret the key derives from: the imported license key when present,
/// otherwise a random per-machine value created once and stored in the OS
/// credential manager.
fn backup_secret(license_key: Option<String>) -> Result<String, String> {
    if let Some(key) = license_key.filter(|k| !k.trim().is_empty()) {
        return Ok(key);
    }
    if let Some(existing) = crate::secure_store::get_secret(FALLBACK_SECRET_KEY) {
        return Ok(existing);
    }
    use rand::Rng;
    let generated: String = rand::thread_rng()
        .sample_iter(rand::distributions::Alphanumeric)
        .take(48)
        .map(char::from)
        .collect();
    if !crate::secure_store::set_secret(FALLBACK_SECRET_KEY, &generated) {
        return Err("could not persist the backup secret".into());
    }
    Ok(generated)
}

async fn app_config(pool: &SqlitePool, key: &str) -> Option<String> {
    sqlx::query_scalar::<_, String>("SELECT value FROM app_config WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .filter(|v| !v.is_empty())
}

async fn current_license_key(pool: &SqlitePool) -> Option<String> {
    sqlx::query_scalar::<_, String>("SELECT license_key FROM licenses LIMIT 1")
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

/// Takes a consistent snapshot of the database file. Checkpointing the WAL
/// first matters: without it the copy can miss committed transactions still
/// living in the -wal file, producing a backup that restores to a stale store.
async fn snapshot(pool: &SqlitePool, db_path: &std::path::Path) -> Result<Vec<u8>, String> {
    // `VACUUM INTO` rather than checkpoint-then-copy.
    //
    // The old sequence was two steps — `PRAGMA wal_checkpoint(TRUNCATE)`, then
    // read or copy the file — with a window between them in which a sale can
    // commit. Nothing in SQLite promises that a plain read of a live database
    // yields a valid one; what comes out can be a mixture of two states.
    // `VACUUM INTO` holds a read transaction for the whole write, so the file it
    // produces is the database as it stood at a single instant, and it does not
    // need the WAL folded in first because it reads through it.
    //
    // It also writes a compacted copy, which is smaller to store and to send.
    let staging = db_path.with_extension(format!("snapshot-{}.db", ulid::Ulid::new()));
    // The path goes into SQL, so it must not be able to close the quote. It is
    // derived from the application's own database path, never from user input,
    // and this rejects the only shape that could.
    let staging_sql = staging.display().to_string();
    if staging_sql.contains('\'') {
        return Err("the database path contains a quote".into());
    }

    let result = async {
        sqlx::query(&format!("VACUUM INTO '{staging_sql}'"))
            .execute(pool)
            .await
            .map_err(|e| format!("could not take a database snapshot: {e}"))?;

        let size = std::fs::metadata(&staging)
            .map_err(|e| format!("could not stat the snapshot: {e}"))?
            .len();
        if size > MAX_SNAPSHOT_BYTES {
            return Err(format!("database is {size} bytes, above the backup cap"));
        }
        std::fs::read(&staging).map_err(|e| format!("could not read the snapshot: {e}"))
    }
    .await;

    // The staging copy is a full second copy of the shop's data; it does not
    // stay on disk whether or not the upload worked.
    let _ = std::fs::remove_file(&staging);
    result
}

/// Runs one backup cycle. Returns `Ok(false)` when backup is simply not
/// configured, which is not an error.
pub async fn run_once(pool: &SqlitePool, db_path: &std::path::Path) -> Result<bool, String> {
    let Some(base_url) = app_config(pool, "storefront_publish_url").await else {
        return Ok(false);
    };
    let Some(signing_secret) = crate::secure_store::get_secret("storefront_publish_secret") else {
        return Ok(false);
    };
    let Some(branch_id) = app_config(pool, "branch_id").await else {
        return Ok(false);
    };

    let secret = backup_secret(current_license_key(pool).await)?;
    let key = derive_key(&secret, &branch_id)?;
    let payload = encrypt(&key, &snapshot(pool, db_path).await?)?;

    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let timestamp = chrono::Utc::now().timestamp().to_string();
    let signature = crate::storefront::publisher::sign_request(
        signing_secret.as_bytes(),
        "POST",
        BACKUP_PATH,
        &timestamp,
        &payload,
    );

    let url = format!("{}{}", base_url.trim_end_matches('/'), BACKUP_PATH);
    // reqwest applies no timeout unless one is asked for, and this was the only
    // outbound client in the codebase that did not ask. An endpoint that accepts
    // the connection and then stalls parks this task for the life of the
    // process: the daily loop never comes round again, and off-site backups stop
    // without an error — on the one feature that exists for the store PC dying.
    //
    // The ceiling is generous rather than tight. A snapshot may be up to
    // MAX_SNAPSHOT_BYTES over whatever connection the shop has, and killing a
    // slow but progressing upload would be its own outage; fifteen minutes is
    // past any real transfer and far short of for ever. The connect timeout is
    // short because failing to reach the host at all is not a slow upload.
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(15 * 60))
        .build()
        .map_err(|e| format!("backup client: {e}"))?;
    let response = client
        .post(&url)
        .header("content-type", "application/octet-stream")
        .header("x-zanpos-store", &branch_id)
        .header("x-zanpos-date", &date)
        // authenticate() requires this and checks it against ID_PATTERN. The
        // date doubles as the idempotency key: one backup per store per day,
        // so a retry overwrites the same object instead of piling up copies.
        .header("x-idempotency-key", &date)
        .header("x-zanpos-timestamp", &timestamp)
        .header("x-zanpos-signature", &signature)
        .body(payload)
        .send()
        .await
        .map_err(|e| format!("backup upload failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("backup rejected with status {}", response.status()));
    }
    Ok(true)
}

/// Spawns the daily backup loop. Sleeps a full interval before its first run so
/// launch — and the first sale of the day — is never competing with it.
pub fn spawn(pool: SqlitePool, db_path: std::path::PathBuf) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(INTERVAL_SECS)).await;
            match run_once(&pool, &db_path).await {
                Ok(true) => tracing::info!("backup: off-site snapshot uploaded"),
                Ok(false) => tracing::debug!("backup: not configured, skipping"),
                Err(e) => {
                    tracing::warn!("backup: {e}");
                    let _ =
                        crate::diagnostics::record(&pool, "warn", "backup_fail", &e, None, None)
                            .await;
                }
            }
        }
    });
}

// ── Commands ─────────────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct BackupStatus {
    /// False when no storefront worker is configured — nothing is being backed up.
    pub configured: bool,
    /// True when the key derives from an imported license, so the vendor can
    /// help recover after the machine is gone.
    pub recoverable_from_license: bool,
}

#[tauri::command]
pub async fn backup_status(
    session_token: String,
    state: tauri::State<'_, crate::AppState>,
) -> Result<BackupStatus, crate::errors::AppError> {
    crate::commands::rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        crate::commands::rbac::OWNER_ONLY,
    )
    .await?;
    let configured = app_config(&state.db, "storefront_publish_url")
        .await
        .is_some();
    let license = current_license_key(&state.db).await;
    Ok(BackupStatus {
        configured,
        recoverable_from_license: recovery_outlook(license.as_deref())
            == RecoveryOutlook::RecoverableFromLicense,
    })
}

/// Runs a backup immediately instead of waiting for the daily cycle.
#[tauri::command]
pub async fn backup_run_now(
    session_token: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::AppState>,
) -> Result<bool, crate::errors::AppError> {
    use tauri::Manager;
    crate::commands::rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        crate::commands::rbac::OWNER_ONLY,
    )
    .await?;
    let db_path = app
        .path()
        .app_data_dir()
        .map_err(|e| crate::errors::AppError::Internal(format!("no app data dir: {e}")))?
        .join("zanpos.db");
    run_once(&state.db, &db_path)
        .await
        .map_err(crate::errors::AppError::Internal)
}

/// Decrypts a downloaded backup to `dest_path`.
///
/// Deliberately writes to a new file rather than over the live database: a
/// restore that clobbers a working store because someone picked the wrong file
/// is a worse outage than the one it was meant to fix. Swapping the file in is
/// a separate, deliberate step taken with the app closed.
#[tauri::command]
pub async fn backup_restore_file(
    session_token: Option<String>,
    encrypted_path: String,
    dest_path: String,
    license_key: Option<String>,
    state: tauri::State<'_, crate::AppState>,
) -> Result<String, crate::errors::AppError> {
    use crate::errors::AppError;
    // Restore has to work on a machine that has no owner yet — that is the
    // whole point of it, and the person standing in front of a dead till
    // cannot sign in to a store that does not exist. So the owner check is
    // required only once a store HAS an owner; before that, possession of the
    // encrypted file and its key IS the authorisation, and the command cannot
    // overwrite anything regardless.
    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await
        .unwrap_or(1); // unreadable users table -> assume configured, stay strict
    if user_count > 0 {
        crate::commands::rbac::session_actor(
            &state.sessions,
            &state.db,
            session_token.as_deref().unwrap_or_default(),
            crate::commands::rbac::OWNER_ONLY,
        )
        .await?;
    }
    if dest_path.trim().is_empty() {
        return Err(AppError::Validation(
            "A destination path is required".into(),
        ));
    }
    let dest = std::path::Path::new(&dest_path);
    if dest.exists() {
        return Err(AppError::Validation(
            "That destination already exists — choose a new file".into(),
        ));
    }
    let branch_id = app_config(&state.db, "branch_id")
        .await
        .ok_or_else(|| AppError::Validation("This device has no branch id".into()))?;

    // An explicitly supplied key wins: after a disk loss the licenses table is
    // gone, and the vendor-reissued key is the only way back in.
    let secret = match license_key.filter(|k| !k.trim().is_empty()) {
        Some(key) => key,
        None => backup_secret(current_license_key(&state.db).await).map_err(AppError::Internal)?,
    };
    let key = derive_key(&secret, &branch_id).map_err(AppError::Internal)?;
    let blob = std::fs::read(&encrypted_path)
        .map_err(|e| AppError::Validation(format!("Could not read the backup file: {e}")))?;
    let plaintext = decrypt(&key, &blob).map_err(AppError::Validation)?;
    std::fs::write(dest, plaintext)
        .map_err(|e| AppError::Internal(format!("Could not write the restored database: {e}")))?;
    Ok(dest.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_snapshot() {
        let key = derive_key("license-abc", "branch-01").unwrap();
        let plaintext = b"SQLite format 3\0some pages".to_vec();
        let blob = encrypt(&key, &plaintext).unwrap();
        assert_ne!(
            &blob[24..],
            &plaintext[..],
            "payload must not be stored in the clear"
        );
        assert_eq!(decrypt(&key, &blob).unwrap(), plaintext);
    }

    #[test]
    fn a_different_store_cannot_decrypt_another_stores_backup() {
        let mine = derive_key("same-license", "branch-01").unwrap();
        let theirs = derive_key("same-license", "branch-02").unwrap();
        assert_ne!(mine, theirs, "branch id must salt the derivation");
        let blob = encrypt(&mine, b"secret takings").unwrap();
        assert!(decrypt(&theirs, &blob).is_err());
    }

    #[test]
    fn tampered_ciphertext_is_rejected_rather_than_returned() {
        let key = derive_key("license-abc", "branch-01").unwrap();
        let mut blob = encrypt(&key, b"takings").unwrap();
        let last = blob.len() - 1;
        blob[last] ^= 0xff;
        assert!(
            decrypt(&key, &blob).is_err(),
            "AEAD must reject a modified blob"
        );
    }

    #[test]
    fn truncated_blob_is_rejected() {
        let key = derive_key("license-abc", "branch-01").unwrap();
        assert!(decrypt(&key, &[0u8; 12]).is_err());
    }

    #[test]
    fn short_branch_id_is_padded_rather_than_failing() {
        assert!(derive_key("license-abc", "b1").is_ok());
    }

    #[test]
    fn recovery_outlook_distinguishes_licensed_from_local_only() {
        assert_eq!(
            recovery_outlook(Some("L-1")),
            RecoveryOutlook::RecoverableFromLicense
        );
        assert_eq!(
            recovery_outlook(Some("   ")),
            RecoveryOutlook::LocalSecretOnly
        );
        assert_eq!(recovery_outlook(None), RecoveryOutlook::LocalSecretOnly);
    }
}
