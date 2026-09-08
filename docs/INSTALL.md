# ZANPOS — Installation

## The current artifact

| | |
|---|---|
| File | `ZANPOS_2.0.1_x64-setup.exe` |
| Type | NSIS installer (Windows) |
| Architecture | x64 |
| Version | 2.0.1 |
| Size | 219,723,242 bytes (209.54 MB) |
| Built | 2026-09-08, from commit `e148185` |
| SHA-256 | `6f107a19e035bd8bfeaefdc7599f821f1d4b3e594fa26e703d66710de3351e3e` |
| Authenticode signing | **Unsigned** — see below |
| Updater signature | Present and **verified** (a different key; see below) |
| Build verified | Yes — `npm run ship`, all nine gates, 0 warnings, fat-LTO release profile, 17m38s |
| Install verified | **No** — see "Why install is untested" below |

Built from this repository at:

```
src-tauri/target/release/bundle/nsis/ZANPOS_2.0.1_x64-setup.exe
```

Verify before installing:

```bash
powershell -Command "(Get-FileHash 'src-tauri/target/release/bundle/nsis/ZANPOS_2.0.1_x64-setup.exe' -Algorithm SHA256).Hash"
```

The value must match the SHA-256 above. It was computed twice and matched both
times.

## Superseded artifacts — do not use

**Filename alone does not distinguish these.** Always check the hash — two of the
three below carry the same filename as something else.

| SHA-256 | What it is |
|---|---|
| `21216702d8a1ffee92a0bd5a015d7098974d4b98004801aa7dd1cb5957a1ec41` | **A 2.0.1 build with the identical filename**, from earlier the same day. It predates the VAT fix, so it charges VAT on the pre-discount total of any whole-bill discount and over-refunds discounted lines. Must not be distributed. |
| `bf0fead21a108e8628791eec70f7b6007caec6ed0607fb07bd234bb2118ccdac` | 2.0.0, superseded by this release. |
| `11ef9fecfec274209a20b804a1dc28a2b9e9c0051c0edc5dec8d3142650a6dd2` | An earlier 2.0.0 of the same filename, superseded before it shipped. |

## Two different signatures — do not confuse them

**Authenticode (Windows):** not configured. There is no `certificateThumbprint`
and no `signCommand` in `tauri.conf.json`, so SmartScreen will warn on first run.
Fixing this needs a code-signing certificate the repository does not have.

**Updater (Tauri/minisign):** present and working. The build emits a `.sig` beside
the installer, and it verifies as Ed25519 over a BLAKE2b-512 hash against the
`pubkey` embedded in `tauri.conf.json` — key id `a0c490b084c5d428` on both sides.
This is what stops a till accepting a tampered update; it says nothing to
SmartScreen, and it is not a substitute for Authenticode.

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
