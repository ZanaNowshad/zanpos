from __future__ import annotations

from dataclasses import dataclass

SEP = "\x1f"


class ValidationError(Exception):
    def __init__(self, code: str, message: str):
        self.code = code
        super().__init__(message)


@dataclass(frozen=True)
class Token:
    value: str
    line: int
    start: int
    end: int


@dataclass(frozen=True)
class Function:
    path: str
    name: str
    line: int
    start: int
    body_start: int
    body_end: int
    end: int
    body: str


@dataclass(frozen=True)
class ParsedFile:
    source: str
    tokens: tuple[Token, ...]
    functions: tuple[Function, ...]


@dataclass(frozen=True)
class Expected:
    id: str
    path: str
    function: str
    control: str
    call: str


@dataclass(frozen=True)
class Case:
    sink: str
    mutation: str
    code: str


@dataclass(frozen=True)
class Project:
    files: dict[str, ParsedFile]
    functions: tuple[Function, ...]
