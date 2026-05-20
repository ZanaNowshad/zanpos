# ZANPOS — Sub-project C: WhatsApp Integration (Baileys Sidecar)

**Date:** 2026-05-20  
**Status:** Approved  
**Market:** Bahrain (BHD, E.164 +973)

---

## Overview

Send a single bilingual (English + Arabic) WhatsApp message to the customer's contact number automatically when a delivery sale is confirmed at checkout. The integration uses a bundled Baileys Node.js sidecar — no Meta approval, no per-message cost, no external infrastructure required.

---

## Trigger

One message fires per delivery sale, at the moment `onConfirm` completes in `PaymentModal` and the sale result is returned with `is_delivery = true`. No messages on status changes, cancellations, or non-delivery sales.

---

## Architecture

### Sidecar (`src-tauri/sidecar/whatsapp-sidecar/`)

A small Node.js Express HTTP server built with [Baileys](https://github.com/WhiskeySockets/Baileys). Compiled to a single self-contained binary via `pkg` — no Node.js installation required on the shop PC.

**Endpoints:**

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/status` | `{ connected: bool, qr?: string }` — `qr` is a base64 PNG when a QR is pending |
| `POST` | `/send` | `{ to: "+97333050666", message: "..." }` → `{ ok: bool }` |
| `POST` | `/disconnect` | Logs out and clears session |

**Session persistence:** Baileys auth credentials saved to the Tauri app data directory. Path passed as a CLI argument at sidecar launch (e.g. `--session-dir C:\Users\...\AppData\Roaming\zanpos\wa-session`). QR scan is one-time; subsequent app launches reconnect automatically.

**Port:** `3131` (hardcoded; localhost only).

**Process lifecycle:** Sidecar is started in the Tauri `setup` hook (`lib.rs`) via `Command::new_sidecar("whatsapp-sidecar")`. It is terminated on app exit via the `on_window_event` handler. If the sidecar crashes, Tauri does not auto-restart it — a reconnect attempt is made on next `whatsapp_status` call from the frontend.

### Rust (`src-tauri/src/commands/whatsapp_commands.rs`)

Four Tauri commands proxying to the sidecar over `reqwest` (async, tokio-native — consistent with Tauri's async runtime):

| Command | RBAC | Proxies to |
|---------|------|------------|
| `whatsapp_status` | any role | `GET /status` |
| `whatsapp_get_qr` | any role | `GET /status` (returns `qr` field) |
| `whatsapp_send(to, message)` | any role | `POST /send` |
| `whatsapp_disconnect(actor_user_id)` | manager_or_owner | `POST /disconnect` |

Errors from the sidecar (not running, HTTP error) are caught and returned as `AppError::Validation` — they never block the sale.

### Frontend

New files:
- `src/components/WhatsAppStatusPill.tsx` — POS header indicator
- `src/components/WhatsAppQRModal.tsx` — QR scan modal (shared between Settings and auto-trigger)

Modified files:
- `src/pages/PosPage.tsx` — render `WhatsAppStatusPill` in header; after delivery sale confirmed, call send logic
- `src/components/BackOfficeModal.tsx` — WhatsApp section in Settings tab
- Store setup wizard — new "WhatsApp & Payments" step
- `src/tauri/commands.ts` — 4 new command wrappers
- `src/types.ts` — `WhatsAppStatus` interface
- `src/App.css` — pill + QR modal styles

---

## Message Template

Sent to `delivery.contact_number` (already E.164 normalized). All fields sourced from the confirmed `SaleResult` + `DeliveryInput` + `store_config`.

```
🛵 *Your delivery order is confirmed!*

📋 Order: #{{RECEIPT_NUMBER}}
💰 Total: BHD {{NET_TOTAL}}
📍 Address: {{ADDRESS}}
🏠 {{HOUSE_NUMBER}}, {{AREA}}

💳 Please send payment via BenefitPay to:
    *{{BENEFIT_NUMBER}}*
📸 Share the receipt screenshot to confirm payment.

---

🛵 *تم تأكيد طلب التوصيل الخاص بك!*

📋 الطلب: #{{RECEIPT_NUMBER}}
💰 الإجمالي: BHD {{NET_TOTAL}}
📍 العنوان: {{ADDRESS}}
🏠 {{HOUSE_NUMBER}}، {{AREA}}

💳 يرجى إرسال الدفع عبر BenefitPay إلى:
    *{{BENEFIT_NUMBER}}*
📸 شارك صورة الإيصال لتأكيد الدفع.

---
_{{STORE_NAME}} • {{STORE_PHONE}}_
شكراً لطلبك — Thank you 🙏
```

**House/Area line:** omitted if both fields are empty.  
**Template builder:** a pure Rust function `build_delivery_whatsapp_message(params)` in `whatsapp_commands.rs` — no DB calls, fully testable.

---

## Store Config Fields

Two new keys in the existing `store_config` key/value table:

| Key | Example | Required |
|-----|---------|----------|
| `whatsapp_benefit_number` | `33050666` | Yes — collected at store setup |
| `store_phone` | `+97317001234` | Yes — collected at store setup (if not already present) |

`STORE_NAME` comes from the existing `branches.name` column.

---

## QR Connection Flow & UI

### POS Header — `WhatsAppStatusPill`

A small pill rendered in the POS header next to the shift indicator:

- 🟢 **WA** — connected
- 🔴 **WA** — disconnected (clickable for manager/owner → opens `WhatsAppQRModal`)
- ⟳ **WA** — connecting / QR pending

Cashiers see the pill read-only (no click action). Polling interval: every 30 seconds via `setInterval` + `whatsapp_status`.

### `WhatsAppQRModal`

Displays the base64 QR PNG from `/status`. Auto-refreshes every 20 seconds (Baileys QR TTL is ~30s). Closes automatically once `connected = true`. Used in two places:
1. Back Office → Settings → WhatsApp section (manual connect)
2. Auto-triggered at checkout when delivery confirmed + disconnected + manager/owner logged in

### Back Office → Settings Tab — WhatsApp Section

- Connection status badge
- **Connect** button → opens `WhatsAppQRModal`
- **Disconnect** button (manager/owner only)
- `benefit_number` input field → saved to `store_config`

### Store Setup Wizard — "WhatsApp & Payments" Step

New step inserted after the branch/device configuration step:

1. **BenefitPay Number** input (required) — saved to `store_config.whatsapp_benefit_number`
2. **Store Phone** input (optional if already set)
3. **Connect WhatsApp** section — shows QR inline, with a prominent "Skip for now" button
4. Skipping is non-blocking — the wizard advances normally

---

## Runtime Send Flow (at checkout)

```
Delivery sale confirmed
        │
        ▼
whatsapp_status called
        │
   ┌────┴─────┐
connected?   not connected
   │              │
   ▼         manager/owner?
whatsapp_send    ├── yes → open WhatsAppQRModal
   │             │         after QR scanned → whatsapp_send
   ▼             └── no  → warning toast, sale completes
success toast            "WhatsApp not connected — message not sent"
"📱 WhatsApp sent to +973XXXXXXXX"
```

Errors from `whatsapp_send` (sidecar unreachable, send failure) show a warning toast but never block the sale or trigger a retry.

---

## Build Process

1. `cd src-tauri/sidecar/whatsapp-sidecar && npm install`
2. `npx pkg . --target node18-win-x64 --output ../../../src-tauri/binaries/whatsapp-sidecar-x86_64-pc-windows-msvc.exe`
3. Register in `tauri.conf.json` under `tauri.bundle.externalBin`
4. CI/CD: sidecar build step runs before `tauri build`

The binary name follows Tauri's sidecar naming convention: `{name}-{target-triple}`.

---

## Error Handling Summary

| Scenario | Behaviour |
|----------|-----------|
| Sidecar not started / crashed | `whatsapp_status` returns `{ connected: false }` — treated as disconnected |
| Send fails (HTTP error) | Warning toast, sale unaffected |
| QR expires before scan | Modal auto-refreshes QR every 20s |
| Session invalidated by WhatsApp | Status shows disconnected; manager re-scans |
| No `contact_number` on delivery | Send is skipped silently (should not happen — required field) |
| `benefit_number` not configured | Send is skipped; Settings shows configuration prompt |

---

## Out of Scope

- Messages on status changes (out for delivery, delivered) — Sub-project C strictly covers order confirmation only
- Message delivery receipts / read receipts
- Inbound WhatsApp messages
- Multi-device WhatsApp (separate future topic)
- iOS / macOS sidecar binary (Windows 11 only for now)
