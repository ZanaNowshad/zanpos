from __future__ import annotations

import json
import re
from pathlib import Path

from .core import Case, Expected, Project, ValidationError, SEP
from .lexer import lex, c
from .parser import locate, replace_source, canonical_span
from .contracts import (
    EXPECTED, LAN_ID, LAN_PATH, LAN_LITERAL, SETUP_GUARD, AI_IDS,
    BASE_MUTATIONS, AI_MUTATIONS, SETUP_MUTATIONS, LEXER_CASES,
)


def copied_source(item: Expected, raw_call: str, mutation: str) -> str:
    tag = re.sub(r"[^a-z0-9_]", "_", item.id)
    if mutation in {"aliased_reference", "wrapper_reference"}:
        tokens = item.control.split(SEP)
        reference = f"{tokens[0]}::{tokens[2]}"
        if mutation == "aliased_reference":
            return f"use crate::db::repositories::{reference} as copied_{tag};\nfn fixture_{tag}() {{ let _ = copied_{tag}; }}\n"
        return f"fn fixture_{tag}_wrapper() {{ let _ = {reference}; }}\n"
    attribute = ""
    if mutation == "copied_command":
        attribute = "#[tauri::command]\n"
    elif mutation == "copied_parameterized_tauri":
        attribute = '#[tauri::command(rename_all = "snake_case")]\n'
    elif mutation == "copied_long_attribute":
        attribute = "#[tauri::command]\n" + "#[allow(dead_code, unused_variables)]\n" * 6
    return f"{attribute}pub async fn fixture_{tag}() {{ let _ = {raw_call}; }}\n"


def mutate(project: Project, case: Case) -> Project:
    item = next((value for value in EXPECTED if value.id == case.sink), None)
    if case.sink == LAN_ID:
        source = project.files[LAN_PATH].source + '\nfn fixture_lan() { let _ = std::net::UdpSocket::bind("0.0.0.0:0"); }\n'
        if case.mutation == "additional_same_function":
            fn = locate(project, LAN_PATH, "lan_ips")
            assert fn
            source = project.files[LAN_PATH].source[:fn.body_start] + '\nlet _ = std::net::UdpSocket::bind("0.0.0.0:0");\n' + project.files[LAN_PATH].source[fn.body_start:]
        return replace_source(project, LAN_PATH, source)
    if item is None:
        raise ValidationError("FIXTURE_INVENTORY", f"unknown sink {case.sink}")
    fn = locate(project, item.path, item.function)
    if fn is None:
        raise ValidationError("FIXTURE_BUILD", f"missing production function for {item.id}")
    source = project.files[item.path].source
    start, end = canonical_span(source, fn, item.call)
    raw_call = source[start:end]
    if case.mutation == "before_authorization":
        source = source[:start] + "unimplemented!()" + source[end:]
        source = source[:fn.body_start] + f"\nlet _ = {raw_call};\n" + source[fn.body_start:]
    elif case.mutation == "additional_same_function":
        source = source[:fn.body_start] + f"\nlet _ = {raw_call};\n" + source[fn.body_start:]
    elif case.mutation in {"copied_command", "copied_non_tauri_helper", "copied_parameterized_tauri", "copied_long_attribute", "aliased_reference", "wrapper_reference", "copied_helper_call"}:
        fixture_path = "src-tauri/src/commands/__validator_fixture.rs"
        return replace_source(project, fixture_path, copied_source(item, raw_call, case.mutation))
    elif case.mutation in {"false_guard", "alternate_branch", "guard_after_helper", "duplicated_helper"}:
        guard_start, guard_end = canonical_span(source, fn, SETUP_GUARD)
        raw_guard = source[guard_start:guard_end]
        if case.mutation == "false_guard":
            source = source[:guard_start] + f"if false {{ {raw_guard} }}" + source[guard_end:]
        elif case.mutation == "alternate_branch":
            source = source[:start] + f"if false {{ {raw_call}; }} else {{ }}" + source[end:]
        elif case.mutation == "duplicated_helper":
            source = source[:start] + raw_call + ";\n" + source[start:]
        else:
            semicolon = source.find(";", end) + 1
            source = source[:guard_start] + source[start:semicolon] + "\n" + raw_guard + source[guard_end:start] + source[semicolon:]
    else:
        raise ValidationError("FIXTURE_INVENTORY", f"unknown mutation {case.mutation}")
    return replace_source(project, item.path, source)


def required_mutations(sink: str) -> dict[str, str]:
    if sink == LAN_ID:
        return {"copied_command": "LAN_LITERAL_INVENTORY", "additional_same_function": "LAN_LITERAL_INVENTORY"}
    if sink == "setup_watermark_helper":
        return {"copied_command": "SINK_INVENTORY", "additional_same_function": "SINK_INVENTORY"}
    result = dict(BASE_MUTATIONS)
    if sink in AI_IDS:
        result.update(AI_MUTATIONS)
    if sink == "setup_pull_catalog_watermark":
        result.update(SETUP_MUTATIONS)
    return result


def load_fixtures(path: Path) -> list[Case]:
    data = json.loads(path.read_text(encoding="utf-8"))
    if data.get("owner") != "ZANPOS Maintainers" or data.get("review_by") != "2027-01-31":
        raise ValidationError("FIXTURE_INVENTORY", "owner/review_by mismatch")
    expected_ids = {item.id for item in EXPECTED} | {LAN_ID}
    sinks = data.get("sinks", [])
    if len(sinks) != len(expected_ids) or {item.get("id") for item in sinks} != expected_ids:
        raise ValidationError("FIXTURE_INVENTORY", "sink IDs must be exact and unique")
    cases: list[Case] = []
    for sink in sinks:
        if sink.get("valid") != "production" or set(sink) != {"id", "valid", "invalid"}:
            raise ValidationError("FIXTURE_INVENTORY", f"invalid schema for {sink.get('id')}")
        actual = [(item.get("name"), item.get("code")) for item in sink["invalid"]]
        required = sorted(required_mutations(sink["id"]).items())
        if len(actual) != len(set(actual)) or sorted(actual) != required:
            raise ValidationError("FIXTURE_INVENTORY", f"mutation inventory mismatch for {sink['id']}")
        cases.extend(Case(sink["id"], name, code) for name, code in actual)
    lexer = [(item.get("name"), item.get("expect")) for item in data.get("lexer_edges", [])]
    required_lexer = sorted((name, value[0]) for name, value in LEXER_CASES.items())
    if len(lexer) != len(set(lexer)) or sorted(lexer) != required_lexer:
        raise ValidationError("FIXTURE_INVENTORY", "lexer edge inventory mismatch")
    return cases
