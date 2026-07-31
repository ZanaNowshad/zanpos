# ZANPOS — Complete Reproducibility Specification

> Generated from exhaustive line-by-line analysis of every source file in the repository.
> Target: Rebuild ZANPOS from scratch with 100% feature parity.
> Date: 2026-07-03

---

## 1. PROJECT OVERVIEW

ZANPOS is a local-first, offline-capable retail point-of-sale desktop application for Windows, built with Tauri v2 (Rust backend, React frontend). It targets Bahraini and Gulf retail businesses with BHD currency support (3 decimal places), thermal receipt printing (ESC/POS), WhatsApp notifications via Baileys sidecar, AI-powered business insights via multi-provider LLM integration, multi-terminal LAN sync via embedded hub server, and a full migration agent for importing data from legacy POS systems.

**Target platform:** Windows 10/11 (x64), with macOS/Linux build targets available but secondary.

**Core principles:**
1. Local-first — all operations work offline, sync when connected
2. Integer money — all monetary values stored as integer minor units (fils), zero floating-point
3. Server-side tax recalculation — client tax values are advisory; server always recomputes
4. Audit hash chain — every mutation produces a SHA-256 chained audit entry
5. Sequential receipt numbering — per-device atomic increment, never reused
6. Role-based access control — four roles (owner, manager, cashier, accountant) with database-backed permissions
7. Offline-capable sync — dual-mode (hub/terminal) with FK-safe push ordering and merge strategies

---

## 2. TECH STACK

### Frontend Dependencies

| Package | Version | Purpose |
|---|---|---|
| react | ^19.1.0 | UI framework |
| react-dom | ^19.1.0 | DOM renderer |
| @tauri-apps/api | ^2 | Tauri IPC (invoke, Channel) |
| @tauri-apps/plugin-dialog | ^2.7.1 | Native file dialogs |
| @tauri-apps/plugin-opener | ^2 | OS open-url integration |
| @tauri-apps/plugin-updater | ^2 | In-app updates |
| @tauri-apps/plugin-clipboard-manager | ^2 | Clipboard access |
| @tauri-apps/plugin-fs | ^2 | Filesystem access |
| @tauri-apps/plugin-process | ^2 | Process lifecycle |
| @tauri-apps/plugin-shell | ^2 | Shell command execution |
| lucide-react | ^1.16.0 | Icon library |
| jsbarcode | ^3.12.3 | CODE128 barcode generation |
| i18next / react-i18next | latest | Internationalization (EN/AR) |
| jspdf | latest | PDF receipt generation |
| html2canvas | latest | DOM-to-canvas for printing |
| dompurify | latest | HTML sanitization |
| marked | latest | Markdown rendering (AI chat) |

### Rust Crates (Cargo.toml)

| Crate | Version | Features | Purpose |
|---|---|---|---|
| tauri | 2 | — | Application framework |
| tauri-plugin-opener | 2 | — | OS opener |
| tauri-plugin-dialog | 2 | — | Native dialogs |
| tauri-plugin-updater | 2 | — | Auto-updater |
| serde / serde_json | 1 | derive | Serialization |
| sqlx | 0.8 | sqlite, runtime-tokio, macros, migrate | Async SQLite |
| tokio | 1 | full | Async runtime |
| ulid | 1 | serde | ULID generation |
| rust_decimal | 1 | — | Decimal arithmetic (inventory) |
| tracing / tracing-subscriber / tracing-appender | 0.1/0.3/0.2 | env-filter | Logging |
| chrono | 0.4 | serde | Date/time |
| reqwest | 0.12 | json, stream | HTTP client |
| futures | 0.3 | — | Async utilities |
| axum | 0.8 | http1, json, tokio, query | Hub HTTP server |
| argon2 | 0.5 | rand | PIN hashing |
| rand | 0.8 | getrandom | RNG |
| thiserror | 1 | — | Error derive |
| anyhow | 1 | — | Flexible errors |
| serialport | 4 | — | Serial/COM ports |
| sha2 | 0.10 | — | SHA-256 (audit chain) |
| hex | 0.4 | — | Hex encoding |
| printpdf | 0.7 | — | PDF receipt generation |
| base64 | 0.22 | — | Base64 codec |
| keyring | 2 | — | OS credential storage |
| mysql | 25 | — | MySQL (migration) |
| tiberius | 0.12 | tokio, native-tls | MSSQL (migration) |
| calamine | 0.24 | — | Excel reading |
| csv | 1.3 | — | CSV parsing |
| zip | 2 | — | ZIP extraction |
| tokio-util | 0.7 | compat | Tokio compat |
| winapi (Windows only) | 0.3 | winspool, minwindef, etc. | Windows printing |

**Profile:** `lto = "fat"`, `codegen-units = 1`, `strip = "symbols"`, `opt-level = "s"`

### Sidecar Runtime

- Node.js >= 20.0.0 (bundled as `node.exe`)
- @whiskeysockets/baileys 7.0.0-rc13
- express ^4.19.2, pino ^9.0.0, qrcode ^1.5.4, tesseract.js ^5.1.1

### CI Platform

GitHub Actions, `windows-latest` and `ubuntu-latest` runners. Four jobs: `rust` (fmt + clippy + test + cargo-audit), `frontend` (tsc + eslint + vitest + npm audit), `build` (Tauri debug build, depends on rust+frontend), `release-gate` (summary, main/master only, depends on all three).

---

## 3. FRONTEND ARCHITECTURE

### Entry Point

`index.html` loads `src/main.tsx` which renders `<App />` inside `<React.StrictMode>` via `React 19 createRoot`.

### App Component (src/App.tsx)

The app uses a **manual view state machine** — no router library. `type View = "login" | "shift_check" | "shift_open" | "pos" | "office_ai"`. Navigation between views is handled by `useState<View>`.

**Tab names in OfficeAI workspace (13 tabs across 5 sections):**

| Section | Tabs |
|---|---|
| Assistant | assistant |
| Products | products, categories, deliveries |
| Reports | reports, cashier, eod |
| Inventory | inventory |
| Customers | customers |
| System | settings, audit, devices, users |

**Visibility rules:** Owner sees all 13. Manager sees all except audit/devices/users. Cashier sees only assistant/products/categories/deliveries.

### Styling Approach

Single large CSS file (`src/App.css`, ~15,500 lines). 7 themes controlled by `data-theme` attribute on `<html>`: dark, light, midnight, blossom, forest, sky, sage. Each theme sets ~15 CSS custom properties (--bg, --bg-card, --text, --text-muted, --border, --primary, --primary-hover, --accent, --danger, --success, --warning, etc.).

**Custom property categories:**
- Theme colors (per-theme overrides)
- Typography: `--font-display` (Syncopate 700), `--font-heading` (Bricolage Grotesque 400-800), `--font-body` (DM Sans 300-800), `--font-mono` (IBM Plex Mono 400-700), `--font-code` (JetBrains Mono 400-500)
- Z-index layers: `--z-dropdown` (100), `--z-sticky` (200), `--z-nav` (300), `--z-dock` (400), `--z-modal-backdrop` (500), `--z-modal` (600), `--z-popover` (700), `--z-toast` (800), `--z-top` (900)
- Transitions: `--ease-out` (cubic-bezier), `--ease-spring` (spring), `--dur-enter` (150ms), `--dur-flourish` (400ms), `--stagger-step` (50ms)
- Touch targets: `--touch-sm` (32px) to `--touch-xl` (56px)
- Button sizes: `--btn-sm` (28px) to `--btn-xl` (48px)
- Input: `--input-height` (40px)

45+ keyframe animations with `prefers-reduced-motion` support.

### Key Utilities

**currency.ts** (`src/services/currency.ts`): `parseMoney(amount: string, exponent: number): number` — parses decimal string to integer minor units. `formatMoney(minor: number, exponent: number): string` — formats integer to decimal string with correct decimal places. BHD exponent = 3.

**money.ts (Rust)** — see Section 9.

### Component Inventory (by chunk)

**vendor chunk:** react, react-dom, react-dom/client
**icons chunk:** lucide-react
**jsbarcode chunk:** jsbarcode
**tauri-api chunk:** @tauri-apps/* packages
**setup-wizard chunk:** SetupWizard page, setup/ components
**migration chunk:** MigrationAgentPage
**officeai chunk:** All officeai/ components, ChatPanel, KpiSidebar, CopilotDock, RunPanel, etc.
**backoffice chunk:** ProductsTab, CategoriesTab, ProductFormModal, BulkStockTakeModal, BarcodesPrintModal, ReportsTab, CashierReportTab, EodCashupTab, TodayReportModal, XReportModal, RecentSalesModal, UsersTab, SettingsTab, AuditLogTab, CustomersTab, DevicesTab, InventoryTab, SyncQueueModal, DeliveriesTab, DeliveryForm, RefundModal, HoldModal, DiscountModal, LineDiscountModal, LineEditModal, CustomItemModal, PriceInputModal, PaymentModal, ReceiptPreview

### Test Specification

**Framework:** vitest v2 with @vitest/coverage-v8
**Coverage thresholds:** 80% lines, 80% branches, 80% functions
**Test files:**
- `src/__tests__/money.test.ts` — formatMoney, parseMoney: BHD (exp=3), USD (exp=2), exp=0, negatives, invalid input, round-trip invariance
- `src/__tests__/posProductFilters.test.ts` — filterProductsForSale, productIsOutOfStock, productIsLowStock
- `src/__tests__/adminChatClear.test.ts` — clearAdminChat edge cases

---

## 4. RUST BACKEND MODULE MAP

```
src-tauri/src/
├── main.rs                          — Binary entry point, calls zanpos_lib::run()
├── lib.rs                           — App setup: logging, DB pool, hub, sidecar, sync worker,
│                                      proactive loop, Tauri builder with all commands, Windows Job Object
├── errors/mod.rs                    — AppError enum (NotFound, Validation, Permission, Conflict,
│                                      Database, Internal, Serde, Io), AppResult<T> alias
├── secure_store.rs                  — OS credential store wrapper (keyring crate, service="zanpos")
├── hub/
│   ├── mod.rs                       — HubState, HubHandle, token_digest (SHA-256), lan_ips(),
│   │                                  start_hub(pool, port, token)
│   └── rest.rs                      — 4 axum routes: probe_ok, info, pull_table, push_table
├── sync_v2/
│   ├── mod.rs                       — Re-exports SyncWorker, declares apply/client/worker modules
│   ├── worker.rs                    — SyncWorker: spawn loop, push_pending (FK-safe order, 19 tables),
│   │                                  pull_changes (watermark + offset tracking), prune_old_data
│   ├── apply.rs                     — apply_row dispatcher: LWW merge, append-only, customer merge,
│   │                                  app_config allowlist, per-table pre-processing, recompute_stock_level
│   └── client.rs                    — HttpSyncClient: upsert_rows (POST), pull_rows (GET with filters),
│                                      pull_branch, hub_info
├── db/
│   ├── mod.rs                       — init_db(path): SqlitePoolOptions (max_conn=6, acquire_timeout=30s),
│   │                                  WAL mode, FK enforcement, 15s busy_timeout, sqlx::migrate!
│   ├── helpers.rs                   — active_branch_id(), active_device_id()
│   └── repositories/
│       ├── mod.rs                   — 14 module declarations
│       ├── sale_repo.rs             — finalize_sale (15-step transaction), next_receipt_number
│       ├── refund_repo.rs           — create_refund (BEGIN IMMEDIATE, per-item ceiling, double-refund guard)
│       ├── held_cart_repo.rs        — save/list/resume/delete held carts
│       ├── product_repo.rs          — PRODUCT_QUERY, search_products, get_by_barcode, list_all_active
│       ├── product_dedup_repo.rs    — find_duplicate_groups, merge_products, soft_delete_product
│       ├── shift_repo.rs            — open_shift, close_shift (complex expected-cash formula)
│       ├── delivery_repo.rs         — create/list/get/update_status/confirm_payment/cancel/revert
│       ├── report_repo.rs           — today_summary (5 concurrent aggregation queries)
│       ├── auth_repo.rs             — hash_pin (Argon2id), verify_pin, login_pin (lockout logic),
│       │                              rehash_plain_pins
│       ├── audit_hash.rs            — compute_audit_hash (SHA-256, 12 NUL-separated fields),
│       │                              insert_audit_entry, fetch_last_hash, verify_chain
│       ├── sync_repo.rs             — get_sync_status (per-table pending counts)
│       ├── ai_admin_repo.rs         — get/set_config, create/get/execute action, undo records,
│       │                              sessions, usage log, cleanup
│       ├── ai_chat_history_repo.rs  — save_message, load_history (subquery: DESC then ASC),
│       │                              clear_history
│       └── proactive_repo.rs        — insert_alert, has_active, list_undismissed, dismiss,
│                                      get/set_watermark (ON CONFLICT upsert)
├── domain/
│   ├── mod.rs                       — 10 module declarations
│   ├── cart.rs                      — Cart, CartLine (14 fields), add/remove/recalculate/validate
│   ├── sale.rs                      — PaymentInput, SaleResult, PaymentSummary, SaleItemSummary
│   ├── refund.rs                    — SaleForRefund, RefundItemInput, RefundResult, HeldCartSummary
│   ├── money.rs                     — mul_minor_by_qty, parse_major_to_minor, format_minor,
│   │                                  apply_discount_bp, calc_tax_exclusive
│   ├── product.rs                   — Product, ProductWithPrice, LowStockAlert, TaxRule
│   ├── delivery.rs                  — DeliveryInput, DeliveryRow, DeliveryListFilter, etc.
│   ├── shift.rs                     — Shift struct
│   ├── report.rs                    — TodaySummary
│   ├── auth.rs                      — UserSummary, SessionUser
│   └── ai_admin.rs                  — StreamEvent enum (16 variants), ProviderConfig, FeatureToggles,
│                                      ProactiveAlert, AiSession, AiChatInput, etc.
├── ai/
│   ├── mod.rs                       — 18 sub-module declarations
│   ├── client.rs                    — AnthropicClient: send() to /v1/messages, SSE parsing
│   ├── provider.rs                  — Provider enum (Anthropic/OpenAI/Gemini), send_chat(),
│   │                                  continue_with_tool_turns(), truncate_history()
│   ├── openai_client.rs             — OpenAIClient: send_chat(), send_stream() with callbacks,
│   │                                  parse_xml_tool_calls() fallback
│   ├── streaming.rs                 — run_streaming_chat(), run_streaming_chat_openai(),
│   │                                  auto_apply_mutation(), engine_op_bypass_list()
│   ├── tools.rs                     — all_tool_definitions(), execute_read_tool(), dry_run_mutation(),
│   │                                  execute_mutation(), execute_undo(), resolve_mutation_risk(),
│   │                                  filtered_tool_definitions()
│   ├── tools_read_ext.rs            — 15 extended read tools
│   ├── tools_read_ext2.rs           — ~55 analytics tools
│   ├── tools_read_ext3.rs           — ~25 supplier/inventory tools
│   ├── tools_write_ext3.rs          — 6 extended mutation tools
│   ├── intent_engine.rs             — 20 deterministic intents, execute_intent()
│   ├── oauth.rs                     — openai()/google() OAuthConfig PKCE flows
│   └── engine/
│       ├── mod.rs                   — PriceOp enum (Percent/Absolute/Set), apply_price()
│       ├── ops.rs                   — Operation trait (5 methods), 8 implementations, Registry
│       ├── batch.rs                 — execute_price_adjust(), execute_op() keyset pagination, undo_run()
│       ├── selector.rs              — Selector (category/text/supplier/below_reorder filters),
│       │                              compile() to CTE+SQL, count(), needs_stock_join()
│       └── runs.rs                  — create_run(), set_status(), set_status_guarded(),
│                                      set_cancelled(), status_only(), get_run()
└── commands/
    ├── mod.rs                       — 29 module declarations
    ├── admin_commands.rs            — 23 commands: full CRUD for products/categories/tax_rules/users
    ├── ai_admin_commands.rs         — 17 commands: provider config, feature toggles, ai_chat (streaming),
    │                                  execute/batch/undo/cancel, bulk engine, usage, feedback, kill
    ├── auth_commands.rs             — 4 commands: list_users, login_pin, verify_owner_pin,
    │                                  validate_manager_pin (60s override token)
    ├── cash_commands.rs             — 5 commands: cash_event_create, list, drawer_summary,
    │                                  no_sale, x_report
    ├── catalog_import_commands.rs   — 2 commands: extract (OCR+AI), apply
    ├── customer_commands.rs         — 5 commands: list, create, update, get, add_loyalty
    ├── delivery_commands.rs         — 7 commands: list, get, update_status, confirm_payment,
    │                                  cancel, revert_payment, rider_suggestions
    ├── device_commands.rs           — 3 commands: list, create, toggle_active
    ├── ghost_barcode_commands.rs    — 6 commands: record, summary, list, dismiss, prefill, resolve
    ├── held_cart_commands.rs        — 4 commands: save, list, resume, delete
    ├── hub_commands.rs              — 7 commands: status, enable, regenerate_token, test_connection,
    │                                  join, connect_existing, set_url
    ├── inventory_commands.rs        — 7 commands: get_levels, get_levels_paged, get_low_stock,
    │                                  get_movements, receive_stock, adjust_stock, bulk_stock_take
    ├── migration_commands.rs        — 13 commands: inspect, ai_map, execute, connect_test,
    │                                  list_tables, query_remote, list_processes, find_db_files,
    │                                  read_file, decompress, zanpos_stats, rollback, agent_chat
    ├── override_token.rs            — store_override_token() (60s TTL), consume_override_token()
    ├── payment_confirm_commands.rs  — 4 commands: list, unseen_count, mark_all_seen, override;
    │                                  record_pending(), run_verification()
    ├── phase10a_commands.rs         — 8 commands: timeout config, db_backup, tax_by_day, audit_log,
    │                                  audit_verify_chain, reports_config load/save
    ├── pos_commands.rs              — 17 commands: cart operations, add/remove/update_quantity,
    │                                  scan_barcode, set_discount, add_custom_item, finalize_sale,
    │                                  print_receipt, void_sale, load_sale
    ├── product_commands.rs          — 3 commands: search (cursor-paginated), get_by_barcode, list_all
    ├── rbac.rs                      — owner_only(), manager_or_owner(), require_any_role(),
    │                                  can_override_refund()
    ├── receipt_pdf.rs               — generate_receipt_pdf() — A5 148x210mm, Helvetica, ASCII-only
    ├── refund_commands.rs           — 3 commands: get_sale, receipt_reprint, create (cross-device policy)
    ├── report_commands.rs           — 10 commands: today, date_range, top_products, sales_list,
    │                                  by_cashier, eod_cashup, z_report, db_integrity, reports_config
    ├── setup_commands.rs            — 9 commands: app_config_load, setup_wizard_complete,
    │                                  settings get/update, benefit_number, business_flags,
    │                                  operational_settings
    ├── shift_commands.rs            — 3 commands: get_active, open, close
    ├── sync_commands.rs             — 11 commands: status, trigger_now, bulk_initial, setup_pull_catalog,
    │                                  force_full_resync, reset_stuck, queue_list/retry/dismiss,
    │                                  queue_stats, diagnostics
    ├── thermal_commands.rs          — 6 commands: list_ports (PowerShell+serialport, 30s cache),
    │                                  get/set config, print_test, print_receipt_raw, open_cash_drawer
    ├── updater_commands.rs          — 3 commands: check_for_updates, download_and_install, product_pick_image
    ├── whatsapp_commands.rs         — 9 commands: status, send_delivery (PDF), disconnect, save_config,
    │                                  notify_arrival, payment_reminder, import_contacts, send_receipt_pdf
    └── whatsapp_inbox_commands.rs   — 10 commands: list_contacts, list_groups, set/get_targets,
                                       poll_messages, list_messages, get_media, mark_read,
                                       mark_all_read, clear_messages
```

---

## 5. DATABASE SCHEMA

### Connection Configuration

- **Engine:** SQLite via sqlx 0.8, WAL journal mode, foreign_keys=ON, synchronous=NORMAL
- **Pool:** max_connections=6, acquire_timeout=30s
- **Per-connection:** busy_timeout=15s, cache_size=-8000 (8MB), wal_autocheckpoint=1000
- **Migration:** 21 SQL files in `src-tauri/migrations/` (0001 through 0021), run by `sqlx::migrate!`
- **Post-migration reset:** 19 tables have stuck `sync_status='pending'` rows reset (sync_attempts=0, empty updated_at backfilled)

### CORE TABLES

#### branches
| Column | Type | Null | Default | Notes |
|---|---|---|---|---|
| branch_id | TEXT | NOT NULL | | PK |
| branch_code | TEXT | NOT NULL | | UNIQUE INDEX (partial, is_active) |
| name | TEXT | NOT NULL | | |
| currency | TEXT | NOT NULL | 'BHD' | |
| timezone | TEXT | NOT NULL | 'Asia/Bahrain' | |
| address | TEXT | YES | | |
| phone | TEXT | YES | | |
| receipt_header | TEXT | YES | | |
| receipt_footer | TEXT | YES | | |
| tax_number | TEXT | YES | | |
| cr_number | TEXT | YES | | |
| is_active | INTEGER | NOT NULL | 1 | |
| created_at | TEXT | NOT NULL | | |
| updated_at | TEXT | NOT NULL | | |
| deleted_at | TEXT | YES | | |
| version | INTEGER | NOT NULL | 1 | |
| sync_status | TEXT | NOT NULL | 'pending' | |
| sync_attempts | INTEGER | NOT NULL | 0 | |

#### roles
| Column | Type | Null | Default | Notes |
|---|---|---|---|---|
| role_id | TEXT | NOT NULL | | PK |
| name | TEXT | NOT NULL | | UNIQUE |
| created_at | TEXT | NOT NULL | | |
| updated_at | TEXT | NOT NULL | '' | |
| sync_status | TEXT | NOT NULL | 'pending' | |
| sync_attempts | INTEGER | NOT NULL | 0 | |

Seed data: owner, manager, cashier (hardcoded ULIDs).

#### devices
| Column | Type | Null | Default | Notes |
|---|---|---|---|---|
| device_id | TEXT | NOT NULL | | PK |
| branch_id | TEXT | NOT NULL | | FK→branches |
| device_code | TEXT | NOT NULL | | UNIQUE(branch_id, device_code) |
| name | TEXT | NOT NULL | | |
| status | TEXT | NOT NULL | 'online' | |
| is_active | INTEGER | NOT NULL | 1 | |
| next_receipt_seq | INTEGER | NOT NULL | 1 | Atomic receipt counter |
| last_seen_at | TEXT | YES | | |
| created_at | TEXT | NOT NULL | | |
| updated_at | TEXT | NOT NULL | | |
| deleted_at | TEXT | YES | | |
| version | INTEGER | NOT NULL | 1 | |
| sync_status | TEXT | NOT NULL | 'pending' | |
| sync_attempts | INTEGER | NOT NULL | 0 | |

#### users
| Column | Type | Null | Default | Notes |
|---|---|---|---|---|
| user_id | TEXT | NOT NULL | | PK |
| branch_id | TEXT | NOT NULL | | |
| display_name | TEXT | NOT NULL | | |
| username | TEXT | NOT NULL | | UNIQUE |
| pin_hash | TEXT | NOT NULL | | Argon2id PHC string |
| role_id | TEXT | NOT NULL | | FK→roles |
| branch_scope | TEXT | NOT NULL | '[]' | |
| is_active | INTEGER | NOT NULL | 1 | |
| failed_pin_attempts | INTEGER | NOT NULL | 0 | |
| locked_until | TEXT | YES | | RFC 3339 |
| last_login_at | TEXT | YES | | |
| created_at | TEXT | NOT NULL | | |
| updated_at | TEXT | NOT NULL | | |
| deleted_at | TEXT | YES | | |
| version | INTEGER | NOT NULL | 1 | |
| sync_status | TEXT | NOT NULL | 'pending' | |
| sync_attempts | INTEGER | NOT NULL | 0 | |

Excluded from sync: pin_hash, failed_pin_attempts, locked_until, last_login_at.

#### products
| Column | Type | Null | Default | Notes |
|---|---|---|---|---|
| product_id | TEXT | NOT NULL | | PK |
| category_id | TEXT | NOT NULL | | FK→categories |
| name | TEXT | NOT NULL | | |
| sku | TEXT | YES | | |
| barcode | TEXT | YES | | |
| description | TEXT | YES | | |
| track_inventory | INTEGER | NOT NULL | 1 | |
| allow_decimal_quantity | INTEGER | NOT NULL | 0 | |
| is_active | INTEGER | NOT NULL | 1 | |
| tax_rule_id | TEXT | YES | | FK→tax_rules |
| cost_minor | INTEGER | YES | | |
| currency | TEXT | NOT NULL | 'BHD' | |
| reorder_point | INTEGER | NOT NULL | 0 | |
| image_path | TEXT | YES | | |
| default_supplier_id | TEXT | YES | | |
| created_at | TEXT | NOT NULL | | |
| updated_at | TEXT | NOT NULL | | |
| deleted_at | TEXT | YES | | Soft delete |
| version | INTEGER | NOT NULL | 1 | |
| sync_status | TEXT | NOT NULL | 'pending' | |
| sync_attempts | INTEGER | NOT NULL | 0 | |

Indexes: category_id, barcode, name, sku, sync_status, (is_active, deleted_at).

#### product_prices
| Column | Type | Null | Default | Notes |
|---|---|---|---|---|
| price_id | TEXT | NOT NULL | | PK |
| product_id | TEXT | NOT NULL | | FK→products |
| branch_id | TEXT | YES | | NULL = global price |
| price_type | TEXT | NOT NULL | 'selling' | |
| price_minor | INTEGER | NOT NULL | | |
| currency | TEXT | NOT NULL | 'BHD' | |
| effective_from | TEXT | NOT NULL | | |
| effective_to | TEXT | YES | | NULL = currently active |
| created_by_user_id | TEXT | NOT NULL | | |
| created_by_ai_action_id | TEXT | YES | | |
| created_at | TEXT | NOT NULL | | |
| updated_at | TEXT | NOT NULL | | |
| sync_status | TEXT | NOT NULL | 'pending' | |
| sync_attempts | INTEGER | NOT NULL | 0 | |

#### categories
| Column | Type | Null | Default | Notes |
|---|---|---|---|---|
| category_id | TEXT | NOT NULL | | PK |
| parent_category_id | TEXT | YES | | |
| name | TEXT | NOT NULL | | |
| sort_order | INTEGER | NOT NULL | 0 | |
| is_active | INTEGER | NOT NULL | 1 | |
| + standard timestamp/version/sync columns | | | | |

#### tax_rules
| Column | Type | Null | Default | Notes |
|---|---|---|---|---|
| tax_rule_id | TEXT | NOT NULL | | PK |
| name | TEXT | NOT NULL | | |
| rate_basis_points | INTEGER | NOT NULL | 0 | 1000 = 10% |
| inclusive | INTEGER | NOT NULL | 0 | 0=exclusive, 1=inclusive |
| is_active | INTEGER | NOT NULL | 1 | |
| effective_from | TEXT | NOT NULL | | |
| effective_to | TEXT | YES | | |
| + standard timestamp/version/sync columns | | | | |

Seed: VAT 10% exclusive (1000 bp), Zero-rated (0 bp).

#### stock_levels
| Column | Type | Null | Default | Notes |
|---|---|---|---|---|
| stock_level_id | TEXT | NOT NULL | | PK |
| product_id | TEXT | NOT NULL | | FK→products |
| branch_id | TEXT | NOT NULL | | |
| quantity_on_hand | TEXT | NOT NULL | '0' | Stored as TEXT for precision |
| last_movement_at | TEXT | YES | | |
| + standard timestamp/sync columns | | | | |

UNIQUE(product_id, branch_id).

#### stock_movements
| Column | Type | Null | Default | Notes |
|---|---|---|---|---|
| movement_id | TEXT | NOT NULL | | PK |
| product_id | TEXT | NOT NULL | | FK→products |
| branch_id | TEXT | NOT NULL | | |
| device_id | TEXT | NOT NULL | | |
| origin_device_id | TEXT | NOT NULL | '' | |
| movement_type | TEXT | NOT NULL | | sale/refund/void/adjustment/stock_take/receive/manual_adjust |
| quantity_delta | TEXT | NOT NULL | | |
| quantity_after | TEXT | NOT NULL | | |
| reference_type | TEXT | YES | | |
| reference_id | TEXT | YES | | |
| notes | TEXT | YES | | |
| created_by_user_id | TEXT | YES | | |
| + standard timestamp/sync columns | | | | |

#### sales
| Column | Type | Null | Default | Notes |
|---|---|---|---|---|
| sale_id | TEXT | NOT NULL | | PK |
| receipt_number | TEXT | NOT NULL | | UNIQUE |
| branch_id | TEXT | NOT NULL | | |
| device_id | TEXT | NOT NULL | | |
| origin_device_id | TEXT | NOT NULL | '' | |
| shift_id | TEXT | NOT NULL | | |
| cashier_user_id | TEXT | NOT NULL | | |
| status | TEXT | NOT NULL | 'completed' | completed/voided/refunded/partially_refunded |
| gross_total_minor | INTEGER | NOT NULL | 0 | |
| discount_total_minor | INTEGER | NOT NULL | 0 | |
| tax_total_minor | INTEGER | NOT NULL | 0 | |
| net_total_minor | INTEGER | NOT NULL | 0 | |
| currency | TEXT | NOT NULL | 'BHD' | |
| business_date | TEXT | NOT NULL | | |
| sold_at | TEXT | NOT NULL | | |
| created_offline | INTEGER | NOT NULL | 0 | |
| idempotency_key | TEXT | NOT NULL | | UNIQUE |
| customer_id | TEXT | YES | | |
| is_delivery | INTEGER | NOT NULL | 0 | |
| + standard timestamp/sync columns | | | | |

Indexes: shift_id, business_date, receipt_number, sync_status, customer_id, status, updated_at.

#### sale_items
| Column | Type | Null | Default | Notes |
|---|---|---|---|---|
| sale_item_id | TEXT | NOT NULL | | PK |
| sale_id | TEXT | NOT NULL | | FK→sales |
| product_id | TEXT | YES | | NULL for custom items |
| product_name_snapshot | TEXT | NOT NULL | | |
| sku_snapshot | TEXT | YES | | |
| barcode_snapshot | TEXT | YES | | |
| quantity | TEXT | NOT NULL | | |
| unit_price_minor | INTEGER | NOT NULL | | |
| line_discount_minor | INTEGER | NOT NULL | 0 | |
| tax_rule_snapshot | TEXT | NOT NULL | '{}' | |
| tax_amount_minor | INTEGER | NOT NULL | 0 | Server-computed |
| line_total_minor | INTEGER | NOT NULL | 0 | Server-computed |
| note | TEXT | YES | | |
| voided | INTEGER | NOT NULL | 0 | |
| refunded_amount_minor | INTEGER | NOT NULL | 0 | |
| origin_device_id | TEXT | NOT NULL | '' | |
| + standard timestamp/sync columns | | | | |

#### payments
| Column | Type | Null | Default | Notes |
|---|---|---|---|---|
| payment_id | TEXT | NOT NULL | | PK |
| sale_id | TEXT | NOT NULL | | FK→sales |
| origin_device_id | TEXT | NOT NULL | '' | |
| payment_method | TEXT | NOT NULL | | CHECK IN ('cash','card','wallet','other') |
| amount_minor | INTEGER | NOT NULL | | |
| currency | TEXT | NOT NULL | 'BHD' | |
| status | TEXT | NOT NULL | 'approved' | |
| external_reference | TEXT | YES | | |
| tendered_minor | INTEGER | YES | | Cash only |
| change_minor | INTEGER | YES | | Cash only |
| recorded_by_user_id | TEXT | NOT NULL | | |
| recorded_at | TEXT | NOT NULL | | |
| + standard timestamp/sync columns | | | | |

#### refunds
| Column | Type | Null | Default | Notes |
|---|---|---|---|---|
| refund_id | TEXT | NOT NULL | | PK |
| original_sale_id | TEXT | NOT NULL | | FK→sales |
| origin_device_id | TEXT | NOT NULL | '' | |
| refund_receipt_number | TEXT | NOT NULL | | UNIQUE |
| reason | TEXT | NOT NULL | '' | |
| return_reason_code | TEXT | NOT NULL | 'other' | customer_return/defective/wrong_item/exchange/other |
| refund_total_minor | INTEGER | NOT NULL | | |
| currency | TEXT | NOT NULL | 'BHD' | |
| created_by_user_id | TEXT | NOT NULL | | |
| idempotency_key | TEXT | NOT NULL | | UNIQUE |
| + standard timestamp/sync columns | | | | |

#### refund_items
| Column | Type | Null | Default |
|---|---|---|---|
| refund_item_id | TEXT | NOT NULL | PK |
| refund_id | TEXT | NOT NULL | FK→refunds |
| origin_device_id | TEXT | NOT NULL | '' |
| sale_item_id | TEXT | NOT NULL | |
| product_name_snapshot | TEXT | NOT NULL | |
| quantity | TEXT | NOT NULL | |
| unit_price_minor | INTEGER | NOT NULL | 0 |
| refund_amount_minor | INTEGER | NOT NULL | |
| + standard timestamp/sync columns | | | |

#### customers
| Column | Type | Null | Default | Notes |
|---|---|---|---|---|
| customer_id | TEXT | NOT NULL | | PK |
| branch_id | TEXT | NOT NULL | | |
| origin_device_id | TEXT | NOT NULL | '' | |
| name | TEXT | NOT NULL | | |
| phone | TEXT | YES | | UNIQUE partial WHERE NOT NULL |
| email | TEXT | YES | | |
| loyalty_points | INTEGER | NOT NULL | 0 | |
| notes | TEXT | YES | | |
| + standard timestamp/version/sync columns | | | | |

#### delivery_orders
| Column | Type | Null | Default | Notes |
|---|---|---|---|---|
| delivery_id | TEXT | NOT NULL | | PK |
| sale_id | TEXT | NOT NULL | | FK→sales |
| receipt_number | TEXT | NOT NULL | | |
| branch_id | TEXT | NOT NULL | | |
| device_id | TEXT | NOT NULL | | |
| origin_device_id | TEXT | NOT NULL | '' | |
| customer_id | TEXT | YES | | |
| customer_name | TEXT | YES | | |
| contact_number | TEXT | NOT NULL | | E.164 |
| address_text | TEXT | YES | | |
| house_number | TEXT | YES | | |
| area | TEXT | YES | | |
| delivery_status | TEXT | NOT NULL | 'pending' | pending/dispatched/out_for_delivery/delivered/cancelled |
| delivery_staff_name | TEXT | YES | | |
| delivery_note | TEXT | YES | | |
| expected_payment_method | TEXT | NOT NULL | 'cash' | cash/card/wallet |
| payment_status | TEXT | NOT NULL | 'unpaid' | unpaid/paid/cancelled |
| amount_minor | INTEGER | NOT NULL | 0 | |
| currency | TEXT | NOT NULL | 'BHD' | |
| paid_confirmed_by_user_id | TEXT | YES | | |
| paid_confirmed_at | TEXT | YES | | |
| payment_reference | TEXT | YES | | |
| payment_note | TEXT | YES | | |
| created_by_user_id | TEXT | YES | | |
| + standard timestamp/version/sync columns | | | | |

#### shifts
| Column | Type | Null | Default | Notes |
|---|---|---|---|---|
| shift_id | TEXT | NOT NULL | | PK |
| branch_id | TEXT | NOT NULL | | |
| device_id | TEXT | NOT NULL | | |
| origin_device_id | TEXT | NOT NULL | '' | |
| cashier_user_id | TEXT | NOT NULL | | |
| opened_at | TEXT | NOT NULL | | |
| closed_at | TEXT | YES | | |
| opening_cash_minor | INTEGER | NOT NULL | 0 | |
| counted_cash_minor | INTEGER | YES | | |
| expected_cash_minor | INTEGER | YES | | |
| cash_difference_minor | INTEGER | YES | | |
| business_date | TEXT | YES | | |
| status | TEXT | NOT NULL | 'open' | |
| close_notes | TEXT | YES | | |
| + standard timestamp/version/sync columns | | | | |

UNIQUE partial INDEX on (device_id) WHERE status='open'.

#### cash_events
| Column | Type | Null | Default | Notes |
|---|---|---|---|---|
| cash_event_id | TEXT | NOT NULL | | PK |
| shift_id | TEXT | NOT NULL | | FK→shifts |
| branch_id | TEXT | YES | | |
| device_id | TEXT | YES | | |
| origin_device_id | TEXT | NOT NULL | '' | |
| event_type | TEXT | NOT NULL | | paid_in/paid_out/safe_drop |
| amount_minor | INTEGER | YES | | |
| note | TEXT | YES | | |
| created_by_user_id | TEXT | NOT NULL | | |
| + standard timestamp/sync columns | | | | |

#### audit_logs
| Column | Type | Null | Default | Notes |
|---|---|---|---|---|
| audit_log_id | TEXT | NOT NULL | | PK |
| event_type | TEXT | NOT NULL | | sale.created/refund.created/shift.closed/etc. |
| entity_type | TEXT | NOT NULL | | sale/refund/shift/product/etc. |
| entity_id | TEXT | NOT NULL | | |
| actor_user_id | TEXT | YES | | |
| actor_type | TEXT | NOT NULL | 'user' | user/ai_agent |
| ai_action_id | TEXT | YES | | |
| device_id | TEXT | YES | | |
| origin_device_id | TEXT | NOT NULL | '' | |
| branch_id | TEXT | YES | | |
| before_json | TEXT | YES | | |
| after_json | TEXT | YES | | |
| reason | TEXT | YES | | |
| created_at | TEXT | NOT NULL | | |
| hash | TEXT | NOT NULL | | SHA-256, 64 hex chars |
| previous_hash | TEXT | YES | | Chain link |
| override_used | INTEGER | NOT NULL | 0 | |
| + standard timestamp/sync columns | | | | |

#### product_barcodes
| Column | Type | Null | Default |
|---|---|---|---|
| barcode_id | TEXT | YES | |
| product_id | TEXT | NOT NULL | FK→products |
| barcode | TEXT | NOT NULL | UNIQUE |
| created_at | TEXT | NOT NULL | '' |

PK: (product_id, barcode). NOT synced.

### LOCAL-ONLY TABLES (never synced)

#### held_carts
held_cart_id (PK), branch_id, device_id, shift_id, cashier_user_id, cart_json (TEXT, full Cart serialized), note, held_at.

#### app_config
key (PK), value (TEXT, default ''), updated_at. Key-value store for all application settings.

#### unknown_barcodes (ghost_barcodes)
id (PK), barcode, scan_count (INTEGER, 1), first_seen_at, last_seen_at, recorded_by_user_id, status ('pending'/'found'/'not_found'/'dismissed'), product_name, product_id, resolved_by_user_id, resolved_at, brand, category, image_url, raw_json. UNIQUE INDEX on barcode.

#### no_sale_events
no_sale_id (PK), shift_id, branch_id, device_id, actor_user_id, note, created_at.

#### sync_watermark
table_name (PK), last_pulled_at (DEFAULT '1970-01-01T00:00:00Z'), last_pushed_at (DEFAULT '1970-01-01T00:00:00Z'). Seeded with 20 table entries.

#### import_history
import_id (PK), source_path, rows_imported, created_by_user_id, created_at.

### AI TABLES

#### ai_actions
action_id (PK), session_user_id, actor_type ('ai'), tool_name, tool_input_json, tool_input_hash, preview_text, status, confirmation_token, prepared_at, confirmed_at, executed_at, expires_at, result_json, error_message.

#### undo_records
undo_id (PK), action_id, before_json, entity_type, entity_id, snapshot_json, rollback_tool, rollback_input_json, status ('available'/'undone'), undone_at, undone_by_user_id, created_at.

#### ai_runs
run_id (PK), op_id, selector_json, params_json, status, total_count, done_count, checkpoint_cursor, error, created_by, created_at, updated_at. Index: idx_ai_runs_status.

#### ai_run_undo_log
entry_id (PK), run_id (FK→ai_runs), batch_seq, reverse_json, applied (0), created_at. Indexes: run_id, (run_id, batch_seq).

#### ai_sessions
session_id (PK), branch_id, user_id, provider, model, status (CHECK 'active'/'ended'/'error'), total_turns (0), tokens_in (0), tokens_out (0), cost_estimate_usd (0.0), total_latency_ms (0), started_at, ended_at.

#### ai_usage_log
id (PK AUTOINCREMENT), session_id (FK→ai_sessions), turn, tokens_in (0), tokens_out (0), latency_ms (0), provider, model, logged_at.

#### ai_feedback
feedback_id (PK), session_id, user_id, message_id, rating (CHECK 'up'/'down'), comment, created_at.

#### ai_reasoning_log
id (PK AUTOINCREMENT), session_id, branch_id, user_id, turn, tool_name, reasoning, logged_at.

#### ai_chat_messages
id (PK AUTOINCREMENT, inferred), session_id, branch_id, user_id, role, content, message_type, created_at.

Note: Migration 0001 creates `ai_chat_history` (different schema). The Rust code queries `ai_chat_messages`. A migration gap exists — ensure the rebuild creates `ai_chat_messages` matching the Rust AiChatMessage struct.

#### suppliers
supplier_id (PK), name, phone, email, contact_name, address, notes, is_active (1), created_at, updated_at.

#### purchase_orders
po_id (PK), supplier_id (FK→suppliers), status ('draft'), expected_date, received_date, notes, created_by, created_at, updated_at.

#### purchase_order_lines
po_line_id (PK), po_id (FK→purchase_orders), product_id, product_name, ordered_qty (REAL, 0), received_qty (REAL, 0), unit_cost_minor (0), created_at.

#### proactive_alerts
alert_id (PK), branch_id, alert_type, severity (CHECK 'info'/'warning'/'critical'), title, description, detail_json, detected_at, dismissed_at, dismissed_by_user_id, created_at. Indexes: (branch_id, detected_at DESC), partial on branch_id WHERE dismissed_at IS NULL.

#### proactive_watermark
rule_name (PK), last_checked.

#### wa_messages
id (PK), chat_jid, chat_name, is_group (0), sender_jid, sender_name, body, ts, read (0), created_at, media_type. Index: idx_wa_messages_unread (read, ts).

#### payment_confirmations
id (PK), customer_jid, receipt_number, delivery_id, expected_amount_minor, currency_exponent (3), business_name, branch_id, status (CHECK 'pending'/'confirmed'/'failed'), ocr_text, amount_found, name_matched (0), customer_name, reason, seen (0), created_at, resolved_at.

---

## 6. AI SYSTEM DESIGN

### Multi-Provider Architecture

Three providers via the `Provider` enum:
- **Anthropic** — native client calling `https://api.anthropic.com/v1/messages`, default model `claude-sonnet-4-6`, API key from OS credential store or app_config
- **OpenAI** — OpenAI-compatible client, base_url and model from app_config, API key from credential store
- **Gemini** — OpenAI-compatible client at `https://generativelanguage.googleapis.com/v1beta/openai`, default model `gemini-2.0-flash`, API key from credential store

Provider resolution: `Provider::from_db()` checks `ai_config` table first, then `app_config`, then OS keyring.

### Multi-Turn Tool Loop (Text Diagram)

```
User Message
  │
  ├─> Provider::send_chat(system, messages, tool_defs)
  │     └─> Anthropic/OpenAI API call (streaming SSE to frontend)
  │
  ├─> Parse ChatResult { text, tool_calls }
  │
  ├─> For each tool_call:
  │     ├─ If read-only: execute_read_tool() → append text result to history
  │     ├─ If mutation:
  │     │   ├─ resolve_mutation_risk() → Risk::Low (auto-apply) or Risk::High (confirm)
  │     │   ├─ dry_run_mutation() → ToolPreview sent to frontend
  │     │   ├─ On confirm: execute_mutation() → MutationResult
  │     │   │   └─ Creates undo record, writes audit log
  │     │   └─ Append tool_result to history
  │     └─ If engine-op: redirect to auto_apply_mutation()
  │
  ├─> Provider::continue_with_tool_turns(messages, completed_turns)
  │     └─> API follow-up call with tool results in context
  │
  └─> Repeat until stop_reason = "end_turn"
       MAX_TOOL_ROUNDS = 20
```

### Risk Classification Rules

**`Risk` enum:** `Low | High`

**Static high-risk (sync `mutation_risk()`):**
- Tool name starts with `bulk_` or `bulk.`
- Tool name starts with `delete_`, `archive_`, `remove_`
- Tool name contains `user`, `role`, or `_pin`
- Tool is `void_sale`, `merge_products`, `backup_database`, or `vacuum_database`

**Dynamic high-risk escalation (async `resolve_mutation_risk()`):**
- For `update_product_price`: if `|new - current| / current >= 0.50` (50% swing), upgrade to High

**Low-risk mutations (auto-applied without confirmation):**
`update_product_price`, `create_product`, `update_product`, `set_product_active`, `receive_stock`

### Streaming Protocol — All StreamEvent Variants

Defined in `domain/ai_admin.rs` as serde-tagged enum `#[serde(tag="type", rename_all="snake_case")]`:

| Variant | Fields | Purpose |
|---|---|---|
| `Token` | `{ text: String }` | Text delta from LLM |
| `ToolStart` | `{ name: String }` | Tool invocation beginning |
| `ToolDone` | `{ name: String }` | Tool execution complete |
| `MutationPending` | `{ action_id, tool_name, preview, expires_at, assistant_text }` | Single mutation awaiting confirm |
| `MutationBatchPending` | `{ actions: Vec<BatchPendingAction>, assistant_text }` | Batch mutations awaiting confirm |
| `MutationApplied` | `{ tool_name, description, undo_id }` | Mutation successfully executed |
| `Navigate` | `{ tab: String }` | Request tab switch |
| `RunPreview` | `{ run_id, op_id, description, count: i64, samples: Vec<Value> }` | Engine op preview |
| `RunProgress` | `{ run_id, done: i64, total: i64 }` | Engine op batch progress |
| `RunDone` | `{ run_id: String }` | Engine op completed |
| `RunFailed` | `{ run_id, error: String }` | Engine op failed |
| `Done` | — | Stream complete |
| `Error` | `{ message: String }` | Stream error |

SSE transport: Tauri `AppHandle::emit("sse-event", payload)` for each event. Frontend listens via Tauri Channel.

### Undo System

**Two-tier architecture:**

**Tier 1 — Single tool undo** (`undo_records` table):
- Each `execute_mutation()` creates an undo record with `rollback_tool` + `rollback_input_json`
- Status: `available` → `undone`
- `execute_undo()` reconstructs input from JSON and calls `execute_mutation()` for the reverse operation
- Irreversible tools return `_no_undo` (backup, sync repair, void, merge)

**Tier 2 — Batch engine undo** (`ai_run_undo_log` table):
- Each batch records pre-mutation snapshots
- `undo_run()` replays entries in reverse `seq_no DESC` order
- Status transitions: `done` → `undoing` → `undone`

### Proactive Alert Rules (8 Types)

1. **Low stock** — `quantity_on_hand < reorder_point` for tracked products
2. **Dead stock** — no sales in 90 days with positive stock
3. **Overstock** — `quantity_on_hand > 3x monthly_sales_velocity`
4. **Negative stock** — `quantity_on_hand < 0` (data integrity issue)
5. **Price variance** — selling price below cost for any active product
6. **Sync stalled** — consecutive sync failures >= 6
7. **Open shift** — shift open > 24 hours without closure
8. **Cash discrepancy** — `|counted - expected| > 5.000 BHD` (5000 fils) at shift close

Severity levels: `info`, `warning`, `critical`. Deduplicated by `(branch_id, alert_type)` — only one undismissed alert per type. Polling interval: 120 seconds (`PROACTIVE_POLL_SECS`).

### Feature Toggle Gates (9 Categories)

| Gate Key | Function | Covers |
|---|---|---|
| `web_search` | `is_web_search_tool()` | `web_search` |
| `web_fetch` | `is_web_fetch_tool()` | `fetch_url`, `smart_barcode_lookup` |
| `compare_prices` | `is_compare_prices_tool()` | `compare_store_prices` |
| `market_price` | `is_market_price_tool()` | `search_market_prices`, `get_exchange_rates` |
| `smart_analytics` | `is_smart_analytics_tool()` | ~60 analytics tools (LTV, RFM, forecasting, bundling) |
| `proactive` | `is_proactive_tool()` | Alert and notification tools |
| `inventory_ops` | `is_inventory_ops_tool()` | ~20: bulk stock take, suppliers, POs |
| `customer_insights` | `is_customer_insights_tool()` | ~10: segmentation, RFM, churn, LTV |
| `insights_engine` | `is_insights_engine_tool()` | ~6: forecasting, trends, advanced metrics |

Core read-only tools are always available — never gated.

---

## 7. TOOL INVENTORY

### Categories

Tools are organized into: **core read-only** (~70, always available), **extended read-only batch 1** (15 tools), **extended read-only batch 2** (~55 analytics), **extended read-only batch 3** (~25 supplier/inventory), **core mutations** (~32), **extended mutations batch 1** (21), **extended mutations batch 2** (24), **extended mutations batch 3** (6), **engine operations** (8).

### Core Read-Only Tools (representative subset)

| Tool | Parameters | Risk |
|---|---|---|
| `get_product` | `{ product_id: string }` | — |
| `search_products` | `{ query: string }` | — |
| `list_products` | `{ after_id?: string, page_size?: number }` | — |
| `get_stock_levels` | `{ product_id: string }` | — |
| `get_low_stock` | — | — |
| `get_below_reorder` | — | — |
| `list_categories` | — | — |
| `get_today_summary` | — | — |
| `get_sales_report` | `{ from: string, to: string }` | — |
| `get_sales_list` | `{ from, to, offset?, limit? }` | — |
| `get_sale_detail` | `{ sale_id: string }` | — |
| `get_recent_refunds` | `{ days?: number }` | — |
| `get_z_report` | `{ date: string }` | — |
| `get_x_report` | `{ shift_id: string }` | — |
| `get_cash_summary` | `{ shift_id: string }` | — |
| `get_eod_cashup` | `{ date: string }` | — |
| `list_deliveries` | `{ branch_id, status?, payment_status? }` | — |
| `list_customers` | `{ search?: string }` | — |
| `list_users` | — | — |
| `list_roles` | — | — |
| `get_db_integrity` | — | — |
| `get_sync_status` | — | — |
| `get_sync_diagnostics` | — | — |
| `get_branch_settings` | — | — |
| `get_thermal_config` | — | — |
| `get_held_carts` | — | — |
| `get_whatsapp_status` | — | — |
| `get_hub_status` | — | — |
| `get_audit_log` | `{ from?, to?, page? }` | — |
| `get_migration_status` | — | — |
| `list_suppliers` | — | — |
| `list_promotions` | — | — |
| `list_tax_rules` | — | — |
| `list_devices` | — | — |
| `list_purchase_orders` | `{ supplier_id?, status? }` | — |
| `open_tab` | `{ tab: string }` | — |

### Core Mutation Tools (representative subset)

| Tool | Parameters | Risk | Undo |
|---|---|---|---|
| `update_product_price` | `{ product_id, new_price_minor }` | Low (unless 50% swing) | update_product_price with old price |
| `set_product_active` | `{ product_id, is_active }` | Low | Inverse |
| `update_product_name` | `{ product_id, name }` | Low | Update with old name |
| `adjust_stock` | `{ product_id, quantity_delta }` | Low | adjust_stock with -delta |
| `stock_take` | `{ product_id, new_quantity }` | Low | stock_take with old qty |
| `create_product` | `{ name, category_id, price_minor, ... }` | Low | set_product_active=false |
| `update_reorder_point` | `{ product_id, new_reorder_point }` | Low | Update with old value |
| `create_customer` | `{ name, phone?, email? }` | Low | delete_customer |
| `create_user` | `{ display_name, username, pin, role_id }` | High | update_user is_active=false |
| `create_category` | `{ name, parent_category_id? }` | Low | delete_category |
| `create_tax_rule` | `{ name, rate_basis_points, inclusive }` | Low | delete_tax_rule |
| `void_sale` | `{ sale_id, reason }` | High | _no_undo |
| `create_refund` | `{ original_sale_id, items[], reason }` | Low | _no_undo |
| `backup_database` | `{ dest_path }` | High | _no_undo |
| `bulk_update_prices` | `{ updates[] }` | High | Per-item undo records |
| `receive_stock` | `{ product_id, quantity }` | Low | adjust_stock with negative |
| `merge_products` | `{ source_id, target_id, transfer_history }` | High | _no_undo |
| `confirm_delivery_payment` | `{ delivery_id, reference?, note? }` | Low | revert_delivery_payment |
| `cancel_delivery` | `{ delivery_id, reason }` | Low | _no_undo |
| `create_supplier` | `{ name, phone?, email? }` | Low | delete_supplier |

### Engine Operations (8 — bypass standard tool loop)

| Operation ID | Class | Parameters | Mutation |
|---|---|---|---|
| `bulk_price_adjust` | BulkPriceAdjust | `{ op: PriceOp, apply_to_sale: bool }` | Yes |
| `product.create` | ProductCreate | `{ name, category_id, barcode?, price_minor }` | Yes |
| `bulk_product_archive` | BulkProductArchive | `{}` | Yes |
| `bulk_reorder_point_update` | BulkReorderPointUpdate | `{ new_reorder_point: f64 }` | Yes |
| `bulk_promotion_apply` | BulkPromotionApply | `{ promo_price_minor, effective_from, effective_to }` | Yes |
| `bulk_promotion_remove` | BulkPromotionRemove | `{}` | Yes |
| `bulk_supplier_price_sync` | BulkSupplierPriceSync | `{ updates: [{product_id, new_cost_minor}] }` | Yes |
| `bulk_stock_variance_fix` | BulkStockVarianceFix | `{}` | Yes |

### Intent Engine (20 Deterministic Intents)

`search_products`, `get_product_detail`, `create_product`, `update_product`, `get_low_stock`, `receive_stock`, `get_sales_report`, `get_today_summary`, `list_customers`, `create_customer`, `list_users`, `create_user`, `get_cash_status`, `list_deliveries`, `get_sync_status`, `get_audit_log`, `backup_database`, `open_tab`.

Mutation intents: `create_product`, `update_product`, `receive_stock`, `create_customer`, `create_user`, `backup_database`.

---

## 8. POS SYSTEM

### Cart/CartLine Model

**CartLine** (14 fields):
```rust
pub struct CartLine {
    pub cart_line_id: String,          // ULID
    pub product_id: Option<String>,    // None for custom items
    pub product_name: String,
    pub sku: Option<String>,
    pub barcode: Option<String>,
    pub quantity: String,              // Decimal string, e.g. "2.5"
    pub unit_price_minor: i64,
    pub line_discount_minor: i64,
    pub line_discount_reason: Option<String>,
    pub tax_rule_id: String,
    pub tax_rate_basis_points: i64,
    pub tax_inclusive: bool,
    pub tax_amount_minor: i64,         // Server-computed, advisory from client
    pub line_total_minor: i64,         // Server-computed, advisory from client
    pub note: Option<String>,
    pub voided: bool,
}
```

**Cart** (7 fields):
```rust
pub struct Cart {
    pub cart_id: String,               // ULID
    pub branch_id: String,
    pub device_id: String,
    pub shift_id: String,
    pub cashier_user_id: String,
    pub customer_id: Option<String>,
    pub lines: Vec<CartLine>,
    pub bill_discount_minor: i64,
    pub bill_discount_reason: Option<String>,
}
```

Methods: `gross_total()`, `tax_total()`, `discount_total()`, `post_line_total()`, `net_total()`, `validate()`.

Constants: `MAX_QTY = 1_000_000`, `MAX_MINOR = 1_000_000_000_000` (~1B BHD).

### finalize_sale — 15-Step Transaction

1. **Shift check** — Verify shift exists and status is "open"
2. **Empty cart check** — Count non-voided lines, reject if zero
3. **Cart validation** — `cart.validate()`: all quantities and totals within MAX ranges
4. **Price verification** — Batch-query `product_prices` for all line product_ids; record corrections if cart price != DB price
5. **Server-side tax recalculation** — Discard all client tax values. For each line:
   - `effective_price = corrected_price.unwrap_or(line.unit_price_minor)`
   - `subtotal = mul_minor_by_qty(effective_price, line.quantity)`
   - `discounted = max(subtotal - line.line_discount_minor, 0)`
   - Tax: inclusive → `discounted * rate / (10000 + rate)` half-up; exclusive → `calc_tax_exclusive(discounted, rate)`
   - `line_total = discounted + (if tax_inclusive { 0 } else { tax_amount })`
   - Accumulate `server_gross`, `server_tax`, `server_post_line`, `server_discount`
   - `server_net = max(server_post_line - bill_discount_minor, 0)`
6. **Payment validation** — `sum(payments.amount_minor)` must exactly equal `server_net`
7. **Context lookup** — JOIN branches+devices+users for branch_code, currency, branch_name, device_code, cashier_name
8. **Generate sale_id** — New ULID
9. **Timestamp** — `now` = RFC 3339 UTC, `business_date` = Local::now().format("%Y-%m-%d")
10. **BEGIN transaction**
11. **Receipt number** — `UPDATE devices SET next_receipt_seq = next_receipt_seq + 1 WHERE device_id = ? RETURNING next_receipt_seq`. Format: `{branch_code}-{device_code}-{seq:08}`
12. **INSERT sales** — Status='completed', sync_status='pending', all totals (server-computed), idempotency_key, customer_id, is_delivery
13. **INSERT sale_items** — Per line: product_name_snapshot, sku_snapshot, barcode_snapshot, quantity, effective_price, server-computed tax/line_total, voided=0
14. **INSERT payments** — Per payment: cash→tendered+change, non-cash→amount only. Status='approved'
15. **Stock deduction** — For tracked products:
    - If `allow_negative_stock=false`: `UPDATE stock_levels SET quantity_on_hand = CAST(CAST(qty AS REAL) - delta AS TEXT) WHERE ... AND CAST(qty AS REAL) >= delta`
    - If `allow_negative_stock=true`: Same without the guard. `rows_affected==0` check with existing stock record → "Insufficient stock" error
16. **Audit log** — SHA-256 chained entry, event_type="sale.created"
17. **Optional delivery** — `create_delivery_in_tx()` in same transaction
18. **COMMIT**
19. **Post-commit loyalty** — `UPDATE customers SET loyalty_points = loyalty_points + floor(net_total / 1000)` (best-effort, error logged)
20. **Post-commit stock movements** — `movements::deduct_sale()` creates stock_movement records, returns LowStockAlert list (best-effort)
21. **Return SaleResult**

### Payment Methods

String values: `"cash"`, `"card"`, `"wallet"`, `"other"`.
- Cash: Requires `tendered_minor` >= `amount_minor`, produces `change_minor = tendered - amount`
- Card/Wallet/Other: No change, optional `external_reference`

### Void Flow

1. RBAC: manager_or_owner
2. `UPDATE sales SET status='voided', updated_at=now, sync_status='pending' WHERE sale_id=? AND status='completed' RETURNING branch_id, device_id`
3. If no rows affected → "Sale not found or already voided"
4. Post-commit: `movements::return_void_sale()` restores inventory (best-effort, returns stock_warning)
5. Audit log: event_type="sale.voided"

### Refund Flow with Per-Item Ceiling

1. RBAC: require_any_role on own device
2. Check if user is manager/owner for cross-device override
3. Cross-device policy:
   - Same device: no override needed
   - Cross-device + manager/owner: `override_used = true`
   - Cross-device + cashier: requires `manager_override_token` (60s TTL ULID token, consumed once)
4. `BEGIN IMMEDIATE`
5. Validate original sale exists and status != 'voided'
6. Per-item atomic guard:
   ```sql
   UPDATE sale_items
   SET refunded_amount_minor = refunded_amount_minor + ?
   WHERE sale_item_id = ? AND sale_id = ?
     AND (refunded_amount_minor + ?) <= line_total_minor
   ```
   If `rows_affected == 0`: check if item exists → if yes, amount exceeds refundable ceiling → Validation error
7. Generate refund receipt number (shared device sequence)
8. Validate `reason_code` ∈ {customer_return, defective, wrong_item, exchange, other} (default: "other")
9. INSERT refunds + refund_items
10. Check if fully refunded: `all sale_items.refunded_amount_minor >= line_total_minor` → status='refunded', else 'partially_refunded'
11. Audit log entry
12. COMMIT
13. Post-commit stock return: `movements::return_refund()` (best-effort)

### Hold/Resume Cart

**Hold:** Serialize Cart to JSON → INSERT held_carts (held_cart_id, branch_id, device_id, shift_id, cashier_user_id, cart_json, held_at, note)
**List:** `SELECT ... FROM held_carts WHERE device_id = ? ORDER BY held_at DESC`
**Resume:** Fetch cart_json → deserialize → verify all product_ids still exist → delete held_cart row → return Cart with new cart_id and shift_id
**Delete:** `DELETE FROM held_carts WHERE held_cart_id = ?`

### Receipt Format

**ESC/POS thermal (48 char width):** Store name (centered, double-size, bold), body lines (left-aligned, max 500), 4 line feeds + cut (`GS V 0`).

**PDF (A5 148×210mm, Helvetica):** Store name (14pt bold), phone (8.5pt), "RECEIPT" header (13pt bold), receipt number, date, cashier, tax/CR numbers, item table (name truncated to 35 chars, qty, amount), subtotal, discount, tax, TOTAL (11pt bold), payment methods, change, delivery address, "Thank you" footer. ASCII-only substitution.

---

## 9. MONEY RULES

### Function Signatures (domain/money.rs)

```rust
pub fn mul_minor_by_qty(minor: i64, qty: &str) -> i64
pub fn qty_in_range(qty: &str, max: i64) -> bool
pub fn add_decimal_qty_str(a: &str, b: &str) -> String
pub fn parse_major_to_minor(s: &str, exponent: u32) -> Option<i64>
pub fn format_minor(minor: i64, exponent: u32) -> String
pub fn apply_discount_bp(amount_minor: i64, discount_basis_points: i64) -> i64
pub fn calc_tax_exclusive(price_minor: i64, rate_basis_points: i64) -> i64
```

### Rounding Rules

- **mul_minor_by_qty:** `(minor * qty_num + denom / 2) / denom` — integer half-up rounding to nearest minor unit
- **apply_discount_bp:** Floor — `amount_minor * discount_basis_points / 10_000` (100 bp = 1%)
- **calc_tax_exclusive:** Half-up — `(price_minor * rate_basis_points + 5_000) / 10_000`
- **Quantity parsing:** 9 decimal digits max, uses i128 intermediates, output clamped to non-negative i64

### BHD-Specific

- **Exponent:** 3 (1.000 BHD = 1000 fils)
- **format_minor:** Handles negative values correctly — `-500` minor with exp=3 → `"-0.500"` (not `"0.500"`)
- **parse_major_to_minor:** Rejects input with more decimal places than exponent (no silent truncation)

### Discount Application Order

1. Per-line discount (`line_discount_minor`) applied first, reducing taxable base
2. Bill-level discount (`bill_discount_minor`) applied to `post_line_total` (sum of line_totals after line discounts)
3. `net_total = max(post_line_total - bill_discount_minor, 0)`
4. Tax computed on discounted amount per line
5. Identity: `net_total == gross_total - discount_total + tax_total`

### Tax Calculation

- **Exclusive tax:** `tax = price * rate_basis_points / 10000` (half-up), `total = price + tax`
- **Inclusive tax:** `tax = discounted * rate / (10000 + rate)` (half-up), `total = discounted` (tax embedded)

### Constants

| Constant | Value |
|---|---|
| BHD exponent | 3 |
| Non-BHD exponent | 2 |
| MAX_QTY | 1,000,000 |
| MAX_MINOR | 1,000,000,000,000 |
| Discount bp divisor | 10,000 |
| Tax bp divisor | 10,000 |
| Max fractional digits | 9 |

---

## 10. SYNC SYSTEM

### Dual-Mode Architecture

Three modes determined by `app_config.hub_mode`:
- **Standalone:** No sync, `load_client()` returns None
- **Terminal:** Syncs with remote Hub over HTTP. Pushes pending rows, pulls changes
- **Hub:** Source of truth. Never pushes/pulls. Marks all rows synced, prunes old data

### PostgREST-Compatible Endpoints

Terminal communicates with Hub via REST:
- `POST /rest/v1/{table}` — Upsert rows (JSON array body, `Prefer: resolution=merge-duplicates`)
- `GET /rest/v1/{table}?updated_at=gt.{ts}&order=updated_at.asc&limit={N}&offset={N}` — Pull rows since timestamp
- `GET /rest/v1/branches?is_active=eq.true&order=created_at.asc&limit=1` — Pull active branch
- `GET /zanpos/info` — Identity probe, returns `HubInfo { store_name, branch_id, hub_version, hub_time }`

### Worker Loop with Backoff

```
loop {
    read consecutive_failures
    read hub_mode
    determine base_interval:
        hub mode → DEFAULT_HUB_INTERVAL_SECS (300)
        terminal → DEFAULT_INTERVAL_SECS (10)
    apply backoff:
        failures >= 6 → wait = base * 5
        failures >= 3 → wait = base * 2
        else → wait = base
    sleep(wait)
    run_once()  // try_lock prevents concurrent runs
}
```

On panic: supervisor restarts after 5 seconds.

### Row Application Engine — Merge Strategies

**Last-Writer-Wins (`apply_lww`):** Used for categories, tax_rules, products, devices, branches, shifts, delivery_orders, stock_levels, users.
```sql
INSERT INTO {table} ({cols}, sync_status) VALUES ({vals}, 'synced')
ON CONFLICT({pk}) DO UPDATE SET {set_clause}, sync_status='synced'
WHERE datetime({table}.updated_at) < datetime(excluded.updated_at)
```
Null values excluded from SET clause. `should_skip_column()` excludes: sync_status, sync_attempts, deleted_at, version, pin_hash, failed_pin_attempts, locked_until, last_login_at, last_seen_at, next_receipt_seq, override_used.

**Append-Only (`apply_append_only`):** Used for sales, sale_items, payments, refunds, refund_items, stock_movements, audit_logs, product_prices, cash_events.
```sql
INSERT OR IGNORE INTO {table} ({cols}, sync_status) VALUES ({vals}, 'synced')
```

**Customer Merge:** Same as LWW but `loyalty_points = MAX(customers.loyalty_points, excluded.loyalty_points)`.

**app_config Allowlist:** Only keys in `ALLOWED_CONFIG_KEYS` are synced. LWW with updated_at guard.

**Per-table pre-processing:**
- Devices: Deactivates duplicates with same (branch_id, device_code)
- Shifts: Closes other open shifts on same device before applying an open shift
- Users: Deactivates duplicates with same username; fills missing branch_id; replaces missing pin_hash with `"*REMOTE-ONLY*"`; remaps unknown role_id to owner

### FK-Safe Push Ordering

```
branches → categories → tax_rules → products → devices → users →
customers → shifts → sales → sale_items → payments → refunds →
refund_items → stock_movements → stock_levels → audit_logs →
delivery_orders → product_prices → cash_events → app_config
```

Push query: `SELECT * FROM {table} WHERE sync_status='pending' AND sync_attempts < 10 LIMIT 50`. On success: bulk UPDATE to 'synced'. On failure: increment sync_attempts.

### Watermark Tracking

- Per-table watermark stored in `app_config` as `sync_v2_watermark_{table}`
- Starts as `"1970-01-01T00:00:00Z"` (epoch)
- After each batch, max `updated_at` among applied rows becomes new watermark
- Offset tracking: when watermark stays same (all rows in full page share timestamp), offset increments
- On failure: watermark NOT advanced (BUG-SYNC-4 fix)
- FK violations: advance past the row (retried when dependencies arrive)
- SQLITE_BUSY: one retry after 4 seconds

### Constants

| Constant | Value |
|---|---|
| DEFAULT_INTERVAL_SECS (terminal) | 10 |
| DEFAULT_HUB_INTERVAL_SECS (hub) | 300 |
| BATCH_SIZE (push) | 50 |
| MAX_ATTEMPTS (before stuck) | 10 |
| PULL_PAGE_LIMIT | 500 |
| MAX_HTTP_RETRIES | 5 |
| BASE_RETRY_MS | 200 |
| MAX_RETRY_DELAY_MS | 30,000 |
| STOCK_DRIFT_TOLERANCE | 0.001 |
| SYNC_TABLES count | 19 |

### Daily Pruning Policy

Runs once per 24 hours. Only prunes synced rows.
- **Sales retention:** 90 days (configurable via `retention_days_sales`)
- **Audit logs retention:** 30 days (configurable via `retention_days_logs`)

FK-safe deletion order:
1. sale_items (WHERE sale_id IN old synced sales)
2. payments (WHERE sale_id IN old synced sales)
3. sales (WHERE sync_status='synced' AND sold_at < cutoff)
4. audit_logs (WHERE created_at < cutoff AND sync_status='synced')
5. stock_movements (WHERE created_at < cutoff AND sync_status='synced')

Post-deletion: `PRAGMA wal_checkpoint(TRUNCATE)`, then `VACUUM` (both best-effort).

---

## 11. WHATSAPP INTEGRATION

### Sidecar Architecture

```
┌─────────────────────────────┐
│  Tauri Rust Backend         │
│  ┌───────────────────────┐  │
│  │ whatsapp_commands.rs  │──┼── HTTP ──┐
│  │ whatsapp_inbox_cmds.rs│  │          │
│  └───────────────────────┘  │          ▼
│  SidecarHandle {            │   ┌──────────────────┐
│    child: CommandChild      │   │  Node.js Sidecar  │
│    auth_token: String       │   │  Express :3131    │
│  }                          │   │  Baileys v7       │
└─────────────────────────────┘   │  Tesseract OCR    │
                                  └──────────────────┘
```

Rust spawns `node.exe server.mjs --session-dir=<path>` as Tauri sidecar. Auth via 256-bit hex token written to `.sidecar_token` file. All requests (except /health) require `X-Sidecar-Token` header. Windows Job Object binds the child process so it's killed when the POS exits.

### All REST Endpoints

| Method | Path | Auth | Request Body | Response Body |
|---|---|---|---|---|
| GET | `/health` | No | — | `{"ok": true}` |
| GET | `/status` | Yes | — | `{"connected": bool, "qr"?: string}` |
| POST | `/send` | Yes | `{"to": "973XXXXXXXX", "message": "..."}` | `{"ok": true}` or `{"ok": false, "error": "..."}` |
| POST | `/send-document` | Yes | `{"to", "caption"?, "document_base64", "mimetype"?, "filename"?}` | `{"ok": true}` or error |
| GET | `/contacts` | Yes | — | `[{"id": "JID", "name": "..."}]` |
| GET | `/messages` | Yes | `?after=<seq>` | `{"messages": [...], "cursor": int}` |
| GET | `/media` | Yes | `?id=<msg_id>` | `{"ok": true, "base64": "...", "mimetype": "..."}` |
| GET | `/ocr` | Yes | `?id=<msg_id>` | `{"ok": true, "text": "..."}` |
| GET | `/groups` | Yes | — | `[{"id": "JID", "name": "..."}]` |
| POST | `/disconnect` | Yes | — | `{"ok": true}` (logout + restart after 500ms) |
| POST | `/shutdown` | Yes | — | `{"ok": true}` (graceful exit, 3s timeout) |

### Constants

| Constant | Value |
|---|---|
| Sidecar port | 3131 |
| Sidecar URL | `http://127.0.0.1:3131` |
| INBOX_CAP | 300 messages |
| MEDIA_CAP | 40 files |
| QR timeout | 60,000ms |
| connectTimeoutMs | 60,000ms |
| keepAliveIntervalMs | 30,000ms |
| defaultQueryTimeoutMs | 60,000ms |
| retryRequestDelayMs | 1,500ms |
| Reconnect base delay | 2,000ms (exponential, cap 30,000ms) |
| Session dir | `./wa-session` |
| Tesseract lang | "eng", OEM 1 (LSTM) |
| WhatsApp API version | Baileys v7.0.0-rc13 |
| Browser profile | `Browsers.macOS("Desktop")` |

### Pairing Flow

1. Sidecar starts, calls `startBaileys()`
2. If no valid auth state, WhatsApp emits QR code
3. `qrcode` package converts to `data:image/png;base64,...` data URL
4. Rust polls `GET /status` → frontend displays QR in `WhatsAppQRModal`
5. User scans with phone → `connection: "open"` → `isConnected = true`, `qrDataUrl = null`

### Message Polling Flow

1. Baileys emits `messages.upsert` (type="notify", not fromMe)
2. Processed into ring buffer (INBOX_CAP=300), assigned incrementing `msgSeq`
3. Images immediately downloaded via `persistImage()` (media URLs expire)
4. Frontend calls `whatsappPollMessages` → sidecar `GET /messages?after=<lastSeq>` → filters to owner/group targets
5. Payment verification triggered for image replies from customers
6. Messages stored in `wa_messages` table

### Payment Confirmation Flow

1. Customer sends payment screenshot via WhatsApp
2. Message arrives with `mediaType: "image"`, image persisted to disk
3. `run_verification()` triggered: `GET /ocr?id=<msgId>` → Tesseract.js extracts text
4. OCR text + expected amount + business name → AI model for matching
5. Result stored in `payment_confirmations` table (status: confirmed/failed)
6. Notification displayed in POS frontend

---

## 12. HUB

### Axum Server Spec

- **Bind:** `0.0.0.0:{port}`, default port 8923 (configurable via `app_config.hub_port`)
- **Body limit:** 32 MB
- **Graceful shutdown:** via `tokio::sync::oneshot::channel`

### Endpoints

| Method | Path | Auth | Description |
|---|---|---|---|
| GET | `/rest/v1/` | Bearer token | Health probe, returns 200 |
| GET | `/zanpos/info` | Bearer token | Returns `{store_name, branch_id, hub_version, hub_time}` |
| GET | `/rest/v1/{table}` | Bearer token | Pull rows with query params: `updated_at=gt.{ts}`, `origin_device_id=neq.{id}`, `limit` (1-1000, default 500), `offset`, `is_active=eq.true` |
| POST | `/rest/v1/{table}` | Bearer token | Push rows: JSON array body, each row applied via `apply::apply_row()` |

Table validation: Must be in `SYNC_TABLES` (19 tables). Unknown table → 404.

### Auth Model

- **Token storage:** Only SHA-256 digest in `HubState.token_digest: [u8; 32]`. Raw token never in server memory
- **Token generation:** 32 random bytes, stored in OS credential store (Windows Credential Manager, key="hub_store_token")
- **Constant-time comparison:** XOR-accumulate all bytes, return `diff == 0`
- **Header:** `Authorization: Bearer {token}`
- **Device tracking:** `X-Zanpos-Device` header stored in `SeenMap` (device_id → (ip, last_seen RFC3339))

### LAN Discovery

`lan_ips()` function:
1. Create UDP socket bound to `0.0.0.0:0`
2. Connect to probe addresses in order: `8.8.8.8:80`, `192.168.1.1:80`, `10.0.0.1:80`
3. Read local address of connected socket (no packets sent — UDP connect only selects route)
4. Deduplicate, filter non-zero IPs, return unique list

### HubHandle

```rust
pub struct HubHandle {
    pub port: u16,
    pub seen: SeenMap,  // Arc<Mutex<HashMap<device_id, (ip, last_seen)>>>
    shutdown: oneshot::Sender<()>,
}
```

---

## 13. SECURITY

### PIN Authentication

**Algorithm:** Argon2id via `argon2` crate (v0.5.x).
**Params (Argon2::default()):**
- Memory (m_cost): 19,456 KiB (19 MiB)
- Iterations (t_cost): 2
- Parallelism (p_cost): 1
- Output length: 32 bytes
- Salt: 22 chars (128-bit), OsRng

**PIN format:** 4-64 characters.

**Legacy migration:** On startup, `rehash_plain_pins()` finds all `pin_hash LIKE 'PLAIN:%'` rows, re-hashes with Argon2id, updates. At runtime, `verify_pin()` rejects any `PLAIN:` prefix hash (returns false, logs error).

**Lockout rules:**
- MAX_ATTEMPTS = 5
- LOCKOUT_MINUTES = 60
- On 5th consecutive failure: `locked_until = now + 60 minutes`
- After lockout expires: next attempt resets `failed_pin_attempts = 0, locked_until = NULL`
- 1-second minimum delay on ALL failed PIN attempts (brute-force rate limiting)
- User list endpoint: max once per 3 seconds

### Audit Hash Chain

**Algorithm:** SHA-256 (sha2 crate).

**Hash input (12 fields, NUL-separated, in order):**
1. audit_log_id
2. event_type
3. entity_type
4. entity_id
5. actor_user_id
6. actor_type
7. created_at
8. before_json ("" if None)
9. after_json ("" if None)
10. reason ("" if None)
11. previous_hash

**Output:** 64 hex characters.

**Verification:** Walk all rows for device_id ordered by `created_at ASC, audit_log_id ASC`. For each SHA-256 row (length(hash)==64): check stored previous_hash == running previous, recompute and compare hash.

### Secret Storage

**Crate:** `keyring` v2 (Rust keyring crate).
**OS backend:** Windows Credential Manager (DPAPI-encrypted).
**Service name:** `"zanpos"`.
**Keys:** `hub_store_token`, `anthropic_api_key`, `openai_api_key`, `gemini_api_key`, `supabase_service_key` (legacy, deleted on hub migration).

Functions: `get_secret(key) -> Option<String>`, `set_secret(key, value) -> bool`, `delete_secret(key)`.

### CSP Policy

```
default-src 'self';
script-src 'self';
connect-src 'self' http://127.0.0.1:3131 https://github.com https://*.githubusercontent.com https://quranapi.pages.dev;
media-src 'self' https://github.com https://*.githubusercontent.com;
style-src 'self' 'unsafe-inline';
img-src 'self' data: blob:;
font-src 'self'
```

### PII Handling

**Considered PII:** PIN hashes, failed_pin_attempts, locked_until, last_login_at, device last_seen_at, device next_receipt_seq, audit override_used flag, deleted_at timestamps, version numbers.

All PII columns excluded from sync via `should_skip_column()`. PIN hashes never transmitted over network. When receiving remote user updates, missing `pin_hash` replaced with `"*REMOTE-ONLY*"`.

**API keys:** Never stored in SQLite. Stored in OS credential manager. Fetched at point of use, held only in memory.

**Hub token:** SHA-256 digest in server memory; raw token in OS credential store; transmitted as Bearer token over LAN HTTP (no TLS).

---

## 14. HARDWARE INTEGRATION

### Thermal Printer Detection

`thermal_list_ports()`:
1. Windows: PowerShell `Get-CimInstance -Class Win32_Printer | Select-Object Name, Default | ConvertTo-Json -Compress`. Results cached for 30 seconds. Default printer marked "★ (Default)"
2. Serial: `serialport::available_ports()` for COM ports. Skip ports already in Windows printer list. Label format: `"{port} -- {description} [Serial]"`

### Printing Flow

`write_to_port(port_name, baud, payload)`:
- If port doesn't start with "COM": Windows print spooler via `OpenPrinterW`, `StartDocPrinterW`, `WritePrinter` (RAW data type, document name="ZANPOS Receipt")
- If COM port: `serialport` crate, user-configured baud rate (default 9600), RTS/CTS flow control, 15s timeout

ESC/POS commands:
- Init: `ESC @` (0x1B 0x40)
- Align: `ESC a n` (0=left, 1=center, 2=right)
- Bold: `ESC E n`
- Double size: `GS ! 0x11`
- Line feed: `ESC d n`
- Cut: `GS V 0` (full cut)

Valid baud rates: 9600, 19200, 38400, 57600, 115200.

### Cash Drawer

`ESC p 0 60 120` (0x1B 0x70 0x00 60 120):
- Pin 2 (Epson TM / Star mPOP default)
- On-time: 60 × 2ms = 120ms
- Off-time: 120 × 2ms = 240ms

Sent via same `write_to_port` routing as printer.

### Barcode Scanner

No prefix/suffix stripping in backend. Barcode string received as-is from frontend input. Scanner input handled as keyboard wedge in the browser; `PosPage` uses a scan buffer with serial draining (`scanBufferRef`, `scanDrainingRef`) to prevent race conditions from rapid-fire USB scanner input.

---

## 15. MIGRATION AGENT

### Supported Source Databases

- SQLite (.db, .sqlite, .sqlite3)
- Excel (.xlsx, .xls) via calamine
- CSV (.csv)
- JSON (.json)
- SQL dump files (.sql)
- MySQL (via mysql crate)
- MSSQL (via tiberius crate)
- Microsoft Access (.mdb, .accdb) via PowerShell OleDb
- ZIP archives (extracted with zip-slip protection)

### Inspection Flow

1. `migration_inspect_file(path)` — reads schema: file type, sheet names, column names + types, row counts, sample values (max 500K cells)
2. `migration_ai_map(schema, currency_exponent)` — AI-powered field mapping with confidence scores
3. User reviews/adjusts mappings in MappingDrawer UI

### AI-Assisted Field Mapping

The migration agent chat (`migration_agent_chat`) uses 12 tool definitions with a repetition guard (max 12 autonomous steps). Tools include: query_remote_db, inspect_table, propose_mapping, execute_migration, rollback_migration, and others. The AI selects appropriate target columns (product name→products.name, price→product_prices.price_minor with currency exponent conversion, etc.).

### Execution + Rollback

`migration_execute(mappings, currency_exponent)`:
- FK-safe insertion order: categories → products → customers → stock_levels → sales → sale_items → payments
- Each insert uses `INSERT OR IGNORE` for idempotency
- Uses synthetic import anchors: `IMPORT_SHIFT_ID = "SHIFT-IMPORTED-HISTORY-000001"`
- `apply_transform(value, type, exp)` for data cleaning: strip_currency, parse_date, normalize_phone, etc.
- Safe path traversal guards on file reads
- Sandboxed shell command executor with strict allowlist

`migration_rollback(since_iso, user_id)`:
- Deletes in FK-safe reverse order
- Only deletes rows created after the timestamp
- Returns per-table deletion counts

---

## 16. BUILD & DEPLOYMENT

### Vite Config

```typescript
defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] } },
  build: {
    chunkSizeWarningLimit: 200,  // KB
    minify: "esbuild",
    rollupOptions: { output: { manualChunks(id) { /* 8 chunks */ } } }
  },
  esbuild: { drop: ["console", "debugger"] }
})
```

**8 manual chunks:** vendor (react/react-dom), icons (lucide-react), jsbarcode, tauri-api (@tauri-apps), setup-wizard, migration, officeai, backoffice (23 components).

### Tauri Config

```json
{
  "productName": "ZANPOS",
  "version": "2.0.0",
  "identifier": "com.super.zanpos",
  "build": {
    "beforeDevCommand": "npm run dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "npm run build && cd src-tauri/sidecar/whatsapp-sidecar && npm install --omit=dev && node -e \"require('fs').copyFileSync(process.execPath,'node.exe')\"",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [{
      "title": "ZANPOS — Point of Sale",
      "width": 1440, "height": 900,
      "minWidth": 1024, "minHeight": 700,
      "center": true, "maximized": true,
      "resizable": true, "fullscreen": false,
      "decorations": false
    }],
    "security": { "csp": "..." }
  },
  "bundle": {
    "active": true,
    "targets": ["nsis", "msi"],
    "resources": [
      "sidecar/whatsapp-sidecar/node.exe",
      "sidecar/whatsapp-sidecar/server.mjs",
      "sidecar/whatsapp-sidecar/node_modules/**/*"
    ],
    "windows": {
      "digestAlgorithm": "sha256",
      "timestampUrl": "http://timestamp.digicert.com",
      "nsis": {
        "displayLanguageSelector": false,
        "languages": ["English"],
        "installMode": "perMachine",
        "installerIcon": "icons/icon.ico",
        "installerHooks": "./nsis/hooks.nsh"
      }
    }
  },
  "plugins": {
    "updater": {
      "pubkey": "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDJCODBBQTMxRTZBNDhBQTUKUldTbGlxVG1NYXFBSytDckQrZ2RKNDRUem9WeFRRM2F1cDBxdlFZMG9HKzNwS2R0clRFSW5vNVEK",
      "endpoints": ["https://releases.zanpos.app/{{target}}/{{arch}}/{{current_version}}"]
    }
  }
}
```

### Window

- Title: "ZANPOS — Point of Sale"
- Size: 1440×900 (min 1024×700)
- Center: true, Maximized: true, Decorations: false
- Resizable: true, Fullscreen: false

### CI Pipeline

4 jobs on GitHub Actions:
1. **rust** (windows-latest, 30min): checkout → rust-toolchain (stable, rustfmt+clippy) → cargo cache → `cargo fmt --check` → `cargo clippy -- -D warnings` → `cargo test --lib` → `cargo audit` (18 RUSTSEC exemptions)
2. **frontend** (ubuntu-latest, 15min): checkout → Node.js 20 → `npm ci` → `npx tsc --noEmit` → `npx eslint src --ext .ts,.tsx --max-warnings 0` → `npm test` → `npm run coverage || true` → `npm audit --omit=dev --audit-level=high`
3. **build** (windows-latest, 45min, needs rust+frontend): checkout → rust-toolchain → cargo cache → Node.js 20 → `npm ci` → `tauri-action@v0` with `--debug`
4. **release-gate** (ubuntu-latest, needs all, main/master only): summary step

Trigger: push to main/master/develop, PR to main/master/develop. Concurrency: cancel-in-progress on same ref.

---

## 17. TESTING STRATEGY

### Frontend Tests

**Framework:** vitest v2, @vitest/coverage-v8
**Thresholds:** 80% lines, 80% branches, 80% functions
**Test files:**
- `__tests__/money.test.ts` — formatMoney, parseMoney: BHD (exp=3), USD (exp=2), exp=0, negative values, invalid input, round-trip invariance
- `__tests__/posProductFilters.test.ts` — filterProductsForSale, productIsOutOfStock, productIsLowStock: out-of-stock hiding, fast-seller prioritization, search+category combo filtering
- `__tests__/adminChatClear.test.ts` — clearAdminChat: calls clearHistory in correct order, swallows persistence failures

### Rust Tests

**Framework:** `#[cfg(test)]` modules inline in source files, `cargo test --lib`

Key test areas:
- `domain/cart.rs` — 12 tests: cart_new, add_line, remove_line, update_quantity, totals_with_qty_and_price, net_total_matches, validate_empty, validate_max_qty, apply_discount_percent, line_recalculate, add_decimal_qty
- `domain/money.rs` — Extensive tests for all 7 functions, BHD 3-decimal edge cases, 9-digit fraction handling
- `commands/pos_commands.rs` — Barcode resolution, cart operations, sale finalization, void operations
- `commands/cash_commands.rs` — 7 tests: safe_drop/paid_out without note rejected, paid_in without note accepted, zero/negative amounts rejected
- `commands/auth_commands.rs` — Rate limiting, lockout, PIN verification edge cases
- `commands/rbac.rs` — 7 tests: owner passes manager_or_owner, wizard_owner reactivation, cashier blocked, unknown/inactive users rejected
- `commands/refund_commands.rs` — 2 tests: sanitise_reason_code edge cases
- `commands/report_commands.rs` — 5 tests: EOD cashier name, payment method CHECK constraint
- `commands/phase10a_commands.rs` — 5 tests: backup file validity, WAL checkpoint, point-in-time semantics
- `commands/whatsapp_commands.rs` — Phone normalization, message builder tests
- `commands/migration_commands.rs` — Shell sandbox safety tests
- `commands/payment_confirm_commands.rs` — 3 tests: jid_from_phone, extract_json, parse_verdict

### Test Organization

All tests co-located with source files using `#[cfg(test)] mod tests { ... }`. Frontend tests in `src/__tests__/`. No separate test crate.

---

## 18. CONVENTIONS

### Naming

- **ULID primary keys:** `{entity}_id` (e.g., `product_id`, `sale_id`, `shift_id`)
- **ULID foreign keys:** `{entity}_id` matching the referenced table's PK
- **Rust:** snake_case for functions/variables/modules, PascalCase for types, CamelCase for variants
- **TypeScript:** camelCase for variables/functions, PascalCase for components/types, kebab-case for file names
- **SQL:** snake_case for table and column names
- **CSS classes:** kebab-case, semantic prefix-free naming
- **Tauri commands:** snake_case with domain prefix (e.g., `pos_finalize_sale`, `admin_product_create`)

### File Size

Target: 500 lines maximum. Files exceeding this (noted for rebuild consideration):
- `App.css` (15,457 lines) — split by theme/chunk
- `migration_commands.rs` (4,183 lines) — split by source DB type
- `admin_commands.rs` (2,132 lines) — split by entity (product/category/user/tax_rule)
- `tools.rs` (5,292 lines) — split by tool category
- `tools_read_ext2.rs` (2,317 lines) — split by analytics subdomain
- `sale_repo.rs` (1,230 lines) — split finalize_sale into sub-functions
- `pos_commands.rs` (1,391 lines) — split cart ops from sale ops
- `streaming.rs` (1,174 lines) — separate Anthropic SSE from OpenAI SSE

### Comment Policy

Default to no comments. Add only when the WHY is non-obvious. Remove all TODO comments, commented-out code, and dead code in the rebuild. File paths in code review should use clickable Markdown links.

### Error Handling Pattern

All fallible functions return `AppResult<T>` (alias for `Result<T, AppError>`). The `AppError` enum has 8 variants: NotFound, Validation, Permission, Conflict, Database, Internal, Serde, Io. Each has a `user_message()` method for display. Use `?` operator throughout. Never silently swallow errors at system boundaries — always log or propagate.

### Module Visibility

`pub mod` for module declarations, `pub fn` for public functions, never `pub` on struct fields (use getters or derive Serialize/Deserialize for DTOs). Public types in domain modules, private implementation details in command/repo functions.

### Transaction Patterns

- `pool.begin().await` for standard transactions (deferrable)
- `BEGIN IMMEDIATE` for refunds (acquires write lock upfront to prevent double-refund race)
- Always `tx.commit().await` in success path, automatic rollback on drop
- Post-commit work (loyalty points, stock movements) done after commit — best-effort, errors logged not propagated

---

## 19. KEY ARCHITECTURAL DECISIONS

1. **Integer money (minor units):** All monetary values stored as i64 in fils (BHD) or cents. Zero floating-point in financial calculations. Eliminates rounding errors. BHD exponent=3 means 1.500 BHD = 1500 fils.

2. **Server-side tax recalculation:** Client tax values in CartLine are advisory only. The server recomputes all taxes during finalize_sale using confirmed prices and tax rules. Prevents client-side manipulation.

3. **Sequential receipt per device:** Each device has its own `next_receipt_seq` counter, atomically incremented via `UPDATE ... RETURNING`. Format: `{branch_code}-{device_code}-{seq:08}`. Never reused even on void.

4. **Dual SSE paths:** Anthropic uses native SSE line protocol parsing (`event:`/`data:` lines). OpenAI uses standard `data: [DONE]` JSON streaming. Both converge into the same `StreamEvent` enum for frontend consumption.

5. **No router library:** Manual view state machine (`type View` union) avoids React Router overhead in a desktop app with only 5 views and no URL-based navigation.

6. **Single ActiveModal union type:** 18-variant discriminated union ensures only one modal is open at a time, preventing z-index wars and focus management issues.

7. **SQLite quantity_on_hand as TEXT:** Inventory quantities stored as strings to preserve precision across decimal arithmetic, avoiding SQLite REAL floating-point drift. Computed as f64 in Rust, formatted to 4 decimal places with trailing zeros stripped.

8. **FK-safe sync push ordering:** Tables are pushed in dependency order (branches first, cash_events last) so the receiving Hub can resolve foreign keys during LWW/append-only merge.

9. **Insert-or-Ignore for append-only tables:** Sales, payments, refunds, stock_movements, and audit_logs use `INSERT OR IGNORE` (not ON CONFLICT UPDATE) because these are immutable events — once created, never modified remotely.

10. **Loyalty points use MAX merge:** During customer sync, `loyalty_points = MAX(local, remote)` prevents a terminal from overwriting points earned on another device.

11. **PNG QR code data URL:** QR code rendered server-side by the sidecar's `qrcode` npm package, returned as `data:image/png;base64,...` in the `/status` response. Avoids Canvas/web crypto dependency in Tauri webview.

12. **Tesseract.js for offline OCR:** Payment screenshot verification uses local OCR (not cloud API), eliminating latency and privacy concerns. English LSTM model cached in `wa-session/tessdata/`.

13. **Windows Job Object for sidecar cleanup:** The sidecar process is bound to a Windows Job Object so that if the POS crashes, the OS kills the child process automatically — no orphaned Node.js processes.

14. **Argon2id with legacy PLAIN migration:** Existing PINs stored as `PLAIN:0000` are re-hashed on startup. Runtime rejects any `PLAIN:` prefix. This allows gradual migration without forcing all users to reset PINs.

15. **override_used audit flag:** Cross-device refunds record whether a manager override was used, enabling compliance auditing without blocking legitimate customer service.

---

## 20. IMPLEMENTATION ORDER

### Phase 1 — Scaffold + Database (files to create)
- `Cargo.toml` with all dependencies
- `src-tauri/build.rs` (tauri_build::build())
- `src-tauri/src/main.rs` (call run())
- `src-tauri/src/lib.rs` (Tauri builder shell, logging setup)
- `src-tauri/src/errors/mod.rs` (AppError enum)
- `src-tauri/src/db/mod.rs` (init_db with pool config, migration runner)
- `src-tauri/src/db/helpers.rs` (active_branch_id, active_device_id)
- `src-tauri/migrations/` — All 21 SQL migrations in order
- `src-tauri/src/domain/mod.rs` — Module declarations
- `src-tauri/src/domain/auth.rs` (UserSummary, SessionUser)
- `src-tauri/src/domain/product.rs` (Product, ProductWithPrice, TaxRule)
- `src-tauri/src/domain/money.rs` (all 7 functions with tests)
- `src-tauri/src/domain/cart.rs` (Cart, CartLine with tests)
- `src-tauri/src/domain/sale.rs` (PaymentInput, SaleResult, etc.)
- `src-tauri/src/domain/shift.rs` (Shift)
- `src-tauri/src/domain/refund.rs` (RefundItemInput, etc.)
- `src-tauri/src/domain/delivery.rs` (DeliveryInput, etc.)
- `package.json`, `tsconfig.json`, `vite.config.ts`, `index.html`
- `src/main.tsx`, `src/App.tsx`, `src/App.css` (minimal)
- `src/types.ts` (all TypeScript interfaces)

### Phase 2 — Auth + POS Core
- `src-tauri/src/db/repositories/auth_repo.rs` (Argon2id PIN, lockout)
- `src-tauri/src/commands/rbac.rs` (role checks)
- `src-tauri/src/commands/auth_commands.rs` (login, verify PIN)
- `src-tauri/src/commands/setup_commands.rs` (wizard, currency_exponent)
- `src-tauri/src/db/repositories/product_repo.rs` (PRODUCT_QUERY)
- `src-tauri/src/db/repositories/shift_repo.rs` (open/close shift)
- `src-tauri/src/db/repositories/sale_repo.rs` (finalize_sale, next_receipt_number)
- `src-tauri/src/db/repositories/held_cart_repo.rs`
- `src-tauri/src/db/repositories/refund_repo.rs`
- `src-tauri/src/db/repositories/report_repo.rs`
- `src-tauri/src/db/repositories/delivery_repo.rs`
- `src-tauri/src/commands/pos_commands.rs` (all cart + sale commands)
- `src-tauri/src/commands/product_commands.rs`
- `src-tauri/src/commands/shift_commands.rs`
- `src-tauri/src/commands/refund_commands.rs`
- `src-tauri/src/commands/held_cart_commands.rs`
- `src-tauri/src/commands/override_token.rs`
- `src/pages/LoginScreen.tsx`, `src/pages/PosPage.tsx`
- `src/stores/cartStore.ts`, `src/stores/scanStore.ts`
- `src/services/currency.ts`, `src/services/printUtils.ts`
- `src/utils/receiptLines.ts`

### Phase 3 — Products + Inventory
- `src-tauri/src/db/repositories/product_dedup_repo.rs`
- `src-tauri/src/commands/admin_commands.rs` (all 23 commands)
- `src-tauri/src/commands/inventory_commands.rs`
- `src-tauri/src/commands/catalog_import_commands.rs`
- `src-tauri/src/commands/customer_commands.rs`
- `src/components/ProductsTab.tsx`, `CategoriesTab.tsx`, etc.
- `src/components/InventoryTab.tsx`, `CustomersTab.tsx`

### Phase 4 — Hardware
- `src-tauri/src/commands/thermal_commands.rs` (ESC/POS printing, cash drawer)
- `src-tauri/src/commands/cash_commands.rs`
- `src-tauri/src/commands/ghost_barcode_commands.rs`
- `src-tauri/src/commands/receipt_pdf.rs`
- `src/components/settings/PrinterTab.tsx`

### Phase 5 — Sync + Hub
- `src-tauri/src/db/repositories/sync_repo.rs`
- `src-tauri/src/db/repositories/audit_hash.rs`
- `src-tauri/src/sync_v2/` (all 4 files)
- `src-tauri/src/hub/` (all 2 files)
- `src-tauri/src/commands/sync_commands.rs`
- `src-tauri/src/commands/hub_commands.rs`
- `src-tauri/src/commands/phase10a_commands.rs` (audit, backup, tax reports)
- `src-tauri/src/secure_store.rs`
- `src/components/settings/HubTab.tsx`, `SyncQueueModal.tsx`
- `src/components/SyncChip.tsx`

### Phase 6 — Reports + Settings
- `src-tauri/src/commands/report_commands.rs`
- `src-tauri/src/commands/device_commands.rs`
- `src-tauri/src/commands/delivery_commands.rs`
- `src-tauri/src/commands/updater_commands.rs`
- `src/components/ReportsTab.tsx`, `CashierReportTab.tsx`, `EodCashupTab.tsx`
- `src/components/SettingsTab.tsx` (all sub-tabs)
- `src/components/DeliveriesTab.tsx`, `DeliveryForm.tsx`
- `src/components/DevicesTab.tsx`, `AuditLogTab.tsx`, `UsersTab.tsx`

### Phase 7 — WhatsApp Integration
- `src-tauri/sidecar/whatsapp-sidecar/` — server.mjs, package.json, all Baileys logic
- `src-tauri/src/commands/whatsapp_commands.rs`
- `src-tauri/src/commands/whatsapp_inbox_commands.rs`
- `src-tauri/src/commands/payment_confirm_commands.rs`
- `src-tauri/src/db/repositories/` — wa_messages, payment_confirmations tables
- `src/components/WhatsAppQRModal.tsx`, `WhatsAppStatusPill.tsx`
- `src/components/settings/WhatsAppSection.tsx`
- `src/utils/waMessageFormat.ts`

### Phase 8 — Migration Agent
- `src-tauri/src/commands/migration_commands.rs`
- `src/pages/MigrationAgentPage.tsx` (all 5 sub-components)

### Phase 9 — AI Engine + Proactive
- `src-tauri/src/ai/` (all 18 files)
- `src-tauri/src/db/repositories/ai_admin_repo.rs`
- `src-tauri/src/db/repositories/ai_chat_history_repo.rs`
- `src-tauri/src/db/repositories/proactive_repo.rs`
- `src-tauri/src/commands/ai_admin_commands.rs`
- `src-tauri/src/domain/ai_admin.rs` (StreamEvent, ProviderConfig, etc.)
- `src/officeai/` (all components: ChatPanel, KpiSidebar, CopilotDock, RunPanel, etc.)
- `src/officeai/useChatController.ts`
- `src/hooks/useAutoLock.ts`, `useCountdown.ts`, `useDebouncedValue.ts`, `useFocusTrap.ts`, `useOnlineStatus.ts`, `useTheme.ts`
- `src/i18n/` (en.json, ar.json, index.ts)
- `src/services/cartPersistence.ts`
- `src/utils/stickyNotes.ts`
