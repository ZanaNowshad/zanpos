from __future__ import annotations

from pathlib import Path

from .core import SEP, Function, ParsedFile, Project, Token, ValidationError
from .lexer import joined, lex


def validate_delimiters(tokens: list[Token]) -> None:
    stack: list[Token] = []
    pairs = {")": "(", "]": "[", "}": "{"}
    for token in tokens:
        if token.value in "([{":
            stack.append(token)
        elif token.value in pairs:
            if not stack or stack.pop().value != pairs[token.value]:
                raise ValidationError("PARSE_UNBALANCED_DELIMITER", f"line {token.line}")
    if stack:
        raise ValidationError("PARSE_UNBALANCED_DELIMITER", f"line {stack[-1].line}")


def parse_file(path: str, source: str) -> ParsedFile:
    tokens = lex(source)
    validate_delimiters(tokens)
    functions: list[Function] = []
    for index, token in enumerate(tokens):
        if token.value != "fn" or index + 1 >= len(tokens):
            continue
        opening = next((i for i in range(index + 2, len(tokens)) if tokens[i].value in ("{", ";")), None)
        if opening is None or tokens[opening].value == ";":
            continue
        depth, closing = 0, None
        for cursor in range(opening, len(tokens)):
            depth += tokens[cursor].value == "{"
            depth -= tokens[cursor].value == "}"
            if depth == 0:
                closing = cursor
                break
        if closing is None:
            raise ValidationError("PARSE_UNCLOSED_FUNCTION", f"{path}:{token.line}")
        functions.append(Function(path, tokens[index + 1].value, token.line, token.start, tokens[opening].end,
                                  tokens[closing].start, tokens[closing].end, joined(tokens[opening + 1:closing])))
    return ParsedFile(source, tuple(tokens), tuple(functions))


def assemble(files: dict[str, ParsedFile]) -> Project:
    return Project(files, tuple(fn for path in sorted(files) for fn in files[path].functions))


def load_project(root: Path) -> Project:
    rust_root = root / "src-tauri" / "src"
    if not rust_root.is_dir():
        raise ValidationError("SOURCE_MISSING", f"missing Rust source root: {rust_root}")
    files = {file.relative_to(root).as_posix(): parse_file(file.relative_to(root).as_posix(), file.read_text(encoding="utf-8"))
             for file in sorted(rust_root.rglob("*.rs"))}
    return assemble(files)


def replace_source(project: Project, path: str, source: str) -> Project:
    files = dict(project.files)
    files[path] = parse_file(path, source)
    return assemble(files)


def locate(project: Project, path: str, name: str) -> Function | None:
    found = [fn for fn in project.functions if fn.path == path and fn.name == name]
    return found[0] if len(found) == 1 else None


def canonical_span(source: str, fn: Function, needle: str) -> tuple[int, int]:
    tokens = lex(source[fn.body_start:fn.body_end])
    wanted = needle.split(SEP)
    matches = [i for i in range(len(tokens) - len(wanted) + 1) if [t.value for t in tokens[i:i + len(wanted)]] == wanted]
    if len(matches) != 1:
        raise ValidationError("FIXTURE_BUILD", f"expected one mutation span, found {len(matches)}")
    index = matches[0]
    return fn.body_start + tokens[index].start, fn.body_start + tokens[index + len(wanted) - 1].end
