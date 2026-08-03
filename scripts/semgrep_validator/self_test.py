from __future__ import annotations

from .core import Project, Case, ValidationError
from .parser import replace_source
from .validation import validate
from .fixtures import mutate
from .contracts import LEXER_CASES


def run_self_test(project: Project, cases: list[Case]) -> None:
    failures: list[str] = []
    for case in cases:
        try:
            codes = {code for code, _ in validate(mutate(project, case))}
        except ValidationError as error:
            failures.append(f"{case.sink}:{case.mutation}: pipeline stopped at {error.code}")
            continue
        expected_codes = set(case.code.split(","))
        if codes != expected_codes:
            failures.append(f"{case.sink}:{case.mutation}: expected {sorted(expected_codes)}, got {sorted(codes)}")
    for name, (expected, source) in LEXER_CASES.items():
        try:
            changed = replace_source(project, f"src-tauri/src/commands/__lexer_{name}.rs", source)
            codes = {code for code, _ in validate(changed)}
            actual = "PASS" if not codes else ",".join(sorted(codes))
        except ValidationError as error:
            actual = error.code
        if actual != expected:
            failures.append(f"lexer:{name}: expected {expected}, got {actual}")
    if failures:
        raise ValidationError("SELF_TEST", "; ".join(failures))
