#!/usr/bin/env python3
"""Fail-closed validation for finite Semgrep CE security exceptions.

Owner: ZANPOS Maintainers. Review by: 2027-01-31.
Semgrep owns ordinary ordered RBAC flows. This validator owns the reviewed
nested/control-flow occurrences that Community Edition cannot correlate.
"""

from __future__ import annotations

import argparse
import copy
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path

SEP = "\x1f"


class ValidationError(Exception):
    pass


@dataclass
class Token:
    value: str
    line: int


@dataclass
class Function:
    path: str
    name: str
    line: int
    tauri: bool
    body: str


@dataclass(frozen=True)
class Expected:
    id: str
    path: str
    function: str
    control: str
    call: str
    guards: tuple[str, ...]


@dataclass
class Project:
    functions: list[Function]
    lan_literal_count: int


def lex(source: str) -> list[Token]:
    tokens: list[Token] = []
    i = 0
    line = 1
    length = len(source)
    operators = ("..=", "...", "::", "->", "=>", "==", "!=", "<=", ">=", "&&", "||", "..")
    while i < length:
        ch = source[i]
        if ch.isspace():
            line += ch == "\n"
            i += 1
            continue
        if source.startswith("//", i):
            end = source.find("\n", i + 2)
            i = length if end < 0 else end
            continue
        if source.startswith("/*", i):
            start_line = line
            depth = 1
            i += 2
            while i < length and depth:
                if source.startswith("/*", i):
                    depth += 1
                    i += 2
                elif source.startswith("*/", i):
                    depth -= 1
                    i += 2
                else:
                    line += source[i] == "\n"
                    i += 1
            if depth:
                raise ValidationError(f"unterminated block comment at line {start_line}")
            continue
        if ch == "'":
            end = i + 1
            if end < length and source[end] == "\\":
                if source.startswith("\\u{", end):
                    brace = source.find("}", end + 3)
                    end = length if brace < 0 else brace + 1
                else:
                    end += 2
            else:
                end += 1
            if end < length and source[end] == "'":
                end += 1
                tokens.append(Token(source[i:end], line))
                i = end
            else:
                tokens.append(Token(ch, line))
                i += 1
            continue
        raw = re.match(r"(?:b?r)(#{0,255})\"", source[i:])
        if raw:
            hashes = raw.group(1)
            close = '"' + hashes
            end = source.find(close, i + raw.end())
            if end < 0:
                raise ValidationError(f"unterminated raw string at line {line}")
            end += len(close)
            value = source[i:end]
            tokens.append(Token(value, line))
            line += value.count("\n")
            i = end
            continue
        quote_start = i + 1 if source.startswith(('b"', 'c"'), i) else i
        if source[quote_start:quote_start + 1] == '"':
            start = i
            start_line = line
            i = quote_start + 1
            while i < length:
                if source[i] == "\\":
                    i += 2
                elif source[i] == '"':
                    i += 1
                    break
                else:
                    line += source[i] == "\n"
                    i += 1
            else:
                raise ValidationError(f"unterminated string at line {start_line}")
            tokens.append(Token(source[start:i], line - source[start:i].count("\n")))
            continue
        if ("A" <= ch <= "Z") or ("a" <= ch <= "z") or ch == "_":
            match = re.match(r"[A-Za-z_][A-Za-z0-9_]*", source[i:])
            assert match
            value = match.group(0)
            tokens.append(Token(value, line))
            i += len(value)
            continue
        if "0" <= ch <= "9":
            match = re.match(r"[0-9][A-Za-z0-9_\.]*", source[i:])
            assert match
            value = match.group(0)
            tokens.append(Token(value, line))
            i += len(value)
            continue
        operator = next((op for op in operators if source.startswith(op, i)), None)
        if operator:
            tokens.append(Token(operator, line))
            i += len(operator)
            continue
        tokens.append(Token(ch, line))
        i += 1
    return tokens


def canon(source: str) -> str:
    return SEP.join(token.value for token in lex(source))


def joined(tokens: list[Token]) -> str:
    return SEP.join(token.value for token in tokens)


def matching_brace(tokens: list[Token], opening: int) -> int:
    depth = 0
    for index in range(opening, len(tokens)):
        if tokens[index].value == "{":
            depth += 1
        elif tokens[index].value == "}":
            depth -= 1
            if depth == 0:
                return index
    raise ValidationError(f"unclosed function body at line {tokens[opening].line}")


def parse_functions(path: str, source: str) -> list[Function]:
    tokens = lex(source)
    functions: list[Function] = []
    attribute = canon("#[tauri::command]")
    for index, token in enumerate(tokens):
        if token.value != "fn" or index + 1 >= len(tokens):
            continue
        opening = next((i for i in range(index + 2, len(tokens)) if tokens[i].value in ("{", ";")), None)
        if opening is None or tokens[opening].value == ";":
            continue
        closing = matching_brace(tokens, opening)
        prefix = joined(tokens[max(0, index - 20):index])
        functions.append(
            Function(path, tokens[index + 1].value, token.line, attribute in prefix, joined(tokens[opening + 1:closing]))
        )
    return functions


def c(source: str) -> str:
    return canon(source)


AI_PATH = "src-tauri/src/commands/ai_admin_commands.rs"
WATERMARK = c("sqlx::query(\"DELETE FROM app_config WHERE key LIKE 'sync_v2_watermark_%'\")")

EXPECTED = (
    Expected("delivery_update_status", "src-tauri/src/commands/delivery_commands.rs", "delivery_update_status", c("update_delivery_status("), c("delivery_repo::update_delivery_status(&state.db, &input).await?"), (
        c('if input.delivery_status == "cancelled" { rbac::manager_or_owner(&state.db, &input.actor_user_id).await?; } else { rbac::require_role(&state.db, &input.actor_user_id, &["owner", "manager", "cashier"],).await?; }'),
    )),
    Expected("shift_close", "src-tauri/src/commands/shift_commands.rs", "shift_close", c("close_shift("), c("shift_repo::close_shift(&state.db, &input.shift_id, input.counted_cash_minor, input.notes,).await?"), (
        c("if owner_id != input.actor_user_id { rbac::manager_or_owner(&state.db, &input.actor_user_id).await?; }"),
        c("if shift_device_id != active_device { rbac::owner_only(&state.db, &input.actor_user_id).await?; }"),
    )),
    Expected("ai_execute_action_undo", AI_PATH, "ai_execute_action", c("create_undo_record("), c("ai_admin_repo::create_undo_record(&state.db, &input.action_id, &mutation_result.entity_type, &mutation_result.entity_id, &mutation_result.undo_snapshot_json, &mutation_result.rollback_tool, &mutation_result.rollback_input_json,).await?"), (c("let actor = authorize_office(&state, &session_token).await?;"),)),
    Expected("ai_execute_batch_undo", AI_PATH, "ai_execute_batch_actions", c("create_undo_record("), c("ai_admin_repo::create_undo_record(&state.db, action_id, &mutation_result.entity_type, &mutation_result.entity_id, &mutation_result.undo_snapshot_json, &mutation_result.rollback_tool, &mutation_result.rollback_input_json,).await?"), (c("let actor = authorize_office(&state, &session_token).await?;"),)),
    Expected("ai_chat_expire_old", AI_PATH, "ai_chat_stream", c("expire_old_actions("), c("ai_admin_repo::expire_old_actions(&state.db).await.ok()"), (c("let actor = authorize_office(&state, &session_token).await.map_err(|e| e.to_string())?;"),)),
    Expected("ai_chat_create_session", AI_PATH, "ai_chat_stream", c("create_session("), c("ai_admin_repo::create_session(&state.db, &session_id, &input.branch_id, &input.user_id, &provider_name, &model_name,).await.map_err(|e| e.to_string())?"), (c("let actor = authorize_office(&state, &session_token).await.map_err(|e| e.to_string())?;"),)),
    Expected("ai_chat_save_user", AI_PATH, "ai_chat_stream", c("save_message("), c('ai_chat_history_repo::save_message(&state.db, &session_id, &input.branch_id, &input.user_id, "user", persisted_user_content, "text",).await.map_err(|e| e.to_string())?'), (c("let actor = authorize_office(&state, &session_token).await.map_err(|e| e.to_string())?;"),)),
    Expected("ai_chat_end_status", AI_PATH, "ai_chat_stream", c("end_session("), c("ai_admin_repo::end_session(&state.db, &session_id, end_status).await.ok()"), (c("let actor = authorize_office(&state, &session_token).await.map_err(|e| e.to_string())?;"),)),
    Expected("ai_chat_save_assistant", AI_PATH, "ai_chat_stream", c("save_message("), c('ai_chat_history_repo::save_message(&state.db, &session_id, &input.branch_id, &input.user_id, "assistant", text, "text",).await'), (c("let actor = authorize_office(&state, &session_token).await.map_err(|e| e.to_string())?;"),)),
    Expected("ai_chat_end_cancelled", AI_PATH, "ai_chat_stream", c("end_session("), c('ai_admin_repo::end_session(&state.db, &session_id, "cancelled").await.ok()'), (c("let actor = authorize_office(&state, &session_token).await.map_err(|e| e.to_string())?;"),)),
    Expected("hub_connect_watermark", "src-tauri/src/commands/hub_commands.rs", "hub_connect_existing", c("sqlx::query("), c("sqlx::query(\"DELETE FROM app_config WHERE key LIKE 'sync_v2_watermark_%'\").execute(&mut *tx).await?"), (c("rbac::manager_or_owner(&state.db, &actor_user_id).await?;"),)),
    Expected("setup_pull_catalog_watermark", "src-tauri/src/commands/sync_commands.rs", "setup_pull_catalog", c("clear_setup_pull_watermarks("), c("clear_setup_pull_watermarks(&state.db).await?"), (c('if !setup_pull_catalog_allowed(setup_done.as_deref() == Some("1"), hub_url.as_deref()) { return Err(AppError::Permission("Setup is already complete. Use the sync panel to pull catalog updates.".into(),)); }'),)),
    Expected("setup_watermark_helper", "src-tauri/src/commands/sync_commands.rs", "clear_setup_pull_watermarks", c("sqlx::query("), c("sqlx::query(\"DELETE FROM app_config WHERE key LIKE 'sync_v2_watermark_%'\").execute(pool).await?"), ()),
    Expected("force_resync_watermark", "src-tauri/src/commands/sync_commands.rs", "sync_force_full_resync", c("sqlx::query("), c("sqlx::query(\"DELETE FROM app_config WHERE key LIKE 'sync_v2_watermark_%'\").execute(&state.db).await"), (c("crate::commands::rbac::manager_or_owner(&state.db, &actor_user_id).await?;"),)),
    Expected("hub_truth_pull_watermark", "src-tauri/src/commands/sync_commands.rs", "hub_truth_pull", c("sqlx::query("), c("sqlx::query(\"DELETE FROM app_config WHERE key LIKE 'sync_v2_watermark_%'\").execute(&state.db).await?"), (c("rbac::manager_or_owner(&state.db, &actor_user_id).await?;"),)),
)

LAN_ID = "lan_discovery_bind"
LAN_PATH = "src-tauri/src/hub/mod.rs"
LAN_CALL = c('std::net::UdpSocket::bind("0.0.0.0:0")')
LAN_LITERAL = c('"0.0.0.0:0"')
LAN_BRANCH = c('''
for probe in ["8.8.8.8:80", "192.168.1.1:80", "10.0.0.1:80"] {
    if let Ok(s) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if s.connect(probe).is_ok() {
            if let Ok(a) = s.local_addr() {
                let ip = a.ip().to_string();
                if ip != "0.0.0.0" && !out.contains(&ip) {
                    out.push(ip);
                }
            }
        }
    }
}
''')
CONTROL_IDS = {item.id for item in EXPECTED}


def load_project(root: Path) -> Project:
    functions: list[Function] = []
    literal_count = 0
    rust_root = root / "src-tauri" / "src"
    if not rust_root.is_dir():
        raise ValidationError(f"missing Rust source root: {rust_root}")
    for file in sorted(rust_root.rglob("*.rs")):
        source = file.read_text(encoding="utf-8")
        relative = file.relative_to(root).as_posix()
        functions.extend(parse_functions(relative, source))
        literal_count += joined(lex(source)).count(LAN_LITERAL)
    return Project(functions, literal_count)


def locate(project: Project, expected: Expected) -> Function | None:
    matches = [fn for fn in project.functions if fn.path == expected.path and fn.name == expected.function]
    return matches[0] if len(matches) == 1 else None


def validate(project: Project) -> list[str]:
    errors: list[str] = []
    for expected in EXPECTED:
        function = locate(project, expected)
        if function is None:
            errors.append(f"{expected.id}: expected exactly one {expected.path}::{expected.function}")
            continue
        count = function.body.count(expected.call)
        if count != 1:
            errors.append(f"{expected.id}: expected one exact sink, found {count}")
            continue
        sink_at = function.body.find(expected.call)
        for guard in expected.guards:
            guard_at = function.body.find(guard)
            if guard_at < 0:
                errors.append(f"{expected.id}: required authorization/guard shape is absent")
            elif guard_at >= sink_at:
                errors.append(f"{expected.id}: sink is not dominated by its authorization/guard")

    for function in project.functions:
        start = 0
        while (found := function.body.find(WATERMARK, start)) >= 0:
            allowed = any(
                WATERMARK in item.call and item.path == function.path and item.function == function.name
                for item in EXPECTED
            )
            if not allowed:
                errors.append(f"{function.path}:{function.line}::{function.name}: unexpected watermark mutation")
            start = found + len(WATERMARK)
        if not function.tauri:
            continue
        for control in sorted({item.control for item in EXPECTED if item.control != c("sqlx::query(")}):
            start = 0
            while (found := function.body.find(control, start)) >= 0:
                allowed = any(
                    item.control == control
                    and item.path == function.path
                    and item.function == function.name
                    and function.body.startswith(item.call, found - max(0, item.call.find(control)))
                    for item in EXPECTED
                )
                if not allowed:
                    errors.append(f"{function.path}:{function.line}::{function.name}: unexpected controlled mutation {control.replace(SEP, '')}")
                start = found + len(control)

    lan_functions = [fn for fn in project.functions if fn.path == LAN_PATH and fn.name == "lan_ips"]
    if len(lan_functions) != 1:
        errors.append(f"{LAN_ID}: expected exactly one {LAN_PATH}::lan_ips")
    else:
        body = lan_functions[0].body
        if body.count(LAN_BRANCH) != 1:
            errors.append(f"{LAN_ID}: reviewed UDP bind/connect/local-address branch changed")
    if project.lan_literal_count != 1:
        errors.append(f"{LAN_ID}: expected one 0.0.0.0:0 literal in Rust sources, found {project.lan_literal_count}")
    return errors


def mutate(project: Project, case_id: str, mutation: str) -> Project:
    changed = copy.deepcopy(project)
    if case_id == LAN_ID:
        if mutation not in ("copied_command", "additional_same_function"):
            raise ValidationError(f"unsupported LAN fixture mutation: {mutation}")
        changed.lan_literal_count += 1
        if mutation == "additional_same_function":
            fn = next(fn for fn in changed.functions if fn.path == LAN_PATH and fn.name == "lan_ips")
            fn.body += SEP + LAN_CALL
        return changed
    expected = next((item for item in EXPECTED if item.id == case_id), None)
    if expected is None:
        raise ValidationError(f"unknown fixture sink: {case_id}")
    function = locate(changed, expected)
    if function is None:
        raise ValidationError(f"fixture source missing for {case_id}")
    if case_id == "setup_pull_catalog_watermark":
        if mutation == "before_authorization":
            function.body = WATERMARK + SEP + function.body.replace(expected.call, "", 1)
        elif mutation == "additional_same_function":
            function.body += SEP + WATERMARK
        elif mutation == "copied_command":
            changed.functions.append(Function("src-tauri/src/commands/copied_fixture.rs", "copied_setup_watermark", 1, True, WATERMARK))
        else:
            raise ValidationError(f"unknown fixture mutation: {mutation}")
        return changed
    if mutation == "before_authorization":
        function.body = expected.call + SEP + function.body.replace(expected.call, "", 1)
    elif mutation == "additional_same_function":
        function.body = function.body.replace(expected.call, expected.call + SEP + expected.call, 1)
    elif mutation == "copied_command":
        changed.functions.append(Function("src-tauri/src/commands/copied_fixture.rs", f"copied_{case_id}", 1, True, expected.call))
    else:
        raise ValidationError(f"unknown fixture mutation: {mutation}")
    return changed


def load_fixture_cases(path: Path) -> list[tuple[str, str]]:
    data = json.loads(path.read_text(encoding="utf-8"))
    if data.get("owner") != "ZANPOS Maintainers" or data.get("review_by") != "2027-01-31":
        raise ValidationError("fixture metadata owner/review_by mismatch")
    cases: list[tuple[str, str]] = []
    for item in data.get("sinks", []):
        for mutation in item.get("invalid", []):
            cases.append((item["id"], mutation))
    declared = {item["id"] for item in data.get("sinks", [])}
    required = (CONTROL_IDS - {"ai_execute_batch_undo", "hub_connect_watermark", "setup_watermark_helper", "force_resync_watermark", "hub_truth_pull_watermark"}) | {LAN_ID}
    if declared != required:
        raise ValidationError(f"fixture sink inventory mismatch: expected {sorted(required)}, got {sorted(declared)}")
    return cases


def print_errors(errors: list[str]) -> None:
    for error in errors:
        print(f"ERROR: {error}", file=sys.stderr)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--fixtures", type=Path)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--fixture-case", help="valid or <sink-id>:<mutation>")
    args = parser.parse_args()
    try:
        project = load_project(args.root.resolve())
        cases = load_fixture_cases(args.fixtures) if args.fixtures else []
        if args.fixture_case and args.fixture_case != "valid":
            case_id, separator, mutation_name = args.fixture_case.partition(":")
            if not separator or (case_id, mutation_name) not in cases:
                raise ValidationError(f"undeclared fixture case: {args.fixture_case}")
            project = mutate(project, case_id, mutation_name)
        errors = validate(project)
        if errors:
            print_errors(errors)
            return 1
        if args.self_test:
            if not args.fixtures:
                raise ValidationError("--self-test requires --fixtures")
            failures = []
            for case_id, mutation_name in cases:
                if not validate(mutate(project, case_id, mutation_name)):
                    failures.append(f"{case_id}:{mutation_name}")
            if failures:
                raise ValidationError(f"negative fixtures unexpectedly passed: {failures}")
            print(f"PASS: production policy and {len(cases)} negative fixture mutations validated")
        else:
            print("PASS: finite Semgrep security exceptions validated")
        return 0
    except (OSError, ValueError, ValidationError, StopIteration) as error:
        print(f"ERROR: validator failed closed: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
