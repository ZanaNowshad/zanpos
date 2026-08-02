//! RBAC repository-coverage validator for ZANPOS Semgrep policy.
//!
//! Semgrep Community Edition 1.172 normalizes `use` imports and local aliases
//! for the repository-call pattern used here; the fixtures enforce both forms.
//! CE does not follow arbitrary wrapper call graphs, so the policy additionally
//! enforces the finite `persist_*_wrapper`, `*_mutation`, and `*_write` naming
//! boundary (owner: ZANPOS Maintainers; review_by: 2027-01-31).
use crate::db::repositories::sale_repo;
use sqlx::SqlitePool;

// Fully-qualified mutation call without RBAC: MUST be flagged.
#[tauri::command]
async fn approve_fully_qualified(pool: &SqlitePool) {
    // ruleid: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::sale_repo::approve(pool, "fixture").await;
}

// Imported-path mutation call without RBAC: receiver matching must flag it.
#[tauri::command]
async fn approve_imported_path(pool: &SqlitePool) {
    // ruleid: zanpos-tauri-repository-mutation-requires-rbac
    sale_repo::approve(pool, "fixture").await;
}

// Imported-path with an RBAC guard preceding the sink: ordering baseline.
#[tauri::command]
async fn approve_imported_guarded(pool: &SqlitePool, actor_user_id: &str) {
    rbac::manager_or_owner(pool, actor_user_id).await?;
    // ok: zanpos-tauri-repository-mutation-requires-rbac
    sale_repo::approve(pool, "fixture").await;
    Ok(())
}

// Expanded mutation-verb coverage on the fully-qualified form Semgrep matches.
#[tauri::command]
async fn sync_fully_qualified(pool: &SqlitePool) {
    // ruleid: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::sale_repo::sync(pool, "fixture").await;
}

// Fully-qualified mutation with an RBAC guard BEFORE the sink is suppressed.
#[tauri::command]
async fn approve_fully_qualified_guarded(pool: &SqlitePool, actor_user_id: &str) {
    rbac::manager_or_owner(pool, actor_user_id).await?;
    // ok: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::sale_repo::approve(pool, "fixture").await;
    Ok(())
}

// A guard after the sink cannot authorize the preceding mutation.
#[tauri::command]
async fn sync_fully_qualified_late_guard(pool: &SqlitePool, actor_user_id: &str) {
    // ruleid: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::sale_repo::sync(pool, "fixture").await;
    rbac::manager_or_owner(pool, actor_user_id).await?;
    Ok(())
}
