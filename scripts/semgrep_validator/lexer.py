from __future__ import annotations

import re
from collections.abc import Sequence

from .core import SEP, Token, ValidationError


def lex(source: str) -> list[Token]:
    tokens: list[Token] = []
    i, line, length = 0, 1, len(source)
    operators = ("..=", "...", "::", "->", "=>", "==", "!=", "<=", ">=", "&&", "||", "..")
    while i < length:
        start, start_line, ch = i, line, source[i]
        if ch.isspace():
            line += ch == "\n"
            i += 1
            continue
        if source.startswith("//", i):
            end = source.find("\n", i + 2)
            i = length if end < 0 else end
            continue
        if source.startswith("/*", i):
            depth, i = 1, i + 2
            while i < length and depth:
                if source.startswith("/*", i):
                    depth, i = depth + 1, i + 2
                elif source.startswith("*/", i):
                    depth, i = depth - 1, i + 2
                else:
                    line += source[i] == "\n"
                    i += 1
            if depth:
                raise ValidationError("LEX_UNTERMINATED_COMMENT", f"line {start_line}")
            continue
        raw = re.match(r"(?:br|cr|r)(#{0,255})\"", source[i:])
        if raw:
            close = '"' + raw.group(1)
            end = source.find(close, i + raw.end())
            if end < 0:
                raise ValidationError("LEX_UNTERMINATED_RAW", f"line {line}")
            i = end + len(close)
            value = source[start:i]
            tokens.append(Token(value, start_line, start, i))
            line += value.count("\n")
            continue
        quote = i + 1 if source.startswith(('b"', 'c"'), i) else i
        if source[quote:quote + 1] == '"':
            i = quote + 1
            while i < length and source[i] != '"':
                if source[i] == "\\":
                    i += 2
                else:
                    line += source[i] == "\n"
                    i += 1
            if i >= length:
                raise ValidationError("LEX_UNTERMINATED_STRING", f"line {start_line}")
            i += 1
            tokens.append(Token(source[start:i], start_line, start, i))
            continue
        if ch == "'":
            char_end = i + 2
            if i + 1 < length and source[i + 1] == "\\":
                char_end = (source.find("}", i + 3) + 1) if source.startswith("\\u{", i + 1) else i + 3
            if 0 < char_end < length and source[char_end] == "'":
                i = char_end + 1
            else:
                lifetime = re.match(r"'(?:[A-Za-z_][A-Za-z0-9_]*|_)", source[i:])
                if not lifetime:
                    raise ValidationError("LEX_UNTERMINATED_CHAR", f"line {line}")
                i += len(lifetime.group(0))
            tokens.append(Token(source[start:i], line, start, i))
            continue
        match = re.match(r"[A-Za-z_][A-Za-z0-9_]*", source[i:]) if (ch.isascii() and (ch.isalpha() or ch == "_")) else None
        if match:
            i += len(match.group(0))
            tokens.append(Token(match.group(0), line, start, i))
            continue
        match = re.match(r"[0-9][A-Za-z0-9_\.]*", source[i:]) if ch.isdigit() else None
        if match:
            i += len(match.group(0))
            tokens.append(Token(match.group(0), line, start, i))
            continue
        operator = next((op for op in operators if source.startswith(op, i)), None)
        i += len(operator) if operator else 1
        tokens.append(Token(operator or ch, line, start, i))
    return tokens


def joined(tokens: list[Token] | tuple[Token, ...] | Sequence[Token]) -> str:
    return SEP.join(token.value for token in tokens)


def c(source: str) -> str:
    return joined(lex(source))
