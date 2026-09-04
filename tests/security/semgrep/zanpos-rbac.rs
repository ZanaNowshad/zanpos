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
    // ruleid: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::sale_repo::approve(pool, "before-auth").await;
    if elevated {
        rbac::manager_or_owner(pool, actor_user_id).await?;
    } else {
        rbac::require_any_role(pool, actor_user_id).await?;
    }
    // ok: zanpos-tauri-repository-mutation-requires-rbac
    let _result = delivery_repo::update_delivery_status(&state.db, &input).await?;
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
    // ruleid: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::sale_repo::approve(pool, "before-auth").await;
    if owner_user_id != actor_user_id {
        rbac::manager_or_owner(pool, actor_user_id).await?;
    }
    if shift_device_id != active_device_id {
        rbac::owner_only(pool, actor_user_id).await?;
    }
    use crate::db::repositories::shift_repo;
    // ok: zanpos-tauri-repository-mutation-requires-rbac
    let _closed = shift_repo::close_shift(pool, "fixture").await?;
    Ok(())
}

#[tauri::command]
async fn ai_execute_action(pool: &SqlitePool, actor_user_id: &str) -> Result<(), sqlx::Error> {
    // ruleid: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::sale_repo::approve(pool, "before-auth").await;
    let _actor = authorize_office(pool, actor_user_id).await?;
    if actor_user_id.is_empty() {
        return Err(sqlx::Error::RowNotFound);
    }
    // ok: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::ai_admin_repo::mark_executed(pool, "fixture").await;
    // ok: zanpos-tauri-repository-mutation-requires-rbac
    ai_admin_repo::create_undo_record(
        &state.db,
        &input.action_id,
        &mutation_result.entity_type,
        &mutation_result.entity_id,
        &mutation_result.undo_snapshot_json,
        &mutation_result.rollback_tool,
        &mutation_result.rollback_input_json,
    )
    .await;
    Ok(())
}

#[tauri::command]
async fn ai_chat_stream(pool: &SqlitePool, actor_user_id: &str) -> Result<(), sqlx::Error> {
    // ruleid: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::sale_repo::approve(pool, "before-auth").await;
    let _actor = authorize_office(pool, actor_user_id)
        .await
        .map_err(|_| sqlx::Error::RowNotFound)?;
    let request = async {
        // ok: zanpos-tauri-repository-mutation-requires-rbac
        ai_admin_repo::expire_old_actions(&state.db).await;
        // ok: zanpos-tauri-repository-mutation-requires-rbac
        ai_admin_repo::create_session(
            &state.db,
            &session_id,
            &input.branch_id,
            &input.user_id,
            &provider_name,
            &model_name,
        )
        .await;
        // ok: zanpos-tauri-repository-mutation-requires-rbac
        ai_chat_history_repo::save_message(
            &state.db,
            &session_id,
            &input.branch_id,
            &input.user_id,
            "user",
            persisted_user_content,
            "text",
        )
        .await;
        // ok: zanpos-tauri-repository-mutation-requires-rbac
        ai_admin_repo::end_session(&state.db, &session_id, end_status).await;
        // ok: zanpos-tauri-repository-mutation-requires-rbac
        ai_chat_history_repo::save_message(
            &state.db,
            &session_id,
            &input.branch_id,
            &input.user_id,
            "assistant",
            text,
            "text",
        )
        .await;
        // ok: zanpos-tauri-repository-mutation-requires-rbac
        ai_admin_repo::end_session(&state.db, &session_id, "cancelled").await;
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

#[tauri::command]
async fn resume_without_rbac(pool: &SqlitePool) {
    // ruleid: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::held_cart_repo::resume_held_cart(pool, "cart", "shift").await;
}

#[tauri::command]
async fn resume_with_rbac(pool: &SqlitePool, actor_user_id: &str) -> Result<(), sqlx::Error> {
    rbac::require_any_role(pool, actor_user_id).await?;
    // ok: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::held_cart_repo::resume_held_cart(pool, "cart", "shift").await;
    Ok(())
}

#[tauri::command]
async fn resume_with_late_rbac(
    pool: &SqlitePool,
    actor_user_id: &str,
) -> Result<(), sqlx::Error> {
    // ruleid: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::held_cart_repo::resume_held_cart(pool, "cart", "shift").await;
    rbac::require_any_role(pool, actor_user_id).await?;
    Ok(())
}

async fn resume_held_cart_mutation(pool: &SqlitePool) {
    use crate::db::repositories::held_cart_repo as held_persistence;
    held_persistence::resume_held_cart(pool, "cart", "shift").await;
}

#[tauri::command]
async fn resume_through_alias_wrapper(pool: &SqlitePool) {
    // ruleid: zanpos-tauri-repository-mutation-requires-rbac
    resume_held_cart_mutation(pool).await;
}

#[tauri::command]
async fn resume_through_guarded_alias_wrapper(
    pool: &SqlitePool,
    actor_user_id: &str,
) -> Result<(), sqlx::Error> {
    rbac::require_any_role(pool, actor_user_id).await?;
    // ok: zanpos-tauri-repository-mutation-requires-rbac
    resume_held_cart_mutation(pool).await;
    Ok(())
}

#[tauri::command]
async fn setup_pull_catalog(pool: &SqlitePool, allowed: bool) -> Result<(), sqlx::Error> {
    // ruleid: zanpos-tauri-mutation-requires-rbac
    sqlx::query("DELETE FROM unrelated_rows")
        .execute(pool)
        .await?;
    if !setup_pull_catalog_allowed(allowed) {
        return Err(sqlx::Error::RowNotFound);
    }
    clear_setup_pull_watermarks(pool).await?;
    Ok(())
}

// ── Manager approval proven by a consumed override token ─────────────────────
//
// These two pin the difference the rule has to be able to see. Both commands
// end in the same repository mutation; only the way the approver is
// established differs.

/// UNSAFE: the approver is a string the caller chose. Naming a manager is not
/// the same as one being present, and the audit row would record them anyway.
#[tauri::command]
async fn void_with_payload_approver(
    pool: &SqlitePool,
    authorized_by_user_id: &str,
) -> Result<(), sqlx::Error> {
    let _ = authorized_by_user_id;
    // ruleid: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::sale_repo::approve(pool, "fixture").await;
    Ok(())
}

/// SAFE: the approver comes back from a single-use token that only
/// `auth_validate_manager_pin` can mint, so a manager's PIN was entered at the
/// till. The role is re-checked inside `manager_approval`.
#[tauri::command]
async fn void_with_consumed_manager_approval(
    pool: &SqlitePool,
    manager_override_token: &str,
) -> Result<(), sqlx::Error> {
    let _approved_by = manager_approval(pool, Some(manager_override_token)).await?;
    // ok: zanpos-tauri-repository-mutation-requires-rbac
    crate::db::repositories::sale_repo::approve(pool, "fixture").await;
    Ok(())
}
