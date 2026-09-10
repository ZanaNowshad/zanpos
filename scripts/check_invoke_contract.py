#!/usr/bin/env python3
"""Fail-closed check that every invoke() call site matches its Tauri command.

Owner: ZANPOS Maintainers. Review by: 2027-01-31.

The RBAC migration moved commands from a payload-derived `actor_user_id` to a
`session_token`. Nothing in the toolchain caught the call sites left behind:
`SessionToken` is assignable to `string`, so a binding that forwards the token
under the old key still type-checks, and the contract test asserted the old
key, so it stayed green. The failure surfaced only at runtime, as Tauri's
"missing required key sessionToken", on a screen nobody had reopened.

This walks the two sides of that contract directly - `#[tauri::command]`
signatures on one side, literal `invoke("name", { ... })` payloads on the
other - and fails when they disagree.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

# Params Tauri injects itself; never supplied by the caller. Matched by type as
# well, so an ordinary param that happens to be called `request` is not skipped.
INJECTED = {"state", "app", "window", "webview", "app_handle"}
INJECTED_TYPES = (
    "State<", "tauri::State<", "AppHandle", "tauri::AppHandle",
    "Window", "tauri::Window", "WebviewWindow", "tauri::WebviewWindow",
)

SKIP_DIRS = ("node_modules", ".npm-cache", "dist", "build", "target", ".git")

# A signature never spans more than this. An unbounded lazy match against the
# rest of the file backtracks catastrophically on the larger command modules.
SIG_WINDOW = 4000


def _skip(path: Path) -> bool:
    return any(part in SKIP_DIRS for part in path.parts)


def camel(name: str) -> str:
    head, *rest = name.split("_")
    return head + "".join(word.capitalize() for word in rest)


def _classify(part: str, required: set, optional: set) -> None:
    part = part.strip()
    if not part or ":" not in part:
        return
    pname, ptype = part.split(":", 1)
    # `mut input: T` binds a param named `input`; the binding mode is not part
    # of the name.
    pname = re.sub(r"^\s*(?:mut\s+|ref\s+)+", "", pname).strip()
    ptype = ptype.strip()
    if pname in INJECTED or pname.startswith("_"):
        return
    if ptype.startswith(INJECTED_TYPES):
        return
    (optional if ptype.startswith("Option<") else required).add(camel(pname))


def _split_params(params: str):
    required, optional = set(), set()
    depth = 0
    buf = ""
    for ch in params:
        if ch in "<([":
            depth += 1
        elif ch in ">)]":
            depth -= 1
        if ch == "," and depth == 0:
            _classify(buf, required, optional)
            buf = ""
            continue
        buf += ch
    _classify(buf, required, optional)
    return required, optional


def rust_commands(rust_root: Path, repo_root: Path) -> dict:
    out = {}
    for path in rust_root.rglob("*.rs"):
        if _skip(path):
            continue
        text = path.read_text(encoding="utf-8", errors="replace")
        for marker in re.finditer(r"#\[tauri::command[^\]]*\]\s*", text):
            tail = text[marker.end(): marker.end() + SIG_WINDOW]
            sig = re.match(r"(?:pub\s+)?(?:async\s+)?fn\s+(\w+)\s*\(", tail)
            if not sig:
                continue
            depth, i = 1, sig.end()
            while i < len(tail) and depth:
                if tail[i] == "(":
                    depth += 1
                elif tail[i] == ")":
                    depth -= 1
                i += 1
            if depth:
                continue
            required, optional = _split_params(tail[sig.end(): i - 1])
            out[sig.group(1)] = {
                "required": required,
                "optional": optional,
                "file": path.relative_to(repo_root).as_posix(),
                "line": text[: marker.start()].count("\n") + 1,
            }
    return out


def registered(lib: Path) -> set:
    text = lib.read_text(encoding="utf-8", errors="replace")
    # Anchor on the real registration call; lib.rs also mentions
    # generate_handler! inside a comment, which a bare find() picks up first.
    anchor = re.search(r"\.invoke_handler\(\s*tauri::generate_handler!", text)
    if not anchor:
        return set()
    open_br = text.find("[", anchor.start())
    if open_br < 0:
        return set()
    depth, i = 1, open_br + 1
    while i < len(text) and depth:
        if text[i] == "[":
            depth += 1
        elif text[i] == "]":
            depth -= 1
        i += 1
    block = text[open_br + 1: i - 1]
    # Strip comments before splitting. A segment like "// POS\ncommands::x" then
    # looks like a comment and silently drops the first entry of every section.
    block = re.sub(r"//[^\n]*", "", block)
    block = re.sub(r"/\*.*?\*/", "", block, flags=re.S)
    return {seg.strip().split("::")[-1] for seg in block.split(",") if seg.strip()}


def _payload_keys(rest: str):
    """Literal keys of an object payload, and whether it was inspectable."""
    opened = re.match(r"\s*,\s*\{", rest)
    if not opened:
        # A non-literal payload (spread, variable, none) cannot be verified.
        return set(), bool(re.match(r"\s*,", rest))
    depth, i = 1, opened.end()
    while i < len(rest) and depth:
        if rest[i] in "{[(":
            depth += 1
        elif rest[i] in "}])":
            depth -= 1
        i += 1
    body = rest[opened.end(): i - 1]
    if "..." in body:
        return set(), True
    keys = set()
    depth = 0
    for token in re.finditer(r"[\w$]+|[{}\[\]()]", body):
        tok = token.group(0)
        if tok in "{[(":
            depth += 1
            continue
        if tok in "}])":
            depth -= 1
            continue
        if depth:
            continue
        after = body[token.end():].lstrip()
        before = body[: token.start()].rstrip()
        if (after.startswith((":", ",")) or after == "") and (
            before == "" or before.endswith((",", "{"))
        ):
            keys.add(tok)
    return keys, False


def invoke_sites(front_root: Path, repo_root: Path) -> list:
    sites = []
    for path in list(front_root.rglob("*.ts")) + list(front_root.rglob("*.tsx")):
        if _skip(path):
            continue
        text = path.read_text(encoding="utf-8", errors="replace")
        for m in re.finditer(r"""invoke(?:<[^>]*>)?\(\s*["'`](\w+)["'`]""", text):
            keys, opaque = _payload_keys(text[m.end():])
            sites.append({
                "cmd": m.group(1),
                "keys": keys,
                "opaque": opaque,
                "file": path.relative_to(repo_root).as_posix(),
                "line": text[: m.start()].count("\n") + 1,
            })
    return sites


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path("."))
    args = parser.parse_args()
    root = args.root.resolve()

    cmds = rust_commands(root / "src-tauri" / "src", root)
    reg = registered(root / "src-tauri" / "src" / "lib.rs")
    sites = invoke_sites(root / "src", root)

    missing_key, unknown_key, no_such, unregistered = [], [], [], []
    for site in sites:
        cmd = cmds.get(site["cmd"])
        if not cmd:
            no_such.append(site)
            continue
        if site["cmd"] not in reg:
            unregistered.append(site)
        if site["opaque"]:
            continue
        absent = cmd["required"] - site["keys"]
        if absent:
            missing_key.append((site, sorted(absent), cmd))
        extra = site["keys"] - cmd["required"] - cmd["optional"]
        if extra:
            unknown_key.append((site, sorted(extra)))

    print("commands defined:    %d" % len(cmds))
    print("commands registered: %d" % len(reg))
    print("invoke sites:        %d" % len(sites))
    print()

    if missing_key:
        print("MISSING REQUIRED KEY (%d) - fails at runtime" % len(missing_key))
        for site, absent, cmd in sorted(missing_key, key=lambda x: (x[0]["file"], x[0]["line"])):
            print("  %s:%d  %s" % (site["file"], site["line"], site["cmd"]))
            print("      absent:  %s" % ", ".join(absent))
            print("      passes:  %s" % (", ".join(sorted(site["keys"])) or "(none)"))
            print("      defined: %s:%d" % (cmd["file"], cmd["line"]))
        print()

    if unknown_key:
        print("KEY NOT IN SIGNATURE (%d) - silently ignored" % len(unknown_key))
        for site, extra in sorted(unknown_key, key=lambda x: (x[0]["file"], x[0]["line"])):
            print("  %s:%d  %s  ->  %s" % (site["file"], site["line"], site["cmd"], ", ".join(extra)))
        print()

    if no_such:
        print("NO SUCH COMMAND (%d)" % len(no_such))
        for site in no_such:
            print("  %s:%d  %s" % (site["file"], site["line"], site["cmd"]))
        print()

    if unregistered:
        print("NOT IN generate_handler! (%d)" % len(unregistered))
        for site in unregistered:
            print("  %s:%d  %s" % (site["file"], site["line"], site["cmd"]))
        print()

    failures = len(missing_key) + len(unknown_key) + len(no_such) + len(unregistered)
    if failures:
        print("FAIL: %d invoke site(s) disagree with their command signature." % failures)
        return 1
    print("OK: every invoke site matches its command signature.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
