from __future__ import annotations

import hashlib
import json
from collections import Counter

from .core import Project
from .lexer import joined
from .parser import locate
from .authorization import validate_authorization
from .contracts import (
    EXPECTED, EXTRA_SINKS, FUNCTION_HASHES, CONTROL_METHODS,
    CONTROL_INVENTORY_SHA256, LAN_ID, LAN_PATH, LAN_LITERAL, LAN_BRANCH,
)


def _ships(parsed) -> bool:
    """True when this file is compiled into the shipped application.

    The inventory audits production control flow. A `#![cfg(test)]` module is
    not in the release build — the ship gate has its own check asserting that —
    so counting its calls conflates test code with shipped code. It also makes
    the control decay: every new test that exercises an audited repository
    function trips the gate, and a gate that fires on correct work teaches
    people to re-bless it without reading, which is the one failure this whole
    file exists to prevent.
    """
    return "#![cfg(test)]" not in parsed.source


def control_inventory(project: Project) -> tuple[Counter, str]:
    sink_counts: Counter = Counter()
    identifier_counts: Counter = Counter()
    controls = {item.control for item in EXPECTED}
    shipped = {path for path, parsed in project.files.items() if _ships(parsed)}
    for fn in project.functions:
        if fn.path not in shipped:
            continue
        for control in controls:
            count = fn.body.count(control)
            if count:
                sink_counts[(fn.path, fn.name, control)] += count
    for path, parsed in project.files.items():
        if path not in shipped:
            continue
        for token in parsed.tokens:
            if token.value not in CONTROL_METHODS:
                continue
            owners = [fn for fn in parsed.functions if fn.start <= token.start < fn.end]
            owner = min(owners, key=lambda fn: fn.end - fn.start).name if owners else "<module>"
            identifier_counts[(path, owner, token.value)] += 1
    payload = json.dumps(sorted((*key, count) for key, count in identifier_counts.items()), separators=(",", ":"))
    return sink_counts, hashlib.sha256(payload.encode()).hexdigest()


def validate(project: Project) -> list[tuple[str, str]]:
    issues: list[tuple[str, str]] = []
    for key, digest in FUNCTION_HASHES.items():
        fn = locate(project, *key)
        if fn is None:
            issues.append(("FUNCTION_INVENTORY", f"expected exactly one {key[0]}::{key[1]}"))
        elif hashlib.sha256(fn.body.encode()).hexdigest() != digest:
            issues.append(("CONTROL_FLOW_CONTRACT", f"reviewed normalized body changed: {key[0]}::{key[1]}"))
    expected_sinks: Counter = Counter((item.path, item.function, item.control) for item in EXPECTED)
    expected_sinks.update(EXTRA_SINKS)
    actual_sinks, identifier_digest = control_inventory(project)
    if actual_sinks != expected_sinks:
        issues.append(("SINK_INVENTORY", "controlled sink path/function/count inventory changed"))
    if identifier_digest != CONTROL_INVENTORY_SHA256:
        issues.append(("CONTROL_REFERENCE_INVENTORY", f"controlled identifier inventory changed ({identifier_digest})"))
    for item in EXPECTED:
        fn = locate(project, item.path, item.function)
        if fn is not None and fn.body.count(item.call) != 1:
            issues.append(("SINK_SHAPE", f"{item.id}: expected one exact reviewed call"))
    lan = locate(project, LAN_PATH, "lan_ips")
    if lan is None:
        issues.append(("LAN_FUNCTION_INVENTORY", f"expected exactly one {LAN_PATH}::lan_ips"))
    elif lan.body.count(LAN_BRANCH) != 1:
        issues.append(("LAN_BRANCH_SHAPE", "reviewed UDP bind/connect/local-address branch changed"))
    literal_count = sum(joined(parsed.tokens).count(LAN_LITERAL) for parsed in project.files.values())
    if literal_count != 1:
        issues.append(("LAN_LITERAL_INVENTORY", f"expected one wildcard UDP literal, found {literal_count}"))
    issues.extend(validate_authorization(project))
    return issues
