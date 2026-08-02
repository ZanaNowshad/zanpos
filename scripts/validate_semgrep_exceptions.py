#!/usr/bin/env python3
"""Fail-closed validation for finite Semgrep CE security exceptions.

Owner: ZANPOS Maintainers. Review by: 2027-01-31.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

# Ensure the project root is on sys.path so that `scripts.semgrep_validator` is importable.
_project_root = Path(__file__).resolve().parent.parent
if str(_project_root) not in sys.path:
    sys.path.insert(0, str(_project_root))

from scripts.semgrep_validator.core import ValidationError
from scripts.semgrep_validator.parser import load_project
from scripts.semgrep_validator.validation import validate
from scripts.semgrep_validator.fixtures import load_fixtures, mutate
from scripts.semgrep_validator.self_test import run_self_test
from scripts.semgrep_validator.contracts import LEXER_CASES


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--fixtures", type=Path)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--fixture-case", help="valid or <sink-id>:<mutation>")
    args = parser.parse_args()
    try:
        project = load_project(args.root.resolve())
        cases = load_fixtures(args.fixtures) if args.fixtures else []
        if args.fixture_case and args.fixture_case != "valid":
            sink, separator, mutation = args.fixture_case.partition(":")
            case = next((item for item in cases if item.sink == sink and item.mutation == mutation), None)
            if not separator or case is None:
                raise ValidationError("FIXTURE_INVENTORY", f"undeclared fixture case: {args.fixture_case}")
            project = mutate(project, case)
        issues = validate(project)
        if issues:
            for code, message in issues:
                print(f"ERROR: {code}: {message}", file=sys.stderr)
            return 1
        if args.self_test:
            if not args.fixtures:
                raise ValidationError("FIXTURE_INVENTORY", "--self-test requires --fixtures")
            run_self_test(project, cases)
            print(f"PASS: production policy, {len(cases)} security mutations, and {len(LEXER_CASES)} lexer edges validated")
        else:
            print("PASS: finite Semgrep security exceptions validated")
        return 0
    except (OSError, ValueError, json.JSONDecodeError, ValidationError) as error:
        code = error.code if isinstance(error, ValidationError) else "VALIDATOR_FAILURE"
        print(f"ERROR: validator failed closed: {code}: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
