# ZANPOS — Complete Build Specification

> Forward-looking build specification incorporating cr-sqlite CRDT sync, MCP AI tool protocol, Tailwind CSS v4, Zustand state management, SQLite FTS5 search, tracing observability, biometric 2FA, Baileys-only WhatsApp, and Receipt OCR → purchase order flow.
> Date: 2026-07-03

---

## 1. PROJECT OVERVIEW & ARCHITECTURE

Build a local-first, offline-capable retail point-of-sale desktop application for Windows, using Tauri v2 (Rust backend, React 19 frontend). Target: Bahraini and Gulf retail businesses with BHD currency support (3 decimal places), thermal receipt printing (ESC/POS), WhatsApp notifications via Baileys sidecar, AI-powered business insights via MCP-standard tool protocol with multi-provider LLM integration, CRDT-based multi-terminal sync via cr-sqlite with embedded hub for WAN relay, and a full migration agent for importing data from legacy POS systems.

**Target platform:** Windows 10/11 (x64). macOS/Linux as secondary build targets.

**Core principles:**
1. Local-first — all operations work offline, sync when connected
2. Integer money — all monetary values stored as integer minor units (fils), zero floating-point
3. Server-side tax recalculation — client tax values are advisory; server always recomputes
4. Audit hash chain — every mutation produces a SHA-256 chained audit entry
5. Sequential receipt numbering — per-device atomic increment, never reused
6. Role-based access control — four roles (owner, manager, cashier, accountant) with database-backed permissions
7. CRDT-based sync — cr-sqlite provides multi-master conflict-free replication; embedded hub relays over WAN
8. MCP AI protocol — all AI tools exposed as MCP tools/resources, replacing custom dispatch chain
9. Optional biometric 2FA — Windows Hello / Touch ID as second factor for sensitive operations

**Architecture (logical layers):**
```
React 19 Frontend (TypeScript, Vite, Tailwind CSS v4, Zustand)
  ↕ Tauri IPC (invoke + Channel for streaming)
Tauri v2 Rust Backend
  ├── commands/       (24 modules, ~190 commands)
  ├── ai/             (multi-provider LLM, MCP tool server, engine, streaming)
  ├── db/             (SQLite + cr-sqlite CRDT extension, WAL mode, FTS5, 9 migrations)
  ├── domain/         (Cart, Money, Sale, Refund, Shift, Product, Auth, Delivery, Report)
  ├── sync_v2/        (CRDT sync via cr-sqlite, hub relay for WAN)
  ├── hub/            (embedded axum 0.8 server, port 8923)
  ├── secure_store.rs (keyring crate)
  ├── biometry.rs     (tauri-plugin-biometry — Windows Hello / Touch ID)
  ├── tracing_init.rs (tracing-subscriber — structured JSON logging)
  └── inventory/      (stock movements, stock repo)
  ↕
SQLite (WAL mode, cr-sqlite CRDT extension, FTS5, max_connections=6, busy_timeout=15s)
  ↕ HTTP (hub WAN relay)
WhatsApp Sidecar (Node.js, Baileys v7.0.0-rc13, Express :3131, Tesseract.js OCR)
```

---

## 2. BUILD SYSTEM & CONFIGURATION

### 2.1 Rust / Cargo (`src-tauri/Cargo.toml`)

**34 dependencies with exact versions:**

| Crate | Version | Features | Purpose |
|---|---|---|---|
| tauri | 2 | — | Application framework |
| tauri-plugin-opener | 2 | — | OS opener |
| tauri-plugin-dialog | 2 | — | Native dialogs |
| tauri-plugin-updater | 2 | — | Auto-updater |
| tauri-plugin-clipboard-manager | 2 | — | Clipboard access |
| tauri-plugin-fs | 2 | — | Filesystem access |
| tauri-plugin-process | 2 | — | Process lifecycle |
| tauri-plugin-shell | 2 | — | Shell command execution |
| tauri-plugin-biometry | 2 | — | Windows Hello / Touch ID 2FA |
| serde / serde_json | 1 | derive | Serialization |
| sqlx | 0.8 | sqlite, runtime-tokio, macros, migrate | Async SQLite |
| cr-sqlite | 0.11 | — | CRDT extension for multi-master sync |
| tokio | 1 | full | Async runtime |
| reqwest | 0.12 | rustls-tls, json, stream | HTTP client |
| axum | 0.8 | — | Hub HTTP server |
| argon2 | 0.5 | alloc | PIN hashing |
| sha2 | 0.10 | — | SHA-256 audit hashes |
| uuid | 1 | v4, fast-rng | ULID generation |
| chrono | 0.4 | serde | Date/time handling |
| keyring | 3 | — | OS secure credential storage |
| tracing | 0.1 | — | Structured span-based instrumentation |
| tracing-subscriber | 0.3 | json, env-filter | JSON log output, per-module filtering |
| tracing-appender | 0.2 | — | Rotating file log writer |
| printpdf | 0.8 | — | PDF receipt generation |
| csv | 1.3 | — | CSV import/export |
| calamine | 0.26 | — | Excel file reading |
| zip | 2 | — | ZIP extraction |
| windows | 0.61 | Win32_Printing, Win32_System_JobObjects | ESC/POS printing, sidecar lifecycle |
| serialport | 4 | — | Serial thermal printer |
| rfd | 0.15 | — | Native file dialogs (Commodo) |
| tower-http | 0.6 | cors | Hub CORS middleware |
| ring | 0.17 | — | Constant-time comparison for hub auth |

**Release profile:**
```toml
[profile.release]
lto = "fat"
codegen-units = 1
strip = "symbols"
opt-level = "s"
```

**21 RUSTSEC audit exemptions** in `src-tauri/.cargo/audit.toml` — mostly Linux GTK and unic-* crates, not applicable on Windows.

### 2.2 Frontend (`package.json`)

**Runtime:** react ^19.1.0, react-dom ^19.1.0, @tauri-apps/api ^2, @tauri-apps/plugin-dialog ^2.7.1, @tauri-apps/plugin-opener ^2, @tauri-apps/plugin-biometry ^2, jsbarcode ^3.12.3, lucide-react ^1.16.0, zustand ^5.0.0, @modelcontextprotocol/sdk ^1.0.0.

**Dev:** typescript ~5.8.3, vite ^7.0.4, @vitejs/plugin-react ^7, vitest ^2.0.0, @vitest/coverage-v8 ^2.0.0, eslint ^10.4.0, @typescript-eslint/* ^8, tailwindcss ^4.0.0, @tailwindcss/vite ^4.0.0.

**Scripts:** `dev`, `build`, `tauri`, `type-check` (tsc --noEmit), `lint`, `test` (vitest run), `coverage` (vitest run --coverage), `check` (type-check + lint + test).

### 2.3 Vite Config (`vite.config.ts`)

- Tailwind CSS v4 plugin via `@tailwindcss/vite`
- Server port **1420**, HMR port **1421** (`strictPort: true`)
- esbuild drops `["console", "debugger"]` in production
- `chunkSizeWarningLimit: 200` (KB)
- 8 `manualChunks` groups: vendor (react), icons (lucide), jsbarcode, tauri-api, setup-wizard, migration, officeai, backoffice

### 2.4 TypeScript Config (`tsconfig.json`)

- Target: `ES2022`. Lib: `ES2022` + `DOM` + `DOM.Iterable`
- Strict: `true`. `noUnusedLocals: true`, `noUnusedParameters: true`, `noFallthroughCasesInSwitch: true`
- JSX: `react-jsx`

### 2.5 ESLint (`eslint.config.js`) — Flat config

- `no-console: warn` (allows warn/error), `@typescript-eslint/no-explicit-any: warn`, `no-unused-vars` with `argsIgnorePattern: "^_"`

### 2.6 Vitest Config (`vitest.config.ts`)

- Environment: `"node"`. Include: `src/**/*.{test,spec}.{ts,tsx}`
- Coverage: v8 provider, thresholds 80/80/80 (branches/functions/lines) informational

### 2.7 Tauri Config (`src-tauri/tauri.conf.json`)

- Window: 1440x900 default, 1024x700 minimum, maximized, no decorations
- CSP: `default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; connect-src 'self' http://127.0.0.1:3131 https://api.anthropic.com https://*.openai.com https://generativelanguage.googleapis.com`
- NSIS: perMachine install, `nsis/hooks.nsh` for firewall rules and wa-session cleanup
- Updater: configured with pubkey + endpoints
- `capabilities/default.json`: 14 permissions including core, dialog, fs, clipboard, updater, process, shell, opener, biometry

### 2.8 Sidecar (`src-tauri/sidecar/whatsapp-sidecar/`)

- **Runtime:** Node >= 20, Baileys v7.0.0-rc13 (the ONLY WhatsApp solution), express ^4.19.2, pino ^9.0.0, qrcode ^1.5.4, tesseract.js ^5.1.1
- Process managed via Windows Job Object (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) so sidecar dies with main process
- Communicates via HTTP on `127.0.0.1:3131` with `X-Sidecar-Token` auth header
- Tesseract.js used for BOTH payment screenshot OCR AND supplier invoice OCR (receipt → purchase order flow)

---

## 3. DATABASE SCHEMA

### 3.1 Pool Configuration (`db/mod.rs` — `init_db()`)

```rust
SqlitePoolOptions::new()
    .max_connections(6)
    .acquire_timeout(Duration::from_secs(10))
    .busy_timeout(Duration::from_secs(15))
    .connect_with(sqlite_connection)
```

**PRAGMAs executed on init:**
```sql
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 15000;
PRAGMA synchronous = NORMAL;
PRAGMA cache_size = -64000;  -- 64MB
PRAGMA temp_store = MEMORY;
PRAGMA mmap_size = 134217728;  -- 128MB
```

**cr-sqlite extension loading:**
```rust
// After pool init, load cr-sqlite CRDT extension
// SELECT crsql_load_extension(db_handle);
// cr-sqlite adds conflict-free replicated tables via CRDT types:
//   - crsql_as_crr(table_name) → enables CRDT on existing table
//   - Automatically adds __crsql_clock, __crsql_version columns
//   - Provides crsql_changes() for querying change deltas
//   - Clock-based merge: no manual conflict resolution needed
```

### 3.2 SQLite FTS5 Full-Text Search

```sql
-- FTS5 virtual table for product search
CREATE VIRTUAL TABLE IF NOT EXISTS products_fts USING fts5(
    name,
    sku,
    barcode,
    description,
    content='products',
    content_rowid='rowid',
    tokenize='porter unicode61'
);

-- Triggers to keep FTS5 index in sync with products table
CREATE TRIGGER IF NOT EXISTS products_fts_insert AFTER INSERT ON products BEGIN
    INSERT INTO products_fts(rowid, name, sku, barcode, description)
    VALUES (new.rowid, new.name, new.sku, new.barcode, new.description);
END;

CREATE TRIGGER IF NOT EXISTS products_fts_delete AFTER DELETE ON products BEGIN
    INSERT INTO products_fts(products_fts, rowid, name, sku, barcode, description)
    VALUES ('delete', old.rowid, old.name, old.sku, old.barcode, old.description);
END;

CREATE TRIGGER IF NOT EXISTS products_fts_update AFTER UPDATE ON products BEGIN
    INSERT INTO products_fts(products_fts, rowid, name, sku, barcode, description)
    VALUES ('delete', old.rowid, old.name, old.sku, old.barcode, old.description);
    INSERT INTO products_fts(rowid, name, sku, barcode, description)
    VALUES (new.rowid, new.name, new.sku, new.barcode, new.description);
END;
```

**FTS5 search query pattern:**
```sql
SELECT p.*, pp.price_minor, c.name AS category_name, tr.rate_basis_points,
       fts.rank AS relevance
FROM products_fts fts
JOIN products p ON p.rowid = fts.rowid
LEFT JOIN product_prices pp ON pp.product_id = p.product_id AND pp.price_type = 'selling'
LEFT JOIN categories c ON c.category_id = p.category_id
LEFT JOIN tax_rules tr ON tr.tax_rule_id = p.tax_rule_id
WHERE products_fts MATCH ?1  -- BM25 ranking by default
  AND p.is_active = 1
  AND p.deleted_at IS NULL
ORDER BY fts.rank
LIMIT 50;
```

**FTS5 constants:**
| Constant | Value |
|----------|-------|
| Tokenizer | porter unicode61 |
| Ranking | BM25 (default) |
| Search result limit | 50 |
| Query min length | 2 chars |

### 3.3 All Tables (9 migrations)

The schema is created across 9 migration files. The DDL below represents the full logical schema.

```sql
CREATE TABLE branches (
    branch_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    branch_code TEXT,
    address TEXT,
    phone TEXT,
    receipt_header TEXT,
    receipt_footer TEXT,
    tax_number TEXT,
    cr_number TEXT,
    currency TEXT NOT NULL DEFAULT 'BHD',
    timezone TEXT NOT NULL DEFAULT 'Asia/Bahrain',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE devices (
    device_id TEXT PRIMARY KEY,
    branch_id TEXT NOT NULL REFERENCES branches(branch_id),
    device_name TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE roles (
    role_id TEXT PRIMARY KEY,
    role_name TEXT NOT NULL UNIQUE  -- 'owner', 'manager', 'cashier', 'accountant'
);

CREATE TABLE users (
    user_id TEXT PRIMARY KEY,
    branch_id TEXT NOT NULL REFERENCES branches(branch_id),
    role_id TEXT NOT NULL REFERENCES roles(role_id),
    display_name TEXT NOT NULL,
    username TEXT NOT NULL UNIQUE,
    pin_hash TEXT NOT NULL,
    biometric_enabled INTEGER NOT NULL DEFAULT 0,  -- 1 if user enrolled biometric 2FA
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE categories (
    category_id TEXT PRIMARY KEY,
    branch_id TEXT NOT NULL REFERENCES branches(branch_id),
    name TEXT NOT NULL,
    parent_category_id TEXT REFERENCES categories(category_id),
    sort_order INTEGER NOT NULL DEFAULT 0,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE tax_rules (
    tax_rule_id TEXT PRIMARY KEY,
    branch_id TEXT NOT NULL REFERENCES branches(branch_id),
    name TEXT NOT NULL,
    rate_basis_points INTEGER NOT NULL,  -- e.g. 10000 = 10%
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE products (
    product_id TEXT PRIMARY KEY,
    branch_id TEXT NOT NULL REFERENCES branches(branch_id),
    category_id TEXT REFERENCES categories(category_id),
    tax_rule_id TEXT REFERENCES tax_rules(tax_rule_id),
    name TEXT NOT NULL,
    sku TEXT,
    barcode TEXT UNIQUE,
    description TEXT,
    image_path TEXT,
    track_inventory INTEGER NOT NULL DEFAULT 1,
    allow_decimal_quantity INTEGER NOT NULL DEFAULT 0,
    is_active INTEGER NOT NULL DEFAULT 1,
    reorder_point REAL NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    deleted_at TEXT
);

CREATE TABLE product_barcodes (
    product_id TEXT NOT NULL REFERENCES products(product_id),
    barcode TEXT NOT NULL UNIQUE,
    is_primary INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (product_id, barcode)
);

CREATE TABLE product_prices (
    price_id TEXT PRIMARY KEY,
    product_id TEXT NOT NULL REFERENCES products(product_id),
    price_minor INTEGER NOT NULL,
    currency TEXT NOT NULL DEFAULT 'BHD',
    price_type TEXT NOT NULL DEFAULT 'selling',  -- 'selling', 'promotional', 'cost'
    effective_from TEXT NOT NULL DEFAULT (datetime('now')),
    effective_to TEXT,
    created_by TEXT NOT NULL DEFAULT 'SYSTEM',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

```sql
-- Adds updated_at column to product_prices for sync tracking
ALTER TABLE product_prices ADD COLUMN updated_at TEXT NOT NULL DEFAULT (datetime('now'));
```

```sql
CREATE TABLE customers (
    customer_id TEXT PRIMARY KEY,
    branch_id TEXT NOT NULL REFERENCES branches(branch_id),
    name TEXT NOT NULL,
    phone TEXT,
    email TEXT,
    address TEXT,
    house_number TEXT,
    area TEXT,
    notes TEXT,
    loyalty_points INTEGER NOT NULL DEFAULT 0,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE sales (
    sale_id TEXT PRIMARY KEY,
    branch_id TEXT NOT NULL REFERENCES branches(branch_id),
    device_id TEXT NOT NULL REFERENCES devices(device_id),
    shift_id TEXT NOT NULL REFERENCES shifts(shift_id),
    cashier_user_id TEXT NOT NULL REFERENCES users(user_id),
    customer_id TEXT REFERENCES customers(customer_id),
    receipt_number TEXT NOT NULL,
    gross_total_minor INTEGER NOT NULL,
    discount_total_minor INTEGER NOT NULL DEFAULT 0,
    tax_total_minor INTEGER NOT NULL DEFAULT 0,
    net_total_minor INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'completed',  -- 'completed', 'voided', 'refunded'
    sold_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE sale_items (
    sale_item_id TEXT PRIMARY KEY,
    sale_id TEXT NOT NULL REFERENCES sales(sale_id),
    product_id TEXT REFERENCES products(product_id),
    name TEXT NOT NULL,
    sku TEXT,
    barcode TEXT,
    quantity REAL NOT NULL,
    unit_price_minor INTEGER NOT NULL,
    cost_minor INTEGER NOT NULL DEFAULT 0,
    line_discount_minor INTEGER NOT NULL DEFAULT 0,
    line_total_minor INTEGER NOT NULL,
    tax_rate_basis_points INTEGER NOT NULL DEFAULT 0,
    tax_inclusive INTEGER NOT NULL DEFAULT 0,
    note TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE payments (
    payment_id TEXT PRIMARY KEY,
    sale_id TEXT NOT NULL REFERENCES sales(sale_id),
    method TEXT NOT NULL,  -- 'cash', 'card', 'wallet', 'credit'
    amount_minor INTEGER NOT NULL,
    change_minor INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE shifts (
    shift_id TEXT PRIMARY KEY,
    branch_id TEXT NOT NULL REFERENCES branches(branch_id),
    device_id TEXT NOT NULL REFERENCES devices(device_id),
    cashier_user_id TEXT NOT NULL REFERENCES users(user_id),
    status TEXT NOT NULL DEFAULT 'open',  -- 'open', 'closed'
    float_minor INTEGER NOT NULL DEFAULT 0,
    cash_sales_minor INTEGER NOT NULL DEFAULT 0,
    card_sales_minor INTEGER NOT NULL DEFAULT 0,
    other_sales_minor INTEGER NOT NULL DEFAULT 0,
    cash_removed_minor INTEGER NOT NULL DEFAULT 0,
    counted_cash_minor INTEGER,
    variance_minor INTEGER,
    variance_status TEXT,  -- 'ok', 'over', 'under'
    notes TEXT,
    opened_at TEXT NOT NULL DEFAULT (datetime('now')),
    closed_at TEXT
);

CREATE TABLE cash_events (
    cash_event_id TEXT PRIMARY KEY,
    branch_id TEXT NOT NULL REFERENCES branches(branch_id),
    device_id TEXT NOT NULL REFERENCES devices(device_id),
    shift_id TEXT NOT NULL REFERENCES shifts(shift_id),
    event_type TEXT NOT NULL,  -- 'paid_in', 'paid_out', 'safe_drop', 'no_sale'
    amount_minor INTEGER NOT NULL,
    reason TEXT,
    created_by_user_id TEXT NOT NULL REFERENCES users(user_id),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE stock_levels (
    product_id TEXT NOT NULL REFERENCES products(product_id),
    branch_id TEXT NOT NULL REFERENCES branches(branch_id),
    quantity_on_hand REAL NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (product_id, branch_id)
);

CREATE TABLE stock_movements (
    movement_id TEXT PRIMARY KEY,
    product_id TEXT NOT NULL REFERENCES products(product_id),
    branch_id TEXT NOT NULL REFERENCES branches(branch_id),
    movement_type TEXT NOT NULL,  -- 'sale', 'refund', 'void_sale', 'adjustment', 'stock_take', 'receive'
    quantity REAL NOT NULL,
    reference_id TEXT,
    reason TEXT,
    created_by_user_id TEXT NOT NULL REFERENCES users(user_id),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE refunds (
    refund_id TEXT PRIMARY KEY,
    original_sale_id TEXT NOT NULL REFERENCES sales(sale_id),
    branch_id TEXT NOT NULL REFERENCES branches(branch_id),
    device_id TEXT NOT NULL REFERENCES devices(device_id),
    refunded_by_user_id TEXT NOT NULL REFERENCES users(user_id),
    manager_user_id TEXT REFERENCES users(user_id),
    total_refunded_minor INTEGER NOT NULL,
    reason TEXT NOT NULL,
    return_reason_code TEXT,  -- 'customer_return', 'defective', 'wrong_item', 'exchange', 'other'
    status TEXT NOT NULL DEFAULT 'completed',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE refund_items (
    refund_item_id TEXT PRIMARY KEY,
    refund_id TEXT NOT NULL REFERENCES refunds(refund_id),
    sale_item_id TEXT NOT NULL REFERENCES sale_items(sale_item_id),
    product_id TEXT NOT NULL REFERENCES products(product_id),
    quantity REAL NOT NULL,
    refund_amount_minor INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE audit_logs (
    audit_id TEXT PRIMARY KEY,
    branch_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id TEXT,
    actor_user_id TEXT NOT NULL,
    before_json TEXT,
    after_json TEXT,
    audit_hash TEXT NOT NULL,  -- SHA-256 chained
    prev_audit_hash TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE held_carts (
    held_id TEXT PRIMARY KEY,
    branch_id TEXT NOT NULL REFERENCES branches(branch_id),
    device_id TEXT NOT NULL REFERENCES devices(device_id),
    held_by_user_id TEXT NOT NULL REFERENCES users(user_id),
    label TEXT,
    cart_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE delivery_orders (
    delivery_id TEXT PRIMARY KEY,
    branch_id TEXT NOT NULL REFERENCES branches(branch_id),
    sale_id TEXT REFERENCES sales(sale_id),
    customer_id TEXT NOT NULL REFERENCES customers(customer_id),
    order_number TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',  -- 'pending','dispatched','delivered','cancelled','returned'
    payment_status TEXT NOT NULL DEFAULT 'unpaid',  -- 'unpaid','paid'
    total_minor INTEGER NOT NULL,
    delivery_charge_minor INTEGER NOT NULL DEFAULT 0,
    delivery_address TEXT,
    house_number TEXT,
    area TEXT,
    notes TEXT,
    rider_name TEXT,
    rider_phone TEXT,
    rider_vehicle TEXT,
    dispatched_at TEXT,
    delivered_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE ai_actions (
    action_id TEXT PRIMARY KEY,
    session_user_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    tool_input_json TEXT NOT NULL,
    tool_input_hash TEXT NOT NULL,
    preview_text TEXT,
    status TEXT NOT NULL DEFAULT 'prepared',  -- 'prepared','executed','cancelled','expired'
    result_json TEXT,
    confirmation_token TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE undo_records (
    undo_id TEXT PRIMARY KEY,
    action_id TEXT NOT NULL REFERENCES ai_actions(action_id),
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    snapshot_json TEXT NOT NULL,
    rollback_tool TEXT NOT NULL,
    rollback_input_json TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'available',  -- 'available','undone'
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    undone_at TEXT,
    undone_by_user_id TEXT
);

CREATE TABLE ai_sessions (
    session_id TEXT PRIMARY KEY,
    branch_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',  -- 'active','ended'
    total_turns INTEGER NOT NULL DEFAULT 0,
    tokens_in INTEGER NOT NULL DEFAULT 0,
    tokens_out INTEGER NOT NULL DEFAULT 0,
    cost_estimate_usd REAL NOT NULL DEFAULT 0.0,
    total_latency_ms INTEGER NOT NULL DEFAULT 0,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    ended_at TEXT
);

CREATE TABLE ai_usage_records (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES ai_sessions(session_id),
    turn INTEGER NOT NULL,
    tokens_in INTEGER NOT NULL DEFAULT 0,
    tokens_out INTEGER NOT NULL DEFAULT 0,
    latency_ms INTEGER NOT NULL DEFAULT 0,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    logged_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE ai_chat_messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    branch_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    role TEXT NOT NULL,  -- 'user','assistant','tool'
    content TEXT NOT NULL,
    message_type TEXT NOT NULL DEFAULT 'text',  -- 'text','tool_call','tool_result','error'
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE ai_feedback (
    feedback_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    message_id INTEGER NOT NULL,
    rating TEXT NOT NULL,  -- 'up','down'
    comment TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- cr-sqlite change tracking (managed by cr-sqlite extension, not user DDL)
-- crsql_changes table is auto-created by crsql_load_extension
-- Each CRR-enabled table gets __crsql_clock and __crsql_version columns

CREATE TABLE sync_watermarks (
    table_name TEXT PRIMARY KEY,
    last_pulled_at TEXT,
    last_pushed_at TEXT,
    cursor TEXT
);

-- sync_conflicts table retained for logging; cr-sqlite resolves conflicts automatically
CREATE TABLE sync_conflicts (
    conflict_id TEXT PRIMARY KEY,
    table_name TEXT NOT NULL,
    row_id TEXT NOT NULL,
    local_json TEXT NOT NULL,
    remote_json TEXT NOT NULL,
    resolution TEXT,  -- always 'crdt_merge' with cr-sqlite
    resolved_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE ghost_barcodes (
    ghost_id TEXT PRIMARY KEY,
    branch_id TEXT NOT NULL REFERENCES branches(branch_id),
    barcode TEXT NOT NULL,
    scan_count INTEGER NOT NULL DEFAULT 1,
    first_scanned_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_scanned_at TEXT NOT NULL DEFAULT (datetime('now')),
    prefill_product_id TEXT REFERENCES products(product_id),
    resolved INTEGER NOT NULL DEFAULT 0,
    dismissed INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE suppliers (
    supplier_id TEXT PRIMARY KEY,
    branch_id TEXT NOT NULL REFERENCES branches(branch_id),
    name TEXT NOT NULL,
    contact_name TEXT,
    phone TEXT,
    email TEXT,
    address TEXT,
    notes TEXT,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE purchase_orders (
    po_id TEXT PRIMARY KEY,
    branch_id TEXT NOT NULL REFERENCES branches(branch_id),
    supplier_id TEXT NOT NULL REFERENCES suppliers(supplier_id),
    status TEXT NOT NULL DEFAULT 'draft',  -- 'draft','ordered','received','cancelled'
    total_minor INTEGER NOT NULL DEFAULT 0,
    notes TEXT,
    source_invoice_scan_id TEXT,  -- FK to supplier_invoice_scans if OCR-created
    ordered_at TEXT,
    received_at TEXT,
    created_by_user_id TEXT NOT NULL REFERENCES users(user_id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE purchase_order_items (
    poi_id TEXT PRIMARY KEY,
    po_id TEXT NOT NULL REFERENCES purchase_orders(po_id),
    product_id TEXT REFERENCES products(product_id),
    name TEXT NOT NULL,
    quantity REAL NOT NULL,
    cost_minor INTEGER NOT NULL,
    received_quantity REAL NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Receipt OCR → purchase order: stores scanned supplier invoices
CREATE TABLE supplier_invoice_scans (
    scan_id TEXT PRIMARY KEY,
    branch_id TEXT NOT NULL REFERENCES branches(branch_id),
    supplier_id TEXT REFERENCES suppliers(supplier_id),
    image_path TEXT NOT NULL,                -- path to captured invoice image
    ocr_text TEXT,                           -- raw Tesseract OCR output
    structured_json TEXT,                    -- AI-structured line items
    status TEXT NOT NULL DEFAULT 'scanned',  -- 'scanned','ocr_done','structured','reviewed','po_created','archived'
    po_id TEXT REFERENCES purchase_orders(po_id),
    scanned_by_user_id TEXT NOT NULL REFERENCES users(user_id),
    scanned_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE customer_notes (
    note_id TEXT PRIMARY KEY,
    customer_id TEXT NOT NULL REFERENCES customers(customer_id),
    content TEXT NOT NULL,
    created_by_user_id TEXT NOT NULL REFERENCES users(user_id),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE override_tokens (
    token TEXT PRIMARY KEY,
    created_by_user_id TEXT NOT NULL REFERENCES users(user_id),
    purpose TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    used INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE whatsapp_messages (
    id TEXT PRIMARY KEY,
    chat_jid TEXT NOT NULL,
    chat_name TEXT,
    is_group INTEGER NOT NULL DEFAULT 0,
    sender_jid TEXT,
    sender_name TEXT,
    body TEXT,
    message_type TEXT,
    media_type TEXT,
    has_media INTEGER NOT NULL DEFAULT 0,
    ts INTEGER NOT NULL,
    read INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE whatsapp_targets (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    owner_jid TEXT,
    owner_name TEXT,
    group_jid TEXT,
    group_name TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE payment_confirmations (
    id TEXT PRIMARY KEY,
    customer_jid TEXT NOT NULL,
    customer_name TEXT,
    receipt_number TEXT NOT NULL,
    expected_amount_minor INTEGER NOT NULL,
    currency_exponent INTEGER NOT NULL DEFAULT 3,
    amount_found TEXT,
    name_matched INTEGER,
    status TEXT NOT NULL DEFAULT 'pending',  -- 'pending','confirmed','failed'
    reason TEXT,
    seen INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    resolved_at TEXT
);

CREATE TABLE proactive_alerts (
    alert_id TEXT PRIMARY KEY,
    branch_id TEXT NOT NULL REFERENCES branches(branch_id),
    alert_type TEXT NOT NULL,
    severity TEXT NOT NULL,  -- 'info','warning','critical'
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    detail_json TEXT,
    detected_at TEXT NOT NULL DEFAULT (datetime('now')),
    dismissed_at TEXT,
    dismissed_by_user_id TEXT REFERENCES users(user_id),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE ai_runs (
    run_id TEXT PRIMARY KEY,
    op_id TEXT NOT NULL,
    selector_json TEXT NOT NULL,
    params_json TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'previewing',  -- 'previewing','executing','done','cancelled','cancelling','failed','undone'
    total_count INTEGER NOT NULL DEFAULT 0,
    done_count INTEGER NOT NULL DEFAULT 0,
    checkpoint_cursor TEXT,
    error_message TEXT,
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE ai_run_undo_log (
    entry_id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL REFERENCES ai_runs(run_id),
    batch_seq INTEGER NOT NULL,
    reverse_json TEXT NOT NULL,
    applied INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE receipt_design (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    design_json TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE wa_message_format (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    format_json TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE product_versions (
    version_id TEXT PRIMARY KEY,
    product_id TEXT NOT NULL REFERENCES products(product_id),
    version INTEGER NOT NULL,
    change_type TEXT NOT NULL,  -- 'price','name','category','tax','cost','reorder','barcode','status'
    before_json TEXT NOT NULL,
    after_json TEXT NOT NULL,
    changed_by_user_id TEXT NOT NULL REFERENCES users(user_id),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

```sql
CREATE INDEX idx_sales_branch ON sales(branch_id);
CREATE INDEX idx_sales_receipt ON sales(receipt_number);
CREATE INDEX idx_sales_sold_at ON sales(sold_at);
CREATE INDEX idx_sale_items_sale ON sale_items(sale_id);
CREATE INDEX idx_payments_sale ON payments(sale_id);
CREATE INDEX idx_stock_product ON stock_levels(product_id);
CREATE INDEX idx_products_barcode ON products(barcode);
CREATE INDEX idx_products_category ON products(category_id);
CREATE INDEX idx_products_name ON products(name);
CREATE INDEX idx_customers_phone ON customers(phone);
CREATE INDEX idx_audit_created ON audit_logs(created_at);
CREATE INDEX idx_audit_entity ON audit_logs(entity_type, entity_id);
```

### 3.4 cr-sqlite CRDT Table Registration

After migrations run, enable CRDT on sync-relevant tables:

```rust
// In post_migration_fixup():
const CRR_TABLES: &[&str] = &[
    "branches", "categories", "tax_rules", "products", "product_prices",
    "product_barcodes", "customers", "devices", "users", "shifts",
    "sales", "sale_items", "payments", "refunds", "refund_items",
    "stock_movements", "stock_levels", "audit_logs", "delivery_orders",
    "cash_events", "purchase_orders", "purchase_order_items",
    "suppliers", "supplier_invoice_scans",
];

for table in CRR_TABLES {
    sqlx::query(&format!("SELECT crsql_as_crr('{}')", table))
        .execute(&pool)
        .await?;
}
```

cr-sqlite provides:
- **Automatic conflict resolution** via CRDT clocks (no manual merge strategies)
- **`crsql_changes()`** table function — query deltas since last sync
- **Clock-based ordering** — no FK-safe push ordering needed; CRDT handles causality
- **Per-row versioning** — `__crsql_version` column tracks change vectors

### 3.5 Post-migration fixup (`db/mod.rs`)

After migrations run, `post_migration_fixup()` checks for missing columns from intermediate dev migrations, enables cr-sqlite CRR on all tables, and rebuilds FTS5 index if needed.

---

## 4. TAURI COMMANDS

Every command is a `#[tauri::command]` async function receiving `State<'_, AppState>` and returning `Result<T, AppError>`. All commands decorated with `#[tracing::instrument(skip(state))]` for structured observability.

### 4.1 Auth Commands (`auth_commands.rs`)

```rust
#[tracing::instrument(skip(state))]
pub async fn auth_list_users(state: State<'_, AppState>) -> Result<Vec<AdminUserRow>, AppError>

#[tracing::instrument(skip(state))]
pub async fn auth_login_pin(user_id: String, pin: String, state: State<'_, AppState>) -> Result<SessionUser, AppError>
// Verifies PIN via auth_repo::verify_pin. Checks lockout (5 attempts, 60 min).
// If user has biometric_enabled=1, returns SessionUser with biometric_required: true
// for step-up verification before sensitive operations.

#[tracing::instrument(skip(state))]
pub async fn auth_verify_biometric(user_id: String, state: State<'_, AppState>) -> Result<bool, AppError>
// Invokes tauri-plugin-biometry to verify Windows Hello / Touch ID.
// Returns true if biometric matches enrolled credential.

#[tracing::instrument(skip(state))]
pub async fn auth_enroll_biometric(user_id: String, state: State<'_, AppState>) -> Result<(), AppError>
// Enrolls biometric credential via tauri-plugin-biometry.
// Sets users.biometric_enabled = 1.

#[tracing::instrument(skip(state))]
pub async fn auth_disable_biometric(user_id: String, actor_user_id: String, state: State<'_, AppState>) -> Result<(), AppError>
// manager_or_owner. Sets users.biometric_enabled = 0.

#[tracing::instrument(skip(state))]
pub async fn auth_reset_pin(target_user_id: String, new_pin: String, actor_user_id: String, state: State<'_, AppState>) -> Result<(), AppError>
// manager_or_owner. Hashes via auth_repo::hash_pin.

#[tracing::instrument(skip(state))]
pub async fn auth_lock_user(target_user_id: String, actor_user_id: String, state: State<'_, AppState>) -> Result<(), AppError>

#[tracing::instrument(skip(state))]
pub async fn auth_unlock_user(target_user_id: String, actor_user_id: String, state: State<'_, AppState>) -> Result<(), AppError>
```

### 4.2 RBAC Guards (`rbac.rs`)

```rust
pub async fn require_role(pool: &SqlitePool, actor_user_id: &str, allowed_roles: &[&str]) -> Result<(), AppError>
pub async fn owner_only(pool: &SqlitePool, user_id: &str) -> Result<(), AppError>
pub async fn manager_or_owner(pool: &SqlitePool, user_id: &str) -> Result<(), AppError>
pub async fn require_any_role(pool: &SqlitePool, user_id: &str) -> Result<(), AppError>
pub async fn can_override_refund(pool: &SqlitePool, user_id: &str) -> Result<bool, AppError>

// Biometric challenge gate for sensitive operations:
pub async fn require_biometric_if_enrolled(pool: &SqlitePool, user_id: &str) -> Result<(), AppError>
// If user has biometric_enabled=1, caller must present a valid biometric challenge response.
// Operations requiring biometric step-up: shift open (if enrolled), refund override, manager approval.
```

### 4.3 POS Commands (`pos_commands.rs`)

```rust
#[tracing::instrument(skip(state))]
pub async fn pos_start_sale(user_id: String, state: State<'_, AppState>) -> Result<String, AppError>

#[tracing::instrument(skip(state))]
pub async fn pos_add_line(token: String, product_id: String, quantity: String, user_id: String, state: State<'_, AppState>) -> Result<CartSession, AppError>

#[tracing::instrument(skip(state))]
pub async fn pos_add_custom_item(token: String, name: String, price_minor: i64, quantity: String, user_id: String, state: State<'_, AppState>) -> Result<CartSession, AppError>

#[tracing::instrument(skip(state))]
pub async fn pos_set_qty(token: String, line_id: String, quantity: String, user_id: String, state: State<'_, AppState>) -> Result<CartSession, AppError>

#[tracing::instrument(skip(state))]
pub async fn pos_remove_line(token: String, line_id: String, user_id: String, state: State<'_, AppState>) -> Result<CartSession, AppError>

#[tracing::instrument(skip(state))]
pub async fn pos_apply_discount(token: String, pct_or_minor: String, user_id: String, state: State<'_, AppState>) -> Result<CartSession, AppError>

#[tracing::instrument(skip(state))]
pub async fn pos_apply_line_discount(token: String, line_id: String, discount_minor: i64, user_id: String, state: State<'_, AppState>) -> Result<CartSession, AppError>

#[tracing::instrument(skip(state))]
pub async fn pos_set_line_price(token: String, line_id: String, price_minor: i64, user_id: String, state: State<'_, AppState>) -> Result<CartSession, AppError>

#[tracing::instrument(skip(state))]
pub async fn pos_finalize(token: String, payments: Vec<PaymentInput>, user_id: String, state: State<'_, AppState>) -> Result<SaleResult, AppError>

#[tracing::instrument(skip(state))]
pub async fn pos_get_cart(token: String, user_id: String, state: State<'_, AppState>) -> Result<CartSession, AppError>

#[tracing::instrument(skip(state))]
pub async fn pos_cancel_sale(token: String, user_id: String, state: State<'_, AppState>) -> Result<(), AppError>
```

### 4.4 Product Commands (`product_commands.rs`)

```rust
#[tracing::instrument(skip(state))]
pub async fn products_list(actor_user_id: String, state: State<'_, AppState>) -> Result<Vec<AdminProduct>, AppError>

#[tracing::instrument(skip(state))]
pub async fn products_search(query: String, category_id: Option<String>, actor_user_id: String, state: State<'_, AppState>) -> Result<Vec<AdminProduct>, AppError>
// Uses FTS5 with BM25 ranking for full-text search. Falls back to LIKE for short queries (< 2 chars).

#[tracing::instrument(skip(state))]
pub async fn product_get(product_id: String, actor_user_id: String, state: State<'_, AppState>) -> Result<AdminProduct, AppError>

#[tracing::instrument(skip(state))]
pub async fn product_create(data: CreateProductInput, actor_user_id: String, state: State<'_, AppState>) -> Result<AdminProduct, AppError>

#[tracing::instrument(skip(state))]
pub async fn product_update(id: String, data: UpdateProductInput, actor_user_id: String, state: State<'_, AppState>) -> Result<(), AppError>

#[tracing::instrument(skip(state))]
pub async fn product_set_active(id: String, active: bool, actor_user_id: String, state: State<'_, AppState>) -> Result<(), AppError>

#[tracing::instrument(skip(state))]
pub async fn product_get_by_barcode(barcode: String, actor_user_id: String, state: State<'_, AppState>) -> Result<Option<AdminProduct>, AppError>

#[tracing::instrument(skip(state))]
pub async fn products_list_all_active(actor_user_id: String, state: State<'_, AppState>) -> Result<Vec<AdminProduct>, AppError>

#[tracing::instrument(skip(state))]
pub async fn products_bulk_import(lines: Vec<BulkImportLine>, actor_user_id: String, state: State<'_, AppState>) -> Result<BulkImportResult, AppError>

#[tracing::instrument(skip(state))]
pub async fn products_duplicate_scan(actor_user_id: String, state: State<'_, AppState>) -> Result<Vec<DuplicateGroup>, AppError>

#[tracing::instrument(skip(state))]
pub async fn products_merge_groups(groups: Vec<MergeGroup>, actor_user_id: String, state: State<'_, AppState>) -> Result<MergeResult, AppError>

#[tracing::instrument(skip(state))]
pub async fn products_merge_all(groups: Vec<MergeGroup>, keeper_id: String, actor_user_id: String, state: State<'_, AppState>) -> Result<(), AppError>
```

### 4.5 Shift Commands (`shift_commands.rs`)

```rust
#[tracing::instrument(skip(state))]
pub async fn shift_get_active(device_id: String, actor_user_id: String, state: State<'_, AppState>) -> Result<Option<Shift>, AppError>

#[tracing::instrument(skip(state))]
pub async fn shift_open(input: OpenShiftInput, state: State<'_, AppState>) -> Result<Shift, AppError>
// If user has biometric_enabled, requires biometric challenge before opening shift.

#[tracing::instrument(skip(state))]
pub async fn shift_close(input: CloseShiftInput, state: State<'_, AppState>) -> Result<Shift, AppError>
```

### 4.6 Refund Commands (`refund_commands.rs`)

```rust
#[tracing::instrument(skip(state))]
pub async fn refund_get_sale(receipt_number: String, requesting_user_id: String, state: State<'_, AppState>) -> Result<SaleForRefund, AppError>

#[tracing::instrument(skip(state))]
pub async fn receipt_reprint(receipt_number: String, requesting_user_id: String, state: State<'_, AppState>) -> Result<SaleResult, AppError>

#[tracing::instrument(skip(state))]
pub async fn refund_create(input: CreateRefundInput, state: State<'_, AppState>) -> Result<RefundResult, AppError>
// Cross-device refund requires biometric step-up if enrolled, or manager override token.
```

### 4.7-4.22 Remaining Commands

All remaining command modules (`cash_commands.rs`, `report_commands.rs`, `setup_commands.rs`, `sync_commands.rs`, `hub_commands.rs`, `thermal_commands.rs`, `whatsapp_commands.rs`, `migration_commands.rs`, `delivery_commands.rs`, `ai_admin_commands.rs`, `ghost_barcode_commands.rs`, `device_commands.rs`, `held_cart_commands.rs`, `customer_commands.rs`, `inventory_commands.rs`, `updater_commands.rs`, `phase10a_commands.rs`) retain their existing signatures as documented in the prior specification, with the addition of `#[tracing::instrument(skip(state))]` on every command and the following new OCR-related commands:

```rust
// New: Receipt OCR commands (whatsapp_commands.rs — reuses sidecar OCR)
#[tracing::instrument(skip(state))]
pub async fn invoice_scan_start(image_path: String, actor_user_id: String, state: State<'_, AppState>) -> AppResult<InvoiceScan>
// Phase 1: Saves scan record, triggers sidecar OCR via /ocr endpoint.

#[tracing::instrument(skip(state))]
pub async fn invoice_scan_status(scan_id: String, actor_user_id: String, state: State<'_, AppState>) -> AppResult<InvoiceScan>
// Returns current status + OCR text if done.

#[tracing::instrument(skip(state))]
pub async fn invoice_scan_structure(scan_id: String, actor_user_id: String, state: State<'_, AppState>) -> AppResult<InvoiceScan>
// Phase 2: AI structures OCR text into line items (name, quantity, cost).

#[tracing::instrument(skip(state))]
pub async fn invoice_scan_create_po(scan_id: String, supplier_id: String, actor_user_id: String, state: State<'_, AppState>) -> AppResult<PurchaseOrder>
// Phase 3: Creates purchase order from structured invoice data.
```

### 4.23 MCP Server Commands (`mcp_commands.rs`) — NEW

```rust
#[tracing::instrument(skip(state))]
pub async fn mcp_list_tools(state: State<'_, AppState>) -> Result<Value, AppError>
// Returns JSON-RPC tools/list response — all registered MCP tools.

#[tracing::instrument(skip(state))]
pub async fn mcp_call_tool(name: String, arguments: Value, state: State<'_, AppState>) -> Result<Value, AppError>
// JSON-RPC tools/call — dispatches to MCP tool handler.

#[tracing::instrument(skip(state))]
pub async fn mcp_list_resources(state: State<'_, AppState>) -> Result<Value, AppError>
// Returns available MCP resources (products, sales, reports as resource URIs).

#[tracing::instrument(skip(state))]
pub async fn mcp_read_resource(uri: String, state: State<'_, AppState>) -> Result<Value, AppError>
// Reads a resource by URI (e.g., "zanpos://products/{id}", "zanpos://reports/today").
```

---

## 5. DOMAIN MODEL & BUSINESS LOGIC

### 5.1 Money (`domain/money.rs`)

All monetary values are `i64` representing minor currency units (fils for BHD, exponent=3).

```rust
pub const MAX_QTY: i64 = 1_000_000;
pub const MAX_MINOR: i64 = 1_000_000_000_000;

pub fn mul_minor_by_qty(price_minor: i64, qty: &str) -> AppResult<i64>
pub fn qty_in_range(qty: &str) -> bool
pub fn add_decimal_qty_str(existing: &str, delta: &str) -> AppResult<String>
pub fn parse_major_to_minor(major: &str, exponent: i32) -> AppResult<i64>
pub fn format_minor(minor: i64, exponent: i32) -> String
pub fn apply_discount_bp(amount_minor: i64, basis_points: i64) -> i64
pub fn calc_tax_exclusive(net_minor: i64, rate_basis_points: i64) -> i64
```

### 5.2 Cart (`domain/cart.rs`)

```rust
pub struct CartLine {
    pub line_id: String, pub product_id: String, pub name: String,
    pub sku: Option<String>, pub barcode: Option<String>,
    pub price_minor: i64, pub cost_minor: i64, pub quantity: String,
    pub note: Option<String>, pub line_discount_minor: i64,
    pub line_total_minor: i64, pub tax_rate_basis_points: i64,
    pub tax_inclusive: bool, pub image_url: Option<String>,
}

pub struct Cart {
    pub session_id: String, pub branch_id: String, pub device_id: String,
    pub lines: Vec<CartLine>, pub discount_total_minor: i64,
    pub net_total_minor: i64, pub tax_total_minor: i64,
}
```

### 5.3 Sale, Refund, Shift, AI Admin

Same structs as prior specification (SaleResult, PaymentSummary, SaleForRefund, RefundableItem, RefundResult, Shift, FeatureToggles, BusinessFlags, OperationalSettings). See prior spec Sections 5.3-5.6 for full type definitions.

### 5.4 Zustand Store Interfaces (Frontend) — NEW

```typescript
// stores/authStore.ts
interface AuthState {
  sessionUser: SessionUser | null;
  isAuthenticated: boolean;
  login: (userId: string, pin: string) => Promise<void>;
  logout: () => void;
  verifyBiometric: () => Promise<boolean>;
}

// stores/shiftStore.ts
interface ShiftState {
  activeShift: Shift | null;
  isOpen: boolean;
  openShift: (input: OpenShiftInput) => Promise<void>;
  closeShift: (input: CloseShiftInput) => Promise<void>;
  refreshShift: () => Promise<void>;
}

// stores/themeStore.ts
interface ThemeState {
  theme: 'dark' | 'light' | 'midnight' | 'forest' | 'blossom' | 'sky' | 'sage';
  setTheme: (theme: string) => void;
}

// stores/syncStore.ts
interface SyncState {
  status: SyncStatus | null;
  isOnline: boolean;
  lastSyncAt: string | null;
  pendingChanges: number;
  refreshStatus: () => Promise<void>;
  triggerSync: () => Promise<void>;
}

// stores/cartStore.ts — backs useCart hook
interface CartStore {
  cart: CartSession | null;
  token: string | null;
  isLoading: boolean;
  startSale: (userId: string) => Promise<string>;
  addLine: (productId: string, qty: string) => Promise<void>;
  removeLine: (lineId: string) => Promise<void>;
  setQty: (lineId: string, qty: string) => Promise<void>;
  applyDiscount: (pctOrMinor: string) => Promise<void>;
  finalize: (payments: PaymentInput[]) => Promise<SaleResult>;
  cancel: () => Promise<void>;
  clearCart: () => void;
}

// stores/chatStore.ts — backs useChatController hook
interface ChatStore {
  messages: ChatMessage[];
  isStreaming: boolean;
  sessionId: string | null;
  sendMessage: (content: string) => Promise<void>;
  clearHistory: () => Promise<void>;
  abortStreaming: () => void;
}
```

### 5.5 Additional Domain Types

Same as prior specification Sections 13.9 (AdminProduct, SessionUser, DeliveryInput, report structs). See prior spec for full definitions.

---

## 6. AI SYSTEM

### 6.1 Multi-Provider Architecture

**Provider enum** (`ai/provider.rs`):
```rust
pub enum Provider {
    Anthropic(AnthropicClient),
    OpenAI(OpenAIClient),
    Gemini(OpenAIClient),  // Gemini via OpenAI-compatible endpoint
}
```

**`Provider::from_db()`** reads `ai_provider` from app_config:
- `"anthropic"` → default model `"claude-sonnet-4-6"`. Prefers OS keyring via `secure_store`.
- `"openai"` → needs key + base_url + model. Compatible with Groq, OpenRouter, DeepSeek, Ollama.
- `"gemini"` → uses `GEMINI_BASE_URL`, default model `"gemini-2.0-flash"`.

**Provider::from_db_with_fallback()** tries primary, then falls back Anthropic → OpenAI → Gemini.

**AnthropicClient** (`ai/client.rs`):
```rust
pub struct AnthropicClient {
    api_key: String, model: String,
    max_tokens: u32,           // default 4096
    temperature: Option<f32>,
    http: Client,              // connect_timeout=10s, timeout=60s
}
```

**OpenAIClient** (`ai/openai_client.rs`):
```rust
pub struct OpenAIClient {
    base_url: String, api_key: String, model: String,
    max_tokens: u32, temperature: Option<f32>,
    http: Client,              // connect_timeout=10s, timeout=1800s
}
```

### 6.2 MCP Tool Protocol — NEW (replaces dispatch chain)

All AI tools are exposed as MCP tools following the Model Context Protocol (Linux Foundation, 97M+ monthly SDK downloads). This replaces the 7-file dispatch chain (`tools.rs` → `tools_read_ext.rs` → `tools_read_ext2.rs` → `tools_read_ext3.rs` / `tools_write_ext.rs` → `tools_write_ext2.rs` → `tools_write_ext3.rs`).

**MCP Server (`ai/mcp_server.rs`):**
```rust
pub struct McpServer {
    tools: HashMap<String, McpTool>,
    resources: HashMap<String, McpResource>,
}

pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,  // JSON Schema
    pub handler: McpToolHandler,
}

pub enum McpToolHandler {
    Read(fn(&AppState, Value) -> Pin<Box<dyn Future<Output=AppResult<Value>> + Send>>),
    Mutation(fn(&AppState, Value) -> Pin<Box<dyn Future<Output=AppResult<Value>> + Send>>),
    Engine(fn(&AppState, Value) -> Pin<Box<dyn Future<Output=AppResult<StreamEvent>> + Send>>),
}

pub struct McpResource {
    pub uri_pattern: String,   // e.g. "zanpos://products/{id}"
    pub name: String,
    pub description: String,
    pub handler: fn(&AppState, HashMap<String, String>) -> Pin<Box<dyn Future<Output=AppResult<Value>> + Send>>,
}

impl McpServer {
    pub fn new() -> Self
    pub fn register_tool(&mut self, tool: McpTool)
    pub fn register_resource(&mut self, resource: McpResource)
    pub fn list_tools(&self) -> Vec<Value>
    pub fn call_tool(&self, name: &str, args: Value) -> AppResult<Value>
    pub fn list_resources(&self) -> Vec<Value>
    pub fn read_resource(&self, uri: &str) -> AppResult<Value>
}
```

**Tool registration** (`ai/mcp_tools.rs`):
```rust
pub fn register_all_tools(server: &mut McpServer) {
    // 137 mutation tools registered via register_tool()
    server.register_tool(McpTool {
        name: "update_product_price".into(),
        description: "Update the selling price of a product...".into(),
        input_schema: json!({ "type": "object", "properties": { ... } }),
        handler: McpToolHandler::Mutation(|state, args| Box::pin(async { ... })),
    });
    // ... all other tools

    // ~91 core read tools
    // ~53 gated analytics tools (behind feature toggles)

    // Resources for direct data access
    server.register_resource(McpResource {
        uri_pattern: "zanpos://products/{id}".into(),
        name: "Product Detail".into(),
        description: "Full product with price, stock, category".into(),
        handler: |state, params| Box::pin(async { ... }),
    });
    server.register_resource(McpResource {
        uri_pattern: "zanpos://reports/today".into(),
        name: "Today Report".into(),
        description: "Today's sales summary".into(),
        handler: |state, _| Box::pin(async { ... }),
    });
    server.register_resource(McpResource {
        uri_pattern: "zanpos://sales/{id}".into(),
        name: "Sale Detail".into(),
        description: "Complete sale with items and payments".into(),
        handler: |state, params| Box::pin(async { ... }),
    });
}
```

**MCP JSON-RPC message format:**
```json
// Request (tools/call)
{ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "update_product_price", "arguments": { "product_id": "...", "new_price_minor": 1500 } } }

// Response (success)
{ "jsonrpc": "2.0", "id": 1, "result": { "content": [{ "type": "text", "text": "Price updated..." }] } }

// Request (tools/list)
{ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }

// Response
{ "jsonrpc": "2.0", "id": 1, "result": { "tools": [{ "name": "update_product_price", "description": "...", "inputSchema": {...} }, ...] } }
```

**Benefits over prior dispatch chain:**
- Single registry instead of 7 files with chained dispatch
- Standard protocol — any MCP-compatible client can use ZanPOS tools
- Resource URIs provide direct data access without tool invocation
- Input schemas are standard JSON Schema (already used in engine ops)
- No more `execute_read_tool → ext → ext2 → ext3` dispatch

### 6.3 Tool Inventory (unchanged from prior spec)

**137 mutation tools** — same list as prior specification Section 6.3.

**~91 core read tools** — same list. Product search uses FTS5 via:
```sql
SELECT ... FROM products_fts WHERE products_fts MATCH ?1 ORDER BY rank LIMIT 50;
```

**~53 gated analytics tools** — same list, behind feature toggles.

### 6.4 Risk Classification

Same as prior spec Section 6.5. `mutation_risk()` classifies as Low/High. `resolve_mutation_risk()` upgrades price changes at >=50% swing. MCP tool handler wraps risk check before execution.

### 6.5 Undo System

Per-action undo via `ai_admin_repo.rs` and engine-level batch undo via `engine/batch.rs`. Same as prior spec Section 6.6.

### 6.6 Engine Operations (`ai/engine/`)

Same Operation trait, Registry, 8 concrete operations, Selector, PriceOp, Run lifecycle, and batch execution as prior spec Section 6.7. Engine operations are registered as MCP tools with `McpToolHandler::Engine`.

### 6.7 Intent Engine (`ai/intent_engine.rs`)

18 intents unchanged. Dispatched via MCP tool calls.

### 6.8 Streaming Protocol (`ai/streaming.rs`)

14 StreamEvent variants unchanged. Anthropic/OpenAI SSE loops unchanged. The streaming dispatch now routes through the MCP tool registry instead of hardcoded dispatch.

### 6.9 Feature Toggle Gates (9 toggles)

Unchanged from prior spec Section 6.10. Toggle state checked in MCP tool handler before executing gated tools.

### 6.10 Migration Agent (12 AI tools)

Unchanged from prior spec Section 6.11. Registered as MCP tools with prefix `mg_`.

### 6.11 Key AI Constants

Same as prior spec Section 6.12.

---

## 7. POS & SALES ENGINE

Same 21-step finalize_sale transaction, refund logic, and held cart operations as prior spec Sections 7.1-7.3.

---

## 8. INVENTORY & STOCK MANAGEMENT

Same stock movements and repository as prior spec Sections 8.1-8.2.

---

## 9. SYNC SYSTEM & HUB — UPGRADED (cr-sqlite CRDT)

### 9.1 CRDT-Based Architecture

cr-sqlite provides multi-master conflict-free replication. Each terminal has a local SQLite database with the cr-sqlite CRDT extension. The embedded hub acts as a WAN relay — it merges changes from all terminals and redistributes them.

**Three operating modes:**
- **`standalone`**: no sync, local-only. cr-sqlite still active locally.
- **`hub`**: runs embedded axum server on port 8923. Accepts cr-sqlite change deltas from terminals, merges, redistributes.
- **`terminal`**: connects to a hub. Pushes local `crsql_changes()` deltas, pulls merged deltas from hub.

### 9.2 Sync Worker (`sync_v2/worker.rs`)

```rust
pub struct SyncWorker { /* ... */ }
pub fn spawn(pool, hub_mode, hub_url, hub_token, device_id, branch_id) -> SyncHandle
pub async fn run_once(&self) -> AppResult<()>
```

**Constants:**
```rust
const MAX_HTTP_RETRIES: u32 = 5;
const BASE_RETRY_MS: u64 = 200;
const MAX_RETRY_DELAY_MS: u64 = 30_000;
```

**CRDT sync flow (replaces prior push/pull ordering):**
1. Query local changes: `SELECT * FROM crsql_changes()` since last sync cursor
2. POST changes to hub `/sync/push` as `CrsqlDelta { table, row_id, cols, clock }`
3. Hub applies deltas via `INSERT OR REPLACE` (CRDT clock determines winner)
4. Hub returns merged deltas from other terminals
5. Terminal applies remote deltas locally
6. No merge strategies needed — CRDT clock ordering resolves all conflicts
7. No FK-safe push ordering needed — CRDT causality tracking handles dependencies

**The 19-table FK-safe push ordering and per-table merge strategies (LWW, append-only, customer MAX merge, config allowlist) are REMOVED.** cr-sqlite's CRDT handles all of this automatically.

### 9.3 Hub Server (`hub/`)

```rust
pub struct HubState {
    pub pool: SqlitePool,
    pub token_digest: String,
    pub device_id: String,
    pub branch_id: String,
}
```

**5 axum routes** (`hub/rest.rs`):
- `POST /sync/push` — receives CRDT deltas, applies via `INSERT OR REPLACE`, returns merged deltas for requesting terminal
- `POST /sync/pull` — returns CRDT deltas since watermark (for new terminal bootstrap)
- `GET /sync/changes` — returns `crsql_changes()` since cursor (used by terminals)
- `GET /sync/status` — health check with CRDT clock stats
- `GET /health` — liveness probe

**Auth middleware:** constant-time XOR comparison of `Authorization: Bearer <token>` digest against stored `token_digest`.

**LAN discovery:** `lan_ips()` returns all non-loopback IPv4 addresses. UDP broadcast on port 8923 for terminal auto-discovery.

### 9.4 cr-sqlite Change Replication

```rust
// On terminal: get local changes since last sync
let changes = sqlx::query_as::<_, CrsqlChange>(
    "SELECT \"table\", pk, cid, val, col_version, db_version, site_id FROM crsql_changes()"
).fetch_all(&pool).await?;

// On hub: apply incoming deltas (CRDT clock wins)
for change in &changes {
    sqlx::query(&format!(
        "INSERT OR REPLACE INTO \"{}\" (rowid, {cols}) VALUES (?1, ?2) ON CONFLICT(rowid) DO UPDATE SET {updates}",
        change.table
    )).execute(&pool).await?;
}
```

### 9.5 Secure Store (`secure_store.rs`)

Unchanged from prior spec Section 9.5.

### 9.6 Sync Scope (`sync/scope.rs`)

Unchanged from prior spec Section 9.6.

### 9.7 Sync Tables

24 tables registered as CRR (Conflict-free Replicated Relations). cr-sqlite automatically adds `__crsql_clock` and `__crsql_version` tracking columns.

---

## 10. AUTH & SECURITY — UPGRADED

### 10.1 PIN Hashing (`auth_repo.rs`)

```rust
pub fn hash_pin(pin: &str) -> AppResult<String>
pub fn verify_pin(pin: &str, hash: &str) -> AppResult<bool>

// Argon2id params:
// m_cost = 19456 (19 MiB)
// t_cost = 2 iterations
// p_cost = 1 parallelism
// output_len = 32 bytes
// salt_len = 22 chars
```

**Login PIN flow:**
1. Look up user by user_id
2. Check `is_active` flag
3. Check lockout: 5 failed attempts within 60 minutes → locked
4. Verify PIN via Argon2id
5. On success: reset attempt counter, return SessionUser
6. On failure: increment attempt counter, record timestamp

**Migration support:** `rehash_plain_pins()` converts legacy PLAIN hashes to Argon2id.

### 10.2 Biometric 2FA (`biometry.rs`) — NEW

`tauri-plugin-biometry` v2 provides optional second-factor authentication using Windows Hello (Windows) and Touch ID (macOS).

```rust
/// Check if biometric auth is available on this device
pub async fn biometry_available(app: &AppHandle) -> Result<bool, AppError>

/// Enroll a biometric credential for a user
/// Stores a WebAuthn-compatible credential bound to the user's device
pub async fn biometry_enroll(app: &AppHandle, user_id: &str) -> Result<(), AppError>

/// Verify biometric against enrolled credential
/// Returns true if user's face/fingerprint matches
pub async fn biometry_verify(app: &AppHandle, user_id: &str) -> Result<bool, AppError>

/// Disable biometric for a user
pub async fn biometry_disable(app: &AppHandle, user_id: &str) -> Result<(), AppError>
```

**Biometric challenge flow:**
1. User has `biometric_enabled = 1` in users table
2. Sensitive operation requested (shift open, refund override, manager approval)
3. Frontend calls `auth_verify_biometric(user_id)` → Rust invokes `tauri-plugin-biometry`
4. OS-native biometric dialog appears (Windows Hello face/fingerprint/PIN, Touch ID)
5. On success: operation proceeds. On failure: operation blocked, audit log recorded
6. Per-frontend session caching: biometric verification valid for 5 minutes or until lock

**Sensitive operations requiring biometric step-up (if enrolled):**
| Operation | Requires Biometric |
|-----------|-------------------|
| Open shift | Yes (if enrolled) |
| Cross-device refund | Yes (if enrolled) |
| Manager override approval | Yes (if enrolled) |
| Change AI provider keys | Yes (if enrolled) |
| Enable/disable hub mode | Yes (if enrolled) |
| Close shift (if not owning cashier) | Yes (if enrolled) |

**Biometric is ALWAYS optional.** PIN remains the primary authentication method. Biometric is a second factor for high-risk operations only. The Argon2id PIN workflow is unchanged.

### 10.3 Audit Hash Chain (`audit_hash.rs`)

Unchanged — SHA-256 of 12 NUL-separated fields, chain verification.

### 10.4 Hub Token Auth

Unchanged — SHA-256 digest + constant-time XOR comparison.

### 10.5 RBAC Roles

| Role | Access |
|------|--------|
| **owner** | Everything: setup, AI config, hub config, user management, all reports, all mutations |
| **manager** | Most things: reports, refunds, products, customers, inventory. Cannot change AI provider keys or hub mode |
| **cashier** | POS operations only: sales, refunds (same-device), shift open/close, basic reports. Biometric 2FA available |
| **accountant** | Read-only access to all reports, audit logs, and financial data. Cannot perform POS operations |

---

## 11. WHATSAPP & SIDECAR — BAILEYS ONLY

### 11.1 Sidecar Server (`whatsapp-sidecar/server.js`)

Baileys v7.0.0-rc13 is the **ONLY** WhatsApp solution. There is no official WhatsApp Business API integration, no Twilio/MessageBird fallback, and no alternate providers.

- **Runtime:** Node >= 20, Express ^4.19.2, port **3131**
- **Baileys v7.0.0-rc13** with config:
  - Browser: `["macOS", "Desktop", "Chrome"]`
  - `connectTimeoutMs: 60000`, `keepAliveIntervalMs: 30000`, `qrTimeout: 60000`
  - `INBOX_CAP: 300`, `MEDIA_CAP: 40`
- **Authentication:** `X-Sidecar-Token` header on all endpoints
- **Tesseract.js OCR:** OEM 1 (LSTM only), lazy singleton worker, English language
- **Media persistence:** FIFO eviction, in-memory buffer with disk fallback

**11 endpoints:**
| Endpoint | Method | Description |
|----------|--------|-------------|
| `/status` | GET | Connection status + QR code if not paired |
| `/connect` | POST | Initiate WhatsApp connection |
| `/disconnect` | POST | Disconnect and clear session |
| `/send-message` | POST | Send text message to JID |
| `/send-document` | POST | Send PDF/document to JID |
| `/contacts` | GET | List all WhatsApp contacts |
| `/groups` | GET | List all WhatsApp groups |
| `/messages` | GET | Poll new messages (cursor-based) |
| `/messages/:id/media` | GET | Download decrypted media |
| `/mark-read` | POST | Mark message as read |
| `/ocr` | POST | Run OCR on image (base64 input) — used for BOTH payment verification AND supplier invoice scanning |

### 11.2 WhatsApp Business Rules

- **Delivery notification:** 1-hour window from order creation
- **Payment reminder:** same-day only (Bahrain UTC+3), bilingual (EN/AR)
- **BenefitPay number:** configurable, stored in app_config `whatsapp_benefit_number`
- **Phone validation:** must start with `+`, 8-16 chars, digits-only after `+`
- **Contact import:** `INSERT OR IGNORE` from sidecar contacts into customers table

### 11.3 Payment Confirmation Pipeline

Unchanged from prior spec Section 11.3.

### 11.4 Baileys Risk Mitigation

Baileys is an unofficial WhatsApp Web reverse-engineering library. Risk mitigations:
- Patch subscription via GitHub Releases monitoring (manual check each release)
- Sidecar restarts handled gracefully (cursor rewind, exponential backoff)
- Phone-only mode (no business account linking needed)
- Session backup/restore via file-based creds persistence
- Rate limiting: max 5 messages/second, max 50 messages/minute
- If WhatsApp blocks the connection, sidecar logs the error and the system falls back to manual notification

---

## 12. THERMAL PRINTING

Unchanged from prior spec Sections 12.1-12.4. ESC/POS byte builders, Windows spooler printing, serial port support, port discovery via WMI.

---

## 13. FRONTEND ARCHITECTURE — UPGRADED

### 13.1 App.tsx State Machine

Same 5 views (`login` → `shift_check` → `shift_open` → `pos` ↔ `office_ai`). Key state managed through Zustand stores instead of scattered React state + props.

### 13.2 State Management — Zustand

Zustand (~1.2KB gzipped, 14.2M weekly downloads) replaces the previous pure-React-state approach for shared cross-component state. Component-local state (form inputs, modal open/close, transient UI) stays as React state.

**Store architecture:**
```
src/stores/
├── authStore.ts       — session user, login/logout, biometric state
├── shiftStore.ts      — active shift, open/close operations
├── themeStore.ts      — theme selection, persistence
├── syncStore.ts       — sync status, online/offline, pending count
├── cartStore.ts       — cart state (backs useCart hook)
├── chatStore.ts       — OfficeAI chat state (backs useChatController hook)
├── settingsStore.ts   — app config, feature toggles, preferences
└── notificationStore.ts — reminder popups, proactive alerts
```

**`useCart` and `useChatController` remain as hooks** but are backed by Zustand stores internally. The hooks expose the same interface to components — migration is transparent to callers.

**`DEVICE` singleton** in `types.ts` remains for currency/exponent/branch info (read-only config).

**localStorage** retained for: sticky notes, WA message format, receipt design, custom item suggestions, dock state (preferences not shared across components).

### 13.3 Tailwind CSS v4 Migration — NEW

**Strategy:** Incremental migration from the 15,456-line monolithic `App.css` to Tailwind CSS v4 utility classes.

**Phase 1 — Foundation (week 1):**
- Install `tailwindcss ^4.0.0` + `@tailwindcss/vite` plugin
- Configure `@theme` in `src/app.css`:
```css
@import "tailwindcss";

@theme {
  --color-brand-50: #FFF8EB;
  --color-brand-100: #FDE9BF;
  --color-brand-500: #F0A500;
  --color-brand-600: #D49200;
  --color-brand-700: #B87A00;
  --color-surface-dark: #1A1713;
  --color-bg-dark: #0E0C0A;
  --color-surface-light: #FFFFFF;
  --color-bg-light: #F0F0F0;
  /* ... full oklch-derived palette for 7 themes */
  --font-sans: "DM Sans", sans-serif;
  --font-display: "Bricolage Grotesque", sans-serif;
  --font-mono: "IBM Plex Mono", monospace;
  --font-code: "JetBrains Mono", monospace;
  --font-brand: "Syncopate", sans-serif;
  --radius-base: 14px;
  --radius-pill: 999px;
}
```
- Map design tokens to Tailwind theme (z-index layers, timing, easing, semantic colors)
- Keep existing `App.css` as fallback; Tailwind utilities override where specified

**Phase 2 — Components (weeks 2-3):**
- Convert components one at a time, starting with standalone components (modals, buttons, form inputs)
- Extract shared patterns: `.btn-primary`, `.btn-ghost`, `.input-field`, `.modal-overlay`, `.modal`
- Port 41+ @keyframes to Tailwind `@keyframes` + `animate-*` utilities
- Theme switching via `data-theme` attribute with Tailwind dark mode variant

**Phase 3 — Layout (week 4):**
- Convert page layouts (PosPage, OfficeAIPage, settings tabs)
- Extract layout components (sidebar, header, tab bar)

**Phase 4 — Cleanup (week 5):**
- Remove unused CSS from App.css
- Split remaining non-Tailwind styles by domain (pos.css, officeai.css, settings.css, login.css, shared.css)
- Target: App.css reduced from 15,456 lines to <2,000 lines (theme variables, complex animations, print styles)

**Why Tailwind CSS v4:**
- Rust Oxide engine — faster builds, zero JS runtime
- CSS-first @theme config — no tailwind.config.js needed
- oklch color space — perceptually uniform colors
- Production CSS: 8-15KB vs current ~250KB+
- Atomic classes enable removing most of the 15,456-line monolith

### 13.4 Hooks

Same 8 hooks as prior spec Section 13.3. `useCart` and `useChatController` are now backed by Zustand stores but expose the same interface.

### 13.5 POS Keyboard Shortcuts

Unchanged from prior spec Section 13.4.

### 13.6 Modal Pattern

Unchanged from prior spec Section 13.5.

### 13.7 TypeScript → Tauri Command Mapping

Same as prior spec Section 13.6.

### 13.8 Rust Infrastructure Files — UPGRADED

**Tracing initialization** (`src-tauri/src/tracing_init.rs`) — NEW:
```rust
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use tracing_appender::rolling::{RollingFileAppender, Rotation};

pub fn init_tracing() {
    // JSON-formatted file logs with rotation
    let file_appender = RollingFileAppender::new(
        Rotation::DAILY,
        "logs",
        "zanpos.log",
    );

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(
            "zanpos_lib=debug,zanpos_lib::ai=info,zanpos_lib::sync_v2=trace,sqlx=warn"
        ));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            fmt::layer()
                .json()
                .with_target(true)
                .with_file(true)
                .with_line_number(true)
                .with_writer(file_appender),
        )
        .with(
            fmt::layer()
                .pretty()
                .with_target(false)
                .with_writer(std::io::stdout),
        )
        .init();
}
```

**`#[tracing::instrument]` usage:**
```rust
#[tracing::instrument(skip(state), fields(user_id = %actor_user_id, branch_id))]
pub async fn products_search(
    query: String,
    category_id: Option<String>,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<AdminProduct>, AppError> {
    // ...
}
```

Every `#[tauri::command]` is decorated with `#[tracing::instrument]`. Logs include: span enter/exit, arguments (non-sensitive), timing, error details.

**Production log configuration:**
- JSON format for log aggregation (Elasticsearch / Loki compatible)
- Daily rotation with 30-day retention
- Per-module filtering via `RUST_LOG` env or compile-time `EnvFilter`
- Default: `info` level, `debug` for zanpos_lib, `trace` for sync, `warn` for sqlx

**`src-tauri/src/lib.rs`** — AppState updated:
```rust
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Arc<RwLock<AppConfig>>,
    pub cart_sessions: Arc<DashMap<String, CartSession>>,
    pub sync_handle: Arc<RwLock<Option<SyncHandle>>>,
    pub hub_handle: Arc<RwLock<Option<HubHandle>>>,
    pub mcp_server: Arc<McpServer>,              // NEW
}

pub fn run() {
    init_tracing();  // NEW — must be called first
    // ... rest of init
}
```

**Other infrastructure files** (errors/mod.rs, override_token.rs, commands/mod.rs, domain types, DB infrastructure) — same as prior spec Sections 13.8-13.10.

### 13.9 Additional Domain Types

Same as prior spec Section 13.9.

### 13.10 DB Infrastructure Files

Same as prior spec Section 13.10, plus:
- `src-tauri/src/db/fts.rs` — FTS5 search helper functions (NEW)

---

## 14. UI COMPONENTS & DESIGN SYSTEM

### 14.1 Design Tokens (Tailwind CSS v4)

Colors migrated from 7-theme CSS custom properties to Tailwind v4 `@theme` with oklch color space:

| Token | Dark (default) | Light |
|-------|---------------|-------|
| `--color-bg` | oklch(0.12 0.01 80) | oklch(0.95 0 0) |
| `--color-surface` | oklch(0.16 0.01 80) | oklch(1 0 0) |
| `--color-accent` | oklch(0.72 0.15 80) | oklch(0.65 0.14 70) |

**Fonts (unchanged):** DM Sans, Bricolage Grotesque, IBM Plex Mono, JetBrains Mono, Syncopate — all self-hosted woff2.

**7 themes:** via `data-theme` attribute. Tailwind dark mode variant handles light/dark, custom variants for midnight/forest/blossom/sky/sage.

**41+ @keyframes:** ported to Tailwind `animate-*` utilities.

### 14.2 Component Inventory

Unchanged from prior spec Section 14.2. Components progressively adopt Tailwind utility classes during Phase 2 migration.

### 14.3 CSS Methodology

**Target state:** Tailwind CSS v4 utility classes for 80%+ of styling. Remaining 20% in domain-split CSS files (complex animations, print styles, ESC/POS preview). No CSS modules, no CSS-in-JS.

---

## 15. REPORTING & ANALYTICS

Unchanged from prior spec Sections 15.1-15.3.

---

## 16. IMPORT/EXPORT & MIGRATION — UPGRADED

### 16.1 Receipt OCR → Purchase Order Flow — NEW

**Goal:** Scan a supplier invoice → OCR extracts text → AI structures into line items → Auto-create purchase order.

**Flow:**
```
1. Capture: User takes photo of supplier invoice (phone/webcam) or imports image file
2. Scan record: Creates supplier_invoice_scans row with status='scanned'
3. OCR: Sidecar /ocr endpoint (Tesseract.js LSTM) extracts raw text → status='ocr_done'
4. AI Structure: LLM parses raw OCR text into structured line items:
   - Product name, quantity, unit cost, total
   - Supplier identification (match to existing supplier)
   - Invoice number, date
   → status='structured'
5. Review: User reviews AI-structured items in a modal, can edit/correct
6. Match: System matches items to existing products by barcode, then name (FTS5 search)
   - Exact match → link to product_id
   - No match → create as new product row suggestion
7. Create PO: On user confirmation:
   - Creates purchase_order (status='ordered') linked to supplier
   - Creates purchase_order_items for each line
   - Updates supplier_invoice_scans with po_id → status='po_created'
   - Stores invoice image for audit trail
8. Receive: When goods arrive, user opens PO, confirms quantities → receive_stock
```

**Tesseract.js configuration for invoice OCR:**
```javascript
// Sidecar /ocr endpoint handles both payment screenshots AND invoices
const worker = await TesseractWorker.create({
  logger: m => pino.debug({ progress: m.progress }),
  OEM: 1,           // LSTM only
  lang: 'eng',
  // Invoice-specific: higher resolution, preserve layout
  tessedit_pageseg_mode: 3,  // Fully automatic page segmentation
});
```

**AI structuring prompt:**
```
You are given OCR text from a supplier invoice. Extract structured data:
- supplier_name: The company name
- invoice_number: Invoice reference number
- invoice_date: Date in YYYY-MM-DD format
- items: Array of { name, quantity (number), unit_cost (number in BHD) }
- total: Grand total in BHD

Raw OCR text:
{ocr_text}

Return JSON only.
```

### 16.2 Migration Agent

Unchanged from prior spec Section 16.1.

### 16.3 Catalog Import

Unchanged from prior spec Section 16.2. OCR step reuses same sidecar `/ocr` endpoint.

### 16.4 Bulk Product Import / CSV Export

Unchanged from prior spec Sections 16.3-16.4.

---

## 17. TESTING & QUALITY

### 17.1 Frontend Tests

Same 3 test files as prior spec Section 17.1. Additional tests needed:
- Zustand store integration tests (auth, cart, shift state transitions)
- Tailwind CSS v4 build verification (purge output size check)
- Biometric enrollment/verification flow tests

### 17.2 Rust Tests

Same as prior spec Section 17.2. Additional tests needed:
- MCP tool registration and dispatch tests
- cr-sqlite CRDT merge tests (two terminals → hub convergence)
- FTS5 search correctness tests (relevance ranking, prefix matching)
- tracing/log output verification tests

### 17.3 Vitest / Rust Audit

Unchanged from prior spec Sections 17.3-17.4.

---

## 18. CI/CD & DEVOPS

### 18.1 GitHub Actions (`.github/workflows/ci.yml`)

Same 4 jobs as prior spec Section 18.1. Build job additionally verifies:
- Tailwind CSS v4 production build (CSS size check < 20KB)
- cr-sqlite extension loads in test environment
- No unused CSS in production bundle (Tailwind purge audit)

### 18.2 Windows NSIS Installer / Auto-Updater

Unchanged from prior spec Sections 18.2-18.3.

---

## 19. CONSTANTS & CONFIGURATION REFERENCE

All prior constants (Sections 19.1-19.9) unchanged. Additions:

### 19.10 cr-sqlite Sync

| Constant | Value |
|----------|-------|
| CRR tables | 24 |
| Change batch size | 1000 |
| Clock precision | microseconds |

### 19.11 FTS5

| Constant | Value |
|----------|-------|
| Tokenizer | porter unicode61 |
| Ranking | BM25 |
| Search result limit | 50 |
| Min query length | 2 chars |

### 19.12 Biometric

| Constant | Value |
|----------|-------|
| Challenge TTL | 5 minutes |
| Max attempts | 3 |
| Supported methods | Windows Hello, Touch ID |

### 19.13 Tracing

| Constant | Value |
|----------|-------|
| Log format | JSON (file), pretty (stdout) |
| Rotation | Daily |
| Retention | 30 days |
| Default level | info |
| Sync level | trace |
| SQLx level | warn |

### 19.14 MCP

| Constant | Value |
|----------|-------|
| Protocol version | 2024-11-05 |
| Transport | stdio + HTTP |
| Resource URI scheme | zanpos:// |
| Max tool name length | 128 chars |

---

## 20. KNOWN ISSUES & TECHNICAL DEBT

### 20.1 Bug Markers in Source

Same as prior spec Section 20.1.

### 20.2 Dead Code

- `src-tauri/src/sync/mod.rs` — re-exports sync_v2 (dead code, kept for compat) — **can be removed** after cr-sqlite migration
- `src-tauri/src/sync/worker.rs` — re-exports sync_v2 (dead code, kept for compat) — **can be removed** after cr-sqlite migration
- `src-tauri/src/ai/tools_read_ext.rs` through `tools_write_ext3.rs` — 7 files **replaced by MCP registry** — remove after MCP migration complete
- `src-tauri/src/sync_v2/apply.rs` — merge strategies **replaced by cr-sqlite CRDT** — remove after cr-sqlite migration

### 20.3 Files Exceeding 500 Lines (Split Candidates)

Same as prior spec Section 20.3, with additions:

| File | Lines | Recommendation |
|------|-------|----------------|
| `src/App.css` | 15,456 | Tailwind CSS v4 migration to reduce to <2,000 lines |
| `src-tauri/src/ai/mcp_tools.rs` | (new) | Keep organized by domain sections; split at 1,500 lines |

### 20.4 Architectural Notes

- **REMOVED:** AI tools dispatch chain (7 files). Replaced by single MCP registry (`ai/mcp_server.rs` + `ai/mcp_tools.rs`).
- **REMOVED:** sync_v2 merge strategies (LWW, append-only, customer MAX merge, config allowlist). Replaced by cr-sqlite CRDT clock ordering.
- **REMOVED:** 19-table FK-safe push ordering. CRDT causality tracking handles dependencies automatically.
- **REMOVED:** sync conflict resolution code. cr-sqlite resolves conflicts deterministically via clock vectors.
- The monolithic `App.css` is being incrementally migrated to Tailwind CSS v4 — largest single refactor opportunity.
- `sync/mod.rs` and `sync/worker.rs` are dead code re-exports that should be removed once all callers use cr-sqlite sync directly.
- WhatsApp remains Baileys-only. No official WhatsApp Business API will be integrated.
