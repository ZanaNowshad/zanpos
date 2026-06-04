# ZANPOS

**Local-first offline-capable retail POS for Windows — built with Tauri v2 + React 19 + SQLite**

ZANPOS is a production-grade point-of-sale system designed for single and multi-branch retail businesses in Bahrain. It operates fully offline, syncing to Supabase when connectivity is available.

---

## Features

- 🧾 **Barcode-first POS** — scan products, manage quantities, apply discounts, split payments
- 🏪 **Multi-branch / multi-device** — last-write-wins catalog sync via Supabase outbox
- 🔐 **RBAC** — cashier / manager / owner roles with Argon2id PIN authentication and account lockout
- 💰 **BHD currency** — 3 decimal places, all amounts as integer minor units (no floating-point money)
- 🧾 **VAT compliance** — NBR/Bahrain TRN on receipts, subtotal + VAT + total breakdown
- 📊 **Reports** — daily tax report with cumulative totals, EOD cashup, X/Z-reports, top products
- 🖨 **ESC/POS thermal printing** — serial port, 48-char receipt with configurable header/footer
- 🔄 **Sync** — 30-second outbox/inbox cycle, watermark-based pull, retry with backoff
- 🔒 **Secure credential storage** — API keys in Windows Credential Manager (DPAPI)
- 📦 **Inventory** — stock movements, low-stock alerts, stock-take with manager approval
- 🤖 **AI Back Office** — Anthropic/OpenAI assistant with 50+ tools, streaming responses, persistent history, and free web search
- 📦 **Deliveries** — order creation, status tracking (pending → out_for_delivery → delivered), rider assignment
- 👤 **Customers** — directory with loyalty points, full CRUD, linked to sales and deliveries
- 💬 **WhatsApp** — delivery confirmation messages via embedded Node.js sidecar (WA Web protocol)
- 🛡 **Audit log** — SHA-256 hash-chained immutable event log with chain-verify command

---

## Architecture

```
Frontend (React 19 + TypeScript + Vite)
    │  invoke() via Tauri IPC
    ▼
Rust backend (Tauri v2 + sqlx + tokio)
    │
    ├── SQLite (WAL mode, FK constraints, argon2id PINs)
    ├── Supabase sync worker (30 s outbox/inbox cycle)
    ├── Thermal printer (serialport ESC/POS)
    ├── Windows Credential Manager (keyring v2)
    └── WhatsApp sidecar (Node.js child process, WA Web)
```

**Key design decisions:**
- All money stored as `i64` minor units (1 BHD = 1000 fils) — no floating-point arithmetic on money
- All IDs are ULIDs for offline-safe uniqueness
- Audit log is SHA-256 hash-chained per device for tamper evidence
- Sync is device-authoritative for transactional data (sales, shifts, refunds) — LWW for catalog and stock

---

## Developer Quick Start

### Prerequisites

- [Rust stable](https://rustup.rs/) ≥ 1.77
- [Node.js](https://nodejs.org/) ≥ 20
- [Tauri CLI prerequisites for Windows](https://v2.tauri.app/start/prerequisites/)

### Setup

```bash
# Install JS dependencies
npm install

# Run in development mode (hot-reload frontend + Rust backend)
npm run tauri dev
```

### Available Scripts

| Command | Description |
|---------|-------------|
| `npm run dev` | Vite dev server only (no Tauri) |
| `npm run tauri dev` | Full Tauri dev mode |
| `npm run check` | Type-check + lint + tests in one pass |
| `npm run type-check` | TypeScript type-check |
| `npm run lint` | ESLint (`--max-warnings 0`) |
| `npm test` | Vitest unit tests |
| `npm run coverage` | Vitest with V8 coverage report |
| `npm run build` | Production frontend build |

```bash
# Rust checks (run from src-tauri/)
cargo fmt --all -- --check     # format check
cargo clippy --all-targets -- -D warnings
cargo test --lib               # 50 unit + integration tests
cargo audit --file Cargo.lock  # dependency CVE scan
```

---

## Admin AI

The AI assistant runs inside the app and has access to the live SQLite database. It uses a multi-turn agentic tool loop (up to 8 turns per message).

### Providers
| Provider | Streaming | Setup |
|----------|-----------|-------|
| Anthropic Claude (claude-sonnet-4-6) | ✅ token-by-token | Paste `sk-ant-…` key in Settings |
| OpenAI-compatible (any `/v1` endpoint) | fallback | Base URL + key + model |

### Tool Categories (50+ tools)
| Category | Tools |
|----------|-------|
| Sales & Reports | `get_today_summary`, `get_daily_report`, `get_date_range_report`, `get_top_products`, `get_hourly_sales`, `get_cashier_performance`, `get_tax_report` |
| Inventory | `get_stock_levels`, `get_low_stock`, `adjust_stock`, `stock_take`, `bulk_stock_take`, `get_stock_movements`, `update_reorder_point` |
| Products | `list_products`, `search_products`, `get_product`, `list_categories`, `update_product_price`, `update_product_name`, `set_product_active`, `create_product` |
| Cash & Audit | `get_cash_summary`, `get_recent_refunds`, `get_audit_log`, `list_safe_drops`, `list_no_sale_events`, `get_audit_chain_status`, `get_sync_status` |
| Customers | `list_customers`, `get_customer`, `create_customer`, `update_customer` |
| Deliveries | `list_deliveries`, `advance_delivery_status` |
| Staff | `list_users`, `get_shift_history` |
| Web Search | `web_search` (DuckDuckGo lite, free, no key), `search_market_prices` |

### Safety Model
- **Read tools** execute immediately and return results to the AI for further reasoning
- **Mutation tools** require explicit admin confirmation before any DB write
- Every confirmed mutation writes an audit log entry and creates an undo record
- Undo is available for most mutations (manager/owner role required)

### Persistence
Chat history is saved to `ai_chat_messages` (SQLite) and reloaded on next launch, grouped by date separators. "Clear chat" wipes both the UI and the DB history for that user/branch.

---

## WhatsApp

The WhatsApp integration uses a **Node.js sidecar** process that implements the WA Web protocol. The sidecar starts automatically when the app launches (if the binary is present at `src-tauri/binaries/`).

```
src-tauri/binaries/
└── whatsapp-sidecar-x86_64-pc-windows-msvc.exe
```

**Flow:**
1. Admin scans QR code in Settings → WhatsApp (links a phone number)
2. When a delivery is marked as "out for delivery", the POS sends a confirmation message to the customer's number
3. Message template is configurable per branch

The sidecar runs on `localhost:3131` and communicates with the Rust backend over HTTP. If the binary is missing, WhatsApp features are silently disabled.

---

## Deliveries

Delivery orders are created at checkout by toggling the "Delivery" mode in the POS. Each delivery has:
- Customer name, contact number, and address
- Expected payment method and amount
- Rider assignment
- Status lifecycle: `pending` → `out_for_delivery` → `delivered` (or `cancelled`)
- WhatsApp notification on status change (optional)

The AI assistant can list and advance delivery statuses with the `list_deliveries` and `advance_delivery_status` tools.

---

## Testing

### Frontend (Vitest)
```bash
npm test
# 18 tests: BHD formatMoney, parseMoney round-trips
```

### Rust (cargo test)
```bash
cd src-tauri && cargo test --lib
# 50 tests across:
#   domain::cart       — 10 tests (tax, discounts, payments)
#   domain::money      —  3 tests (format, tax, discount)
#   inventory::movements — 8 tests (adjust, take, alerts)
#   db::repositories::sale_repo  — 7 integration tests
#   db::repositories::refund_repo — 6 integration tests
#   commands::phase10a_commands — 5 backup/restore tests
#   sync::supabase_client — 1 test (URL parsing)
```

---

## CI Pipeline

4 jobs defined in `.github/workflows/ci.yml`:

| Job | Runs on | What it checks |
|-----|---------|----------------|
| `rust` | windows-latest | fmt, clippy, cargo test, cargo audit |
| `frontend` | ubuntu-latest | tsc, ESLint, vitest, npm audit |
| `build` | windows-latest | Full Tauri debug build |
| `release-gate` | ubuntu-latest | Manual gate checklist (main/master only) |

---

## Security

- **PINs**: Argon2id with random salt; legacy `PLAIN:` PINs re-hashed on first launch
- **API keys**: Windows Credential Manager (never in SQLite)
- **AI action hashes**: each confirmed mutation input is SHA-256 hashed for tamper detection
- **Payment data**: No PAN/CVV/track data stored (PCI DSS SAQ P2PE eligible)
- **Audit trail**: SHA-256 chained, verifiable via `audit_verify_chain` command
- **RBAC**: Role checked server-side on every sensitive command (DB is source of truth)
- **AI mutations**: all writes require explicit user confirmation; undo requires manager/owner role

---

## Compliance (Bahrain NBR / PCI DSS)

See [`docs/compliance-checklist.md`](docs/compliance-checklist.md) for the full checklist.

Key requirements verified in code:
- TRN (Tax Registration Number) printed on every receipt
- VAT breakdown: Subtotal (excl. VAT) + VAT + Total on receipt
- Sequential receipt and refund receipt numbers per branch/device
- UTC timestamps + local `business_date` for cross-midnight sales
- BHD 3-decimal formatting throughout

---

## Backup & Restore

See [`docs/backup-restore-ops.md`](docs/backup-restore-ops.md) for the full operations runbook including:
- Built-in `db_backup` command (owner-only)
- WAL checkpoint requirement before file-copy backup
- Step-by-step restore procedure
- Restore drill sign-off template (required before production)

---

## Sync Architecture

See [`docs/sync-conflict-resolution.md`](docs/sync-conflict-resolution.md) for:
- Entity class conflict strategies (LWW, append-only, device-authoritative)
- Outbox idempotency key design
- Retry behaviour and permanently-failed event handling
- Known gaps and mitigations

---

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/)
- [Tauri extension](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode)
- [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
- [ESLint extension](https://marketplace.visualstudio.com/items?itemName=dbaeumer.vscode-eslint)
