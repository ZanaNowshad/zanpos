use sqlx::SqlitePool;

#[tauri::command]
async fn update_without_rbac(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // ruleid: zanpos-tauri-mutation-requires-rbac
    sqlx::query("UPDATE products SET is_active = 0")
        .execute(pool)
        .await?;
    Ok(())
}

#[tauri::command]
async fn update_with_rbac(pool: &SqlitePool, actor_user_id: &str) -> Result<(), sqlx::Error> {
    rbac::manager_or_owner(pool, actor_user_id).await?;
    // ok: zanpos-tauri-mutation-requires-rbac
    sqlx::query("UPDATE products SET is_active = 1")
        .execute(pool)
        .await?;
    Ok(())
}

#[tauri::command]
async fn update_with_late_rbac(pool: &SqlitePool, actor_user_id: &str) -> Result<(), sqlx::Error> {
    // ruleid: zanpos-tauri-mutation-requires-rbac
    sqlx::query("UPDATE products SET is_active = 1").execute(pool).await?;
    rbac::manager_or_owner(pool, actor_user_id).await?;
    Ok(())
}

#[tauri::command]
async fn update_with_nested_rbac(pool: &SqlitePool, actor_user_id: &str) -> Result<(), sqlx::Error> {
    if actor_user_id.is_empty() {
        return Err(sqlx::Error::RowNotFound);
    }
    rbac::manager_or_owner(pool, actor_user_id).await?;
    // ok: zanpos-tauri-mutation-requires-rbac
    sqlx::query("UPDATE products SET is_active = 1").execute(pool).await?;
    Ok(())
}

#[tauri::command]
async fn update_mixed_order(pool: &SqlitePool, actor_user_id: &str) -> Result<(), sqlx::Error> {
    // ruleid: zanpos-tauri-mutation-requires-rbac
    sqlx::query("UPDATE products SET is_active = 0").execute(pool).await?;
    rbac::manager_or_owner(pool, actor_user_id).await?;
    // ok: zanpos-tauri-mutation-requires-rbac
    sqlx::query("UPDATE products SET is_active = 1").execute(pool).await?;
    Ok(())
}

#[tauri::command]
async fn approve_with_repo_without_rbac(pool: &SqlitePool) {
    // ruleid: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::sale_repo::approve(pool, "fixture").await;
}

#[tauri::command]
async fn approve_with_repo_rbac(pool: &SqlitePool, actor_user_id: &str) -> Result<(), sqlx::Error> {
    rbac::manager_or_owner(pool, actor_user_id).await?;
    // ok: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::sale_repo::approve(pool, "fixture").await;
    Ok(())
}

#[tauri::command]
async fn approve_with_repo_nested_rbac(
    pool: &SqlitePool,
    actor_user_id: &str,
) -> Result<(), sqlx::Error> {
    if actor_user_id.is_empty() {
        return Err(sqlx::Error::RowNotFound);
    }
    rbac::manager_or_owner(pool, actor_user_id).await?;
    // ok: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::sale_repo::approve(pool, "fixture").await;
    Ok(())
}

#[tauri::command]
async fn delivery_update_status(
    pool: &SqlitePool,
    actor_user_id: &str,
    elevated: bool,
) -> Result<(), sqlx::Error> {
    if elevated {
        rbac::manager_or_owner(pool, actor_user_id).await?;
    } else {
        rbac::require_any_role(pool, actor_user_id).await?;
    }
    // ok: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::sale_repo::approve(pool, "fixture").await;
    Ok(())
}

#[tauri::command]
async fn shift_close(
    pool: &SqlitePool,
    actor_user_id: &str,
    owner_user_id: &str,
    shift_device_id: &str,
    active_device_id: &str,
) -> Result<(), sqlx::Error> {
    if owner_user_id != actor_user_id {
        rbac::manager_or_owner(pool, actor_user_id).await?;
    }
    if shift_device_id != active_device_id {
        rbac::owner_only(pool, actor_user_id).await?;
    }
    // ok: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::shift_repo::close(pool, "fixture").await;
    Ok(())
}

#[tauri::command]
async fn ai_execute_action(pool: &SqlitePool, actor_user_id: &str) -> Result<(), sqlx::Error> {
    let _actor = authorize_office(pool, actor_user_id).await?;
    if actor_user_id.is_empty() {
        return Err(sqlx::Error::RowNotFound);
    }
    // ok: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::ai_admin_repo::mark_executed(pool, "fixture").await;
    Ok(())
}

#[tauri::command]
async fn ai_chat_stream(pool: &SqlitePool, actor_user_id: &str) -> Result<(), sqlx::Error> {
    let _actor = authorize_office(pool, actor_user_id)
        .await
        .map_err(|_| sqlx::Error::RowNotFound)?;
    let request = async {
        // ok: zanpos-tauri-repository-mutation-requires-rbac
        crate::db::repositories::ai_admin_repo::create_session(pool, "fixture").await;
    };
    request.await;
    Ok(())
}

#[tauri::command]
async fn rollback_with_late_rbac(pool: &SqlitePool, actor_user_id: &str) -> Result<(), sqlx::Error> {
    // ruleid: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::sale_repo::rollback(pool, "fixture").await;
    rbac::manager_or_owner(pool, actor_user_id).await?;
    Ok(())
}

#[tauri::command]
async fn approve_with_imported_repo(pool: &SqlitePool) {
    use crate::db::repositories::sale_repo;
    // ruleid: zanpos-tauri-repository-mutation-requires-rbac
    sale_repo::approve(pool, "fixture").await;
}

#[tauri::command]
async fn approve_with_local_alias(pool: &SqlitePool) {
    use crate::db::repositories::sale_repo as persistence;
    // ruleid: zanpos-tauri-repository-mutation-requires-rbac
    persistence::approve(pool, "fixture").await;
}

async fn persist_sale_wrapper(pool: &SqlitePool) {
    crate::db::repositories::sale_repo::approve(pool, "fixture").await;
}

#[tauri::command]
async fn approve_through_wrapper(pool: &SqlitePool) {
    // ruleid: zanpos-tauri-repository-mutation-requires-rbac
    persist_sale_wrapper(pool).await;
}

#[tauri::command]
async fn approve_through_guarded_wrapper(
    pool: &SqlitePool,
    actor_user_id: &str,
) -> Result<(), sqlx::Error> {
    rbac::manager_or_owner(pool, actor_user_id).await?;
    // ok: zanpos-tauri-repository-mutation-requires-rbac
    persist_sale_wrapper(pool).await;
    Ok(())
}
