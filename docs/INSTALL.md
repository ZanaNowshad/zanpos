# ZANPOS — Installation

## The current artifact

| | |
|---|---|
| File | `ZANPOS_2.0.0_x64-setup.exe` |
| Type | NSIS installer (Windows) |
| Architecture | x64 |
| Version | 2.0.0 |
| Size | 208,133,483 bytes (198.49 MB) |
| Built | 2026-08-13 02:16:25 |
| SHA-256 | `bf0fead21a108e8628791eec70f7b6007caec6ed0607fb07bd234bb2118ccdac` |
| Signing | **Unsigned** |
| Build verified | Yes — `npm run tauri build` exit 0, fat-LTO release profile, 29m30s |
| Install verified | **No** — see "Why install is untested" below |

Built from this repository at:

```
src-tauri/target/release/bundle/nsis/ZANPOS_2.0.0_x64-setup.exe
```

Verify before installing:

```bash
powershell -Command "(Get-FileHash 'src-tauri/target/release/bundle/nsis/ZANPOS_2.0.0_x64-setup.exe' -Algorithm SHA256).Hash"
```

The value must match the SHA-256 above. It was computed twice and matched both times.

## Superseded artifact — do not use

An earlier installer of the same filename had SHA-256:

```
11ef9fecfec274209a20b804a1dc28a2b9e9c0051c0edc5dec8d3142650a6dd2
```

That binary predates the current source and **must not be installed or
distributed**. It is superseded. Filename alone does not distinguish the two —
always check the hash.

## Why install is untested

The installer was built and audited but **never executed**, because this machine
offers no genuinely isolated environment:

- **Windows 11 Home** — Windows Sandbox requires Pro/Enterprise/Education.
- No Hyper-V on this edition; WSL is present but Linux-only and cannot run an
  NSIS installer.
- The app resolves its database through Tauri's `app_data_dir()`. There is **no
  environment variable or CLI flag in the source** that redirects it, and
  inventing one would not reflect what ships.
- Live application data already exists at `%APPDATA%\com.super.zanpos` and
  `%LOCALAPPDATA%\com.super.zanpos`.

Installing here would therefore run the new build against real data. That was
not done.

To validate the install path, run the installer on a clean Windows machine or a
VM, then confirm: first launch without a dev server, database initialisation,
migration execution, shell and navigation render, language switch, and that a
second launch persists data without re-running migrations destructively.

## Unsigned binary

Windows SmartScreen will warn on first run. Code signing requires a certificate
that is not configured in this repository.
