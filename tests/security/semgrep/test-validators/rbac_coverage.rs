//! RBAC repository-coverage validator for ZANPOS Semgrep policy.
//!
//! Semgrep Community Edition does not resolve `use` imports or aliases, so the
//! `zanpos-tauri-repository-mutation-requires-rbac` rule can only match
//! fully-qualified `crate::db::repositories::...` call paths. Imported-path
//! calls are a documented structural baseline (owner: ZANPOS Maintainers,
//! review_by: 2027-01-31): the mutation-verb coverage below is validated on the
//! fully-qualified form Semgrep CAN see, and the import form is documented here
//! as a known CE gap that code review must cover.
use crate::db::repositories::sale_repo;
use sqlx::SqlitePool;

// Fully-qualified mutation call without RBAC: MUST be flagged.
#[tauri::command]
async fn approve_fully_qualified(pool: &SqlitePool) {
    // ruleid: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::sale_repo::approve(pool, "fixture").await;
}

// Imported-path mutation call without RBAC: CE cannot resolve `use`, so this is
// a documented baseline (NOT flagged by Semgrep; must be caught in review).
#[tauri::command]
async fn approve_imported_path(pool: &SqlitePool) {
    // ok: zanpos-tauri-repository-mutation-requires-rbac
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

// A sink in a command function that contains any RBAC authorization is suppressed
// at function scope — statement-sequence patterns cannot cross block boundaries.
#[tauri::command]
async fn sync_fully_qualified_late_guard(pool: &SqlitePool, actor_user_id: &str) {
    // ok: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::sale_repo::sync(pool, "fixture").await;
    rbac::manager_or_owner(pool, actor_user_id).await?;
    Ok(())
}
