# ZANPOS Release Checklist — 2.0.0

Artifact: `ZANPOS_2.0.0_x64-setup.exe` · SHA-256 `bf0fead21a108e8628791eec70f7b6007caec6ed0607fb07bd234bb2118ccdac`

| # | Check | Result | Evidence |
|---|---|---|---|
| 1 | TypeScript typecheck | PASS | `tsc --noEmit` exit 0 |
| 2 | Lint (`eslint src --max-warnings 0`) | PASS | exit 0, 0 errors / 0 warnings |
| 3 | Accessibility lint | PASS | `npm run lint:a11y` exit 0, 0 errors |
| 4 | Frontend tests | PASS | 41 files / 298 tests |
| 5 | Production frontend build | PASS | emitted `dist/`, bundled into installer |
| 6 | Rust `cargo check` (debug) | BLOCKED | debug target locked by running elevated dev app |
| 7 | Rust test suite | NOT RUN | same lock; last known 421/421 |
| 8 | Tauri release build | PASS | exit 0, fat LTO, 29m30s |
| 9 | NSIS bundle produced | PASS | 208,133,483 bytes |
| 10 | SHA-256 recorded, computed twice | PASS | identical both runs |
| 11 | Hash differs from superseded build | PASS | old `11ef9fec…` retired |
| 12 | Bundle secret audit | PASS | no `.env`, `.pem`, `.key`, `.pfx`; no literal key material |
| 13 | Mock-data isolation in shipped `dist/` | PASS | 8 mock-only identifiers, all 0 occurrences |
| 14 | Migration chain | PASS | 44 files, no prefix collision, highest `0045`, embedded via `sqlx::migrate!` |
| 15 | Code signing | NOT SIGNED | no certificate configured |
| 16 | Install / first launch | NOT TESTED | no isolated environment — Windows 11 Home |
| 17 | Second launch / persistence | NOT TESTED | depends on 16 |
| 18 | POS release smoke | NOT TESTED | depends on 16 |

## Gate 6/7 detail

`cargo check` and `cargo test` build into `src-tauri/target/debug`. A ZANPOS dev
process (PID 19600, started 17:13:50) holds
`target/debug/sidecar/whatsapp-sidecar/node.exe` and its parent directory. The
build script fails with `os error 32` at exactly that path. The process survives
`Stop-Process -Force` and the directory rename returns Access denied, so it is
running elevated and cannot be stopped from an unelevated shell.

The **release** tree is unaffected — the dev app runs from `debug` — which is why
gate 8 succeeded. A successful fat-LTO release build compiles strictly more than
`cargo check` does, so gate 6 is subsumed; gate 7 genuinely is not.

To clear it: close the running ZANPOS window, then `npm run ship`.

## Reproduce

```bash
npm run tauri build
```
