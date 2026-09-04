from __future__ import annotations

from .core import Expected
from .lexer import c

AI_PATH = "src-tauri/src/commands/ai_admin_commands.rs"
WATERMARK = c("sqlx::query(\"DELETE FROM app_config WHERE key LIKE 'sync_v2_watermark_%'\")")
EXPECTED = (
    Expected("delivery_update_status", "src-tauri/src/commands/delivery_commands.rs", "delivery_update_status", c("delivery_repo::update_delivery_status("), c("delivery_repo::update_delivery_status(&state.db, &input).await?")),
    Expected("shift_close", "src-tauri/src/commands/shift_commands.rs", "shift_close", c("shift_repo::close_shift("), c("shift_repo::close_shift(&state.db, &input.shift_id, input.counted_cash_minor, input.notes,).await?")),
    Expected("ai_execute_action_undo", AI_PATH, "ai_execute_action", c("ai_admin_repo::create_undo_record("), c("ai_admin_repo::create_undo_record(&state.db, &input.action_id, &mutation_result.entity_type, &mutation_result.entity_id, &mutation_result.undo_snapshot_json, &mutation_result.rollback_tool, &mutation_result.rollback_input_json,).await?")),
    Expected("ai_execute_batch_undo", AI_PATH, "ai_execute_batch_actions", c("ai_admin_repo::create_undo_record("), c("ai_admin_repo::create_undo_record(&state.db, action_id, &mutation_result.entity_type, &mutation_result.entity_id, &mutation_result.undo_snapshot_json, &mutation_result.rollback_tool, &mutation_result.rollback_input_json,).await?")),
    Expected("ai_chat_expire_old", AI_PATH, "ai_chat_stream", c("ai_admin_repo::expire_old_actions("), c("ai_admin_repo::expire_old_actions(&state.db).await.ok()")),
    Expected("ai_chat_create_session", AI_PATH, "ai_chat_stream", c("ai_admin_repo::create_session("), c("ai_admin_repo::create_session(&state.db, &session_id, &input.branch_id, &input.user_id, &provider_name, &model_name,).await.map_err(|e| e.to_string())?")),
    # Re-reviewed 2026-09-04: `&conversation_id` was inserted after
    # `&session_id` when conversation threading was added. Every attribution
    # argument — branch, user, role literal, content, type — is unchanged and
    # in the same order, so the audited write still records who said what.
    Expected("ai_chat_save_user", AI_PATH, "ai_chat_stream", c("ai_chat_history_repo::save_message("), c('ai_chat_history_repo::save_message(&state.db, &session_id, &conversation_id, &input.branch_id, &input.user_id, "user", persisted_user_content, "text",).await.map_err(|e| e.to_string())?')),
    Expected("ai_chat_end_status", AI_PATH, "ai_chat_stream", c("ai_admin_repo::end_session("), c("ai_admin_repo::end_session(&state.db, &session_id, end_status).await.ok()")),
    Expected("ai_chat_save_assistant", AI_PATH, "ai_chat_stream", c("ai_chat_history_repo::save_message("), c('ai_chat_history_repo::save_message(&state.db, &session_id, &conversation_id, &input.branch_id, &input.user_id, "assistant", text, "text",).await')),
    Expected("ai_chat_end_cancelled", AI_PATH, "ai_chat_stream", c("ai_admin_repo::end_session("), c('ai_admin_repo::end_session(&state.db, &session_id, "cancelled").await.ok()')),
    # Re-reviewed 2026-09-04: hub_connect_existing moved from hub_commands.rs
    # to hub_join_commands.rs. The watermark-clearing call is byte-identical to
    # the shape reviewed before — only the file changed.
    Expected("hub_connect_watermark", "src-tauri/src/commands/hub_join_commands.rs", "hub_connect_existing", WATERMARK, c("sqlx::query(\"DELETE FROM app_config WHERE key LIKE 'sync_v2_watermark_%'\").execute(&mut *tx).await?")),
    Expected("setup_pull_catalog_watermark", "src-tauri/src/commands/sync_commands.rs", "setup_pull_catalog", c("clear_setup_pull_watermarks("), c("clear_setup_pull_watermarks(&state.db).await?")),
    Expected("setup_watermark_helper", "src-tauri/src/commands/sync_commands.rs", "clear_setup_pull_watermarks", WATERMARK, c("sqlx::query(\"DELETE FROM app_config WHERE key LIKE 'sync_v2_watermark_%'\").execute(pool).await?")),
    Expected("force_resync_watermark", "src-tauri/src/commands/sync_commands.rs", "sync_force_full_resync", WATERMARK, c("sqlx::query(\"DELETE FROM app_config WHERE key LIKE 'sync_v2_watermark_%'\").execute(&state.db).await")),
    Expected("hub_truth_pull_watermark", "src-tauri/src/commands/sync_commands.rs", "hub_truth_pull", WATERMARK, c("sqlx::query(\"DELETE FROM app_config WHERE key LIKE 'sync_v2_watermark_%'\").execute(&state.db).await?")),
)

FUNCTION_HASHES = {
    ("src-tauri/src/commands/delivery_commands.rs", "delivery_update_status"): "41266bafaeb9cc9f0ffa18e14cb22be296036c96c76646b58808a097b2bfe432",
    # Re-reviewed 2026-09-03 with the correctness audit. In all three the
    # declared sink call is byte-identical to what it was — SINK_SHAPE still
    # passes — so what moved is the surrounding body, not the controlled call:
    #   shift_close             the pending-COD-delivery guard filtered on
    #                           delivery_orders.shift_id, a column that does not
    #                           exist; unwrap_or(0) read the resulting SQL error
    #                           as "nothing pending", so the guard had never
    #                           fired. It now joins through sales.shift_id.
    #   ai_execute_action       check-then-act race between reading an action's
    #                           status and executing it, closed under one tx.
    #   ai_execute_batch_actions same race on the batch path.
    ("src-tauri/src/commands/shift_commands.rs", "shift_close"): "b4f2df61423fb116475a3e51d3c5f9ff94604cc32b30c412a6efb49e93ee5d52",
    (AI_PATH, "ai_execute_action"): "58c07f4bc6bf050be1b73b2d84bcc0c685cea6a0b608dddb8bf5d8bc91de7937",
    (AI_PATH, "ai_execute_batch_actions"): "7b601798a28a8387debaf0750dcc62aae1279f6df186fa41fc6498a3e3cb73b9",
    (AI_PATH, "ai_chat_stream"): "d22b227c325e1c87fa9123216fa4d249e3558c7df5c213016a66d15fc0e7ffab",
    ("src-tauri/src/commands/hub_join_commands.rs", "hub_connect_existing"): "c04307bfa57057ebbbce48f69f44dbd7b79e4fed748959efec0f627a5e9fce7e",
    ("src-tauri/src/commands/sync_commands.rs", "setup_pull_catalog"): "2d780065c54e6b0a1bead514152e8b4aee555d362a91068f2ac2f1f439ac945b",
    ("src-tauri/src/commands/sync_commands.rs", "clear_setup_pull_watermarks"): "fcc5837eaa21e52f9b17f086824d29e8d2b6b7644a764dca2272a85b6447db3c",
    ("src-tauri/src/commands/sync_commands.rs", "sync_force_full_resync"): "108b4b6c0ef26e6dad190178658789ff8808e1aea9b7fed931789231e341c1fa",
    ("src-tauri/src/commands/sync_commands.rs", "hub_truth_pull"): "e0c2302a383659b39e9b72ddc658f8daff2de7aaa4f453888abe2bdffe73176a",
    ("src-tauri/src/ai/tool_policy.rs", "execute_automatic_mutation"): "970d9d96b112315efe197e6a60cfeb1f557d4f2cf0dbffffc5527bed53c91120",
}
CONTROL_METHODS = {"create_undo_record", "expire_old_actions", "create_session", "save_message", "end_session"}
CONTROL_INVENTORY_SHA256 = "5864ddeed4f46a16172f5c70c86e722ea276654b21a9c42f7db0a55ab2405b1f"
EXTRA_SINKS = (("src-tauri/src/ai/tool_policy.rs", "execute_automatic_mutation", c("ai_admin_repo::create_undo_record(")),
               ("src-tauri/src/commands/report_commands.rs", "test_eod_cashup_cashier_name_populated", c("shift_repo::close_shift(")),
               # Added 2026-09-04. The assistant's shift-close used to be its
               # own UPDATE, which set no expected/variance figures and had no
               # `status = 'open'` guard, so a closed drawer could be silently
               # recounted. It now delegates to the same audited repository
               # function the POS uses — one authoritative close path, which is
               # a new controlled sink precisely because it is the fix.
               ("src-tauri/src/ai/tools_write_ext.rs", "execute", c("shift_repo::close_shift(")),
               # Added 2026-09-04. device_rekey clears the per-table watermarks
               # when a cloned database is given a fresh identity; without it
               # the new terminal would inherit the old one's sync position.
               ("src-tauri/src/device_identity.rs", "rekey_to_fresh", WATERMARK))
LAN_ID, LAN_PATH = "lan_discovery_bind", "src-tauri/src/hub/mod.rs"
LAN_LITERAL = c('"0.0.0.0:0"')
LAN_BRANCH = c('''for probe in ["8.8.8.8:80", "192.168.1.1:80", "10.0.0.1:80"] { if let Ok(s) = std::net::UdpSocket::bind("0.0.0.0:0") { if s.connect(probe).is_ok() { if let Ok(a) = s.local_addr() { let ip = a.ip().to_string(); if ip != "0.0.0.0" && !out.contains(&ip) { out.push(ip); } } } } }''')
SETUP_GUARD = c('''if !setup_pull_catalog_allowed(setup_done.as_deref() == Some("1"), hub_url.as_deref()) { return Err(AppError::Permission("Setup is already complete. Use the sync panel to pull catalog updates.".into(),)); }''')
AI_IDS = {item.id for item in EXPECTED if item.id.startswith("ai_")}

BASE_MUTATIONS = {"before_authorization": "CONTROL_FLOW_CONTRACT", "copied_command": "SINK_INVENTORY", "additional_same_function": "SINK_INVENTORY"}
AI_MUTATIONS = {"copied_non_tauri_helper": "CONTROL_REFERENCE_INVENTORY", "copied_parameterized_tauri": "CONTROL_REFERENCE_INVENTORY", "copied_long_attribute": "CONTROL_REFERENCE_INVENTORY", "aliased_reference": "CONTROL_REFERENCE_INVENTORY", "wrapper_reference": "CONTROL_REFERENCE_INVENTORY"}
SETUP_MUTATIONS = {"false_guard": "CONTROL_FLOW_CONTRACT", "alternate_branch": "CONTROL_FLOW_CONTRACT", "guard_after_helper": "CONTROL_FLOW_CONTRACT", "duplicated_helper": "SINK_INVENTORY", "copied_helper_call": "SINK_INVENTORY"}
LEXER_CASES = {
    "nested_comment": ("PASS", "fn edge() { /* outer /* inner */ done */ let x = 1; }"),
    "raw_byte_c_strings": ("PASS", 'fn edge() { let _ = r###"save_message(/*not code*/)"###; let _ = br#"bytes"#; let _ = c"c-string"; }'),
    "char_and_lifetime": ("PASS", "fn edge<'a>(x: &'a str) { let _ = 'x'; let _ = b'y'; let _: &'a str = x; }"),
    "unterminated_comment": ("LEX_UNTERMINATED_COMMENT", "fn edge() { /* nested /* still open */ }"),
    "unterminated_raw": ("LEX_UNTERMINATED_RAW", 'fn edge() { let _ = r#"open"; }'),
    "unterminated_string": ("LEX_UNTERMINATED_STRING", 'fn edge() { let _ = "open; }'),
    "unbalanced_delimiter": ("PARSE_UNBALANCED_DELIMITER", "fn edge() { let _ = (1 + 2; }"),
}
