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
    Expected("ai_chat_save_user", AI_PATH, "ai_chat_stream", c("ai_chat_history_repo::save_message("), c('ai_chat_history_repo::save_message(&state.db, &session_id, &input.branch_id, &input.user_id, "user", persisted_user_content, "text",).await.map_err(|e| e.to_string())?')),
    Expected("ai_chat_end_status", AI_PATH, "ai_chat_stream", c("ai_admin_repo::end_session("), c("ai_admin_repo::end_session(&state.db, &session_id, end_status).await.ok()")),
    Expected("ai_chat_save_assistant", AI_PATH, "ai_chat_stream", c("ai_chat_history_repo::save_message("), c('ai_chat_history_repo::save_message(&state.db, &session_id, &input.branch_id, &input.user_id, "assistant", text, "text",).await')),
    Expected("ai_chat_end_cancelled", AI_PATH, "ai_chat_stream", c("ai_admin_repo::end_session("), c('ai_admin_repo::end_session(&state.db, &session_id, "cancelled").await.ok()')),
    Expected("hub_connect_watermark", "src-tauri/src/commands/hub_commands.rs", "hub_connect_existing", WATERMARK, c("sqlx::query(\"DELETE FROM app_config WHERE key LIKE 'sync_v2_watermark_%'\").execute(&mut *tx).await?")),
    Expected("setup_pull_catalog_watermark", "src-tauri/src/commands/sync_commands.rs", "setup_pull_catalog", c("clear_setup_pull_watermarks("), c("clear_setup_pull_watermarks(&state.db).await?")),
    Expected("setup_watermark_helper", "src-tauri/src/commands/sync_commands.rs", "clear_setup_pull_watermarks", WATERMARK, c("sqlx::query(\"DELETE FROM app_config WHERE key LIKE 'sync_v2_watermark_%'\").execute(pool).await?")),
    Expected("force_resync_watermark", "src-tauri/src/commands/sync_commands.rs", "sync_force_full_resync", WATERMARK, c("sqlx::query(\"DELETE FROM app_config WHERE key LIKE 'sync_v2_watermark_%'\").execute(&state.db).await")),
    Expected("hub_truth_pull_watermark", "src-tauri/src/commands/sync_commands.rs", "hub_truth_pull", WATERMARK, c("sqlx::query(\"DELETE FROM app_config WHERE key LIKE 'sync_v2_watermark_%'\").execute(&state.db).await?")),
)

FUNCTION_HASHES = {
    ("src-tauri/src/commands/delivery_commands.rs", "delivery_update_status"): "41266bafaeb9cc9f0ffa18e14cb22be296036c96c76646b58808a097b2bfe432",
    ("src-tauri/src/commands/shift_commands.rs", "shift_close"): "d1b32cb313b7860bc9403c5459100bebcdde31481b15e6b18cf029bb4c34c99e",
    (AI_PATH, "ai_execute_action"): "d087b9fca2bd6db0c3fcbd04fb3f804666aeb7d4243209675198b08aee3cb8b9",
    (AI_PATH, "ai_execute_batch_actions"): "7e0cc918f02ed0c6cf78a1b6ce0eae53eabe1348cebeffc521e549d267e9da96",
    (AI_PATH, "ai_chat_stream"): "a591ffcae937a73bfd9f19dc5dc562b441f0e1f75efc50b99076d24996a5bb7c",
    ("src-tauri/src/commands/hub_commands.rs", "hub_connect_existing"): "d9ae75485d68938f5f6292bef7554d0fd020fb043e765837442bc6ea69c75e9c",
    ("src-tauri/src/commands/sync_commands.rs", "setup_pull_catalog"): "ad432e47cd3136fbbdf88ff4d5e348a7a7f79727de331835001f6fba619a8515",
    ("src-tauri/src/commands/sync_commands.rs", "clear_setup_pull_watermarks"): "fcc5837eaa21e52f9b17f086824d29e8d2b6b7644a764dca2272a85b6447db3c",
    ("src-tauri/src/commands/sync_commands.rs", "sync_force_full_resync"): "7446240c721c756a1e9c44996ebf3fb0c345bb602386cb465f135c17346ed754",
    ("src-tauri/src/commands/sync_commands.rs", "hub_truth_pull"): "e0c2302a383659b39e9b72ddc658f8daff2de7aaa4f453888abe2bdffe73176a",
    ("src-tauri/src/ai/tool_policy.rs", "execute_automatic_mutation"): "e0850fcac23fc34601956b2c30cbc540cb67a80cce6362b75fea484c02b4a1d3",
}
CONTROL_METHODS = {"create_undo_record", "expire_old_actions", "create_session", "save_message", "end_session"}
CONTROL_INVENTORY_SHA256 = "9350c3d022349bde52a4558bccc39542ba76052a1f76c26232fc130d1be63ad6"
EXTRA_SINKS = (("src-tauri/src/ai/tool_policy.rs", "execute_automatic_mutation", c("ai_admin_repo::create_undo_record(")),
               ("src-tauri/src/commands/report_commands.rs", "test_eod_cashup_cashier_name_populated", c("shift_repo::close_shift(")))
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
