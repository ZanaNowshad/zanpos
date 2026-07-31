# AGENTS.md — ZANPOS

Tauri v2 desktop POS for Windows (React 19 + Rust + SQLite). Local-first, offline-capable, syncs to Supabase when online.

## Key commands

```bash
npm run tauri dev     # Full dev (Vite + Rust backend). NOT `npm run dev` — that is Vite-only.
npm run check         # Pre-commit: type-check → lint → test (sequential)
npm run type-check    # tsc --noEmit
npm run lint          # ESLint --ext .ts,.tsx --max-warnings 0
npm test              # Vitest run
npm run coverage      # Vitest + v8 coverage (thresholds 80/80/80, informational)

# Rust (run from src-tauri/)
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --lib      # 113 unit + integration tests
cargo audit --file Cargo.lock
```

## Architecture

```
src/                  → React 19 frontend (TypeScript, Vite 7)
  tauri/commands.ts   → All Tauri invoke() wrappers (single file, ~1000+ lines)
  types.ts            → TS types mirroring Rust domain structs
  money.ts            → BHD formatting/parsing (exponent=3, integer minor units)
  components/         → ~50 React components (POS UI, back-office tabs, modals)

src-tauri/
  src/main.rs         → Entry: windows_subsystem = "windows", calls zanpos_lib::run()
  src/lib.rs          → Tauri builder: plugins, AppState (db, sync, whatsapp), logging
  src/commands/       → 25 command modules (one .rs per domain: pos, auth, ai_admin, etc.)
  src/domain/         → Pure domain logic (money, cart, sale, refund, report, etc.)
  src/db/             → sqlx queries, repository pattern
  src/sync_v2/        → Supabase outbox/inbox sync worker (30s cycle, watermark-based)
  src/ai/             → AI agent loop, tool definitions, streaming chat
  src/inventory/      → Stock movements, low-stock alerts, stock-take
  migrations/         → SQLite migrations (numbered .sql files)
  sidecar/            → WhatsApp Node.js sidecar (builds separately to binaries/)
```

## Money rules (critical)

- All money is **integer minor units**. 1 BHD = 1000 fils. `price_minor: 1500` means 1.500 BHD.
- Rust: `i64`. TypeScript: `number`. Never use floating-point for money.
- VAT is stored as basis points (e.g. 1000 = 10%). Tax is calculated in integers; round half-up after division.
- Use `formatMoney()` and `parseMoney()` from `src/money.ts` — never direct floating-point formatting.

## SQLite

- WAL mode, FK constraints enforced. Migrations run automatically via `sqlx::migrate!()` at startup.
- Database path: `{app_data_dir}/zanpos.db`. Check WAL before file-copy backups.
- IDs are ULIDs. Version columns for LWW sync conflict resolution.

## Security constraints

- API keys stored in Windows Credential Manager (keyring v2), **not** in SQLite or .env.
- CSP is strict (`default-src 'self'`). Fetching external URLs from the frontend will fail — use Rust commands.
- WhatsApp sidecar at `localhost:3131` requires `X-Sidecar-Token` auth (token in `.sidecar_token` file).
- PINs: Argon2id with random salt. Legacy `PLAIN:` PINs auto-rehashed on launch.

## Build notes

- The `beforeBuildCommand` in `tauri.conf.json` builds the Node.js WhatsApp sidecar (`cd src-tauri/sidecar/whatsapp-sidecar && npm run build`). If the sidecar binary is missing from `src-tauri/binaries/`, WhatsApp features silently disable.
- Release builds use LTO=fat, codegen-units=1, opt-level=s, strip=symbols.
- Vite dev server must run on port 1420 (strictPort). Tauri connects to `http://localhost:1420`.

## Testing

- Frontend tests are unit tests on utilities (`money.ts`, `adminChatClear.ts`, `posProductFilters.ts`). Environment: node (no jsdom yet).
- Rust tests use `cargo test --lib` (domain + repository integration tests). Some integration tests need a live SQLite DB — sqlx handles this with in-memory or temp DBs.
- CI runs frontend on ubuntu-latest, Rust + build on windows-latest. The Tauri build job requires windows-latest.

## Conventions

- TypeScript: strict mode, noUnusedLocals, noUnusedParameters. ESLint: 0 max-warnings, React 19 (no import needed for JSX).
- Rust: edition 2021, MSRV 1.77, `anyhow` for application errors, `thiserror` for library errors.
- Frontend communicates with backend exclusively via `invoke()` from `@tauri-apps/api/core`. All invoke wrappers are in `src/tauri/commands.ts`.
- Lazy-loaded chunks defined in `vite.config.ts` (backoffice, setup-wizard, migration, icons, jsbarcode, tauri-api, vendor).
- Window: frameless (`decorations: false`), custom titlebar via `WindowControls.tsx`.

## Reference docs

- `docs/backup-restore-ops.md` — backup/restore procedures
- `docs/compliance-checklist.md` — Bahrain NBR / PCI DSS compliance
- `docs/sync-conflict-resolution.md` — sync architecture and conflict resolution
- `README.md` — full feature list and developer quick start
