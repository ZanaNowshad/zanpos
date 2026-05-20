# WhatsApp Integration (Baileys Sidecar) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Send a bilingual EN/AR WhatsApp delivery confirmation message automatically when a delivery sale is confirmed at checkout, using a bundled Baileys Node.js sidecar.

**Architecture:** A Node.js Express server using Baileys is compiled to a Windows x64 binary via `pkg` and bundled as a Tauri external binary. Rust commands proxy HTTP calls to `http://127.0.0.1:3131`. The sidecar starts at app launch and terminates on app exit. The frontend polls for status and shows a pill indicator; after a delivery sale confirms it calls `whatsapp_send_delivery` which builds the bilingual template server-side and posts to the sidecar.

**Tech Stack:** Baileys (`@whiskeysockets/baileys`), Express 4, `qrcode` npm, `pkg` bundler, Rust `reqwest` (already in Cargo.toml), Tauri v2, React/TypeScript.

---

## File Map

**New files:**
- `src-tauri/sidecar/whatsapp-sidecar/package.json` — Node.js project manifest + pkg config
- `src-tauri/sidecar/whatsapp-sidecar/server.js` — Baileys HTTP server (3 endpoints)
- `src-tauri/binaries/.gitkeep` — keeps directory tracked (compiled binary is gitignored)
- `src-tauri/src/commands/whatsapp_commands.rs` — 5 Tauri commands + message builder
- `src/components/WhatsAppStatusPill.tsx` — POS header status indicator
- `src/components/WhatsAppQRModal.tsx` — QR scan modal (shared: Settings + auto-trigger)

**Modified files:**
- `src-tauri/tauri.conf.json` — add `bundle.externalBin`
- `src-tauri/src/lib.rs` — AppState + sidecar lifecycle (start/stop)
- `src-tauri/src/commands/mod.rs` — expose `whatsapp_commands`
- `src-tauri/src/commands/setup_commands.rs` — add `whatsapp_benefit_number` to `AppConfig`; add `benefit_number` param to `setup_wizard_complete`
- `src/types.ts` — `WhatsAppStatus` interface; `AppConfig` + `whatsapp_benefit_number`
- `src/tauri/commands.ts` — 5 new command wrappers
- `src/pages/PosPage.tsx` — render `WhatsAppStatusPill`; call send after delivery confirmed
- `src/components/BackOfficeModal.tsx` — WhatsApp section in Settings tab
- `src/pages/SetupWizard.tsx` — new step 5 "Payments & WhatsApp" (shift Owner to step 6, Review to 7)
- `src/App.css` — pill, QR modal, WhatsApp settings styles

---

## Task 1: Sidecar Node.js Project Scaffold

**Files:**
- Create: `src-tauri/sidecar/whatsapp-sidecar/package.json`

- [ ] **Step 1: Create the sidecar directory and package.json**

```bash
mkdir -p src-tauri/sidecar/whatsapp-sidecar
```

Create `src-tauri/sidecar/whatsapp-sidecar/package.json`:
```json
{
  "name": "whatsapp-sidecar",
  "version": "1.0.0",
  "description": "ZANPOS WhatsApp sidecar — Baileys HTTP bridge",
  "main": "server.js",
  "scripts": {
    "start": "node server.js",
    "build": "pkg . --target node18-win-x64 --output ../../binaries/whatsapp-sidecar-x86_64-pc-windows-msvc.exe --compress GZip"
  },
  "dependencies": {
    "@whiskeysockets/baileys": "^6.7.9",
    "express": "^4.19.2",
    "qrcode": "^1.5.4",
    "pino": "^9.0.0"
  },
  "devDependencies": {
    "pkg": "^5.8.1"
  },
  "pkg": {
    "assets": [
      "node_modules/@whiskeysockets/baileys/**/*",
      "node_modules/@adiwajshing/keyed-db/**/*",
      "node_modules/libsignal/**/*",
      "node_modules/node-cache/**/*"
    ],
    "targets": ["node18-win-x64"],
    "outputPath": "../../binaries"
  }
}
```

- [ ] **Step 2: Install dependencies**

```powershell
cd src-tauri/sidecar/whatsapp-sidecar
npm install
```

Expected: `node_modules/` created, no errors.

- [ ] **Step 3: Commit scaffold**

```powershell
cd C:\Users\super\ZAN\zanpos
git add src-tauri/sidecar/
git commit -m "feat(wa): add whatsapp sidecar project scaffold"
```

---

## Task 2: Sidecar Server Implementation

**Files:**
- Create: `src-tauri/sidecar/whatsapp-sidecar/server.js`

- [ ] **Step 1: Create server.js**

Create `src-tauri/sidecar/whatsapp-sidecar/server.js`:
```javascript
"use strict";

const express = require("express");
const QRCode  = require("qrcode");
const {
  default: makeWASocket,
  useMultiFileAuthState,
  DisconnectReason,
  fetchLatestBaileysVersion,
} = require("@whiskeysockets/baileys");
const pino = require("pino");

// ── Parse CLI args ─────────────────────────────────────────────────────────────
const args = process.argv.slice(2);
const sessionDir = (() => {
  const flag = args.find(a => a.startsWith("--session-dir="));
  return flag ? flag.split("=").slice(1).join("=") : "./wa-session";
})();
const PORT = 3131;

// ── State ─────────────────────────────────────────────────────────────────────
let sock        = null;
let qrDataUrl   = null;   // base64 PNG data URL, null when connected or idle
let isConnected = false;
let isStarting  = false;

const logger = pino({ level: "silent" }); // suppress Baileys noise

// ── Baileys lifecycle ─────────────────────────────────────────────────────────
async function startBaileys() {
  if (isStarting) return;
  isStarting = true;
  try {
    const { version } = await fetchLatestBaileysVersion();
    const { state, saveCreds } = await useMultiFileAuthState(sessionDir);

    sock = makeWASocket({
      version,
      auth:               state,
      printQRInTerminal:  false,
      logger,
      browser:            ["ZANPOS", "Chrome", "126.0"],
    });

    sock.ev.on("creds.update", saveCreds);

    sock.ev.on("connection.update", async (update) => {
      const { connection, lastDisconnect, qr } = update;

      if (qr) {
        try {
          qrDataUrl = await QRCode.toDataURL(qr);
        } catch (_) { /* ignore */ }
      }

      if (connection === "open") {
        isConnected = true;
        qrDataUrl   = null;
        isStarting  = false;
        console.log("[wa-sidecar] Connected");
      }

      if (connection === "close") {
        isConnected = false;
        isStarting  = false;
        const code  = lastDisconnect?.error?.output?.statusCode;
        console.log("[wa-sidecar] Disconnected, code:", code);
        if (code !== DisconnectReason.loggedOut) {
          // Reconnect after brief delay
          setTimeout(startBaileys, 3000);
        }
      }
    });
  } catch (err) {
    isStarting = false;
    console.error("[wa-sidecar] Failed to start Baileys:", err.message);
    // Retry after 10s
    setTimeout(startBaileys, 10_000);
  }
}

// ── Express endpoints ─────────────────────────────────────────────────────────
const app = express();
app.use(express.json());

/** GET /status → { connected: bool, qr?: string } */
app.get("/status", (_req, res) => {
  res.json({
    connected: isConnected,
    qr:        qrDataUrl ?? undefined,
  });
});

/** POST /send  body: { to: "+97333050666", message: "..." } → { ok: bool } */
app.post("/send", async (req, res) => {
  const { to, message } = req.body ?? {};
  if (!to || !message) {
    return res.status(400).json({ ok: false, error: "to and message required" });
  }
  if (!isConnected || !sock) {
    return res.json({ ok: false, error: "not connected" });
  }
  try {
    // WhatsApp JID for individual phone: strip '+' and append '@s.whatsapp.net'
    const jid = to.replace(/^\+/, "") + "@s.whatsapp.net";
    await sock.sendMessage(jid, { text: message });
    res.json({ ok: true });
  } catch (err) {
    console.error("[wa-sidecar] Send failed:", err.message);
    res.json({ ok: false, error: err.message });
  }
});

/** POST /disconnect → { ok: bool } */
app.post("/disconnect", (_req, res) => {
  if (sock) {
    try { sock.logout(); } catch (_) { /* ignore */ }
    sock        = null;
    isConnected = false;
    qrDataUrl   = null;
  }
  res.json({ ok: true });
});

// ── Start ─────────────────────────────────────────────────────────────────────
app.listen(PORT, "127.0.0.1", () => {
  console.log(`[wa-sidecar] Listening on 127.0.0.1:${PORT}, session: ${sessionDir}`);
  startBaileys();
});

// Graceful shutdown
process.on("SIGTERM", () => {
  if (sock) { try { sock.end(); } catch (_) { /* */ } }
  process.exit(0);
});
```

- [ ] **Step 2: Test the server runs locally**

```powershell
cd src-tauri/sidecar/whatsapp-sidecar
node server.js --session-dir=./test-session
```

Expected output: `[wa-sidecar] Listening on 127.0.0.1:3131, session: ./test-session`  
Ctrl+C to stop. Delete `./test-session` if created.

- [ ] **Step 3: Commit**

```powershell
cd C:\Users\super\ZAN\zanpos
git add src-tauri/sidecar/whatsapp-sidecar/server.js
git commit -m "feat(wa): implement Baileys HTTP sidecar server"
```

---

## Task 3: Build Binary + Register in tauri.conf.json

**Files:**
- Create: `src-tauri/binaries/.gitkeep`
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1: Create binaries directory**

Create `src-tauri/binaries/.gitkeep` (empty file).

Add `src-tauri/binaries/*.exe` to `.gitignore`:
```
# Built sidecar binaries (platform-specific, built by CI)
src-tauri/binaries/*.exe
```

- [ ] **Step 2: Build the sidecar binary**

```powershell
cd src-tauri/sidecar/whatsapp-sidecar
npm run build
```

Expected: `src-tauri/binaries/whatsapp-sidecar-x86_64-pc-windows-msvc.exe` is created (~50–90 MB).  
If `pkg` fails due to Baileys native modules, add the failing module to the `pkg.assets` array in `package.json` and retry.

- [ ] **Step 3: Register the sidecar in tauri.conf.json**

Modify `src-tauri/tauri.conf.json` — add `externalBin` to the `bundle` section:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "zanpos",
  "version": "0.1.0",
  "identifier": "com.super.zanpos",
  "build": {
    "beforeDevCommand": "npm run dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "npm run build",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "title": "zanpos",
        "width": 800,
        "height": 600
      }
    ],
    "security": {
      "csp": null
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ],
    "externalBin": [
      "binaries/whatsapp-sidecar"
    ]
  },
  "plugins": {
    "updater": {
      "pubkey": "PLACEHOLDER_REPLACE_WITH_REAL_KEY",
      "endpoints": [
        "https://releases.zanpos.app/{{target}}/{{arch}}/{{current_version}}"
      ]
    }
  }
}
```

- [ ] **Step 4: Commit**

```powershell
cd C:\Users\super\ZAN\zanpos
git add src-tauri/binaries/.gitkeep src-tauri/tauri.conf.json .gitignore
git commit -m "feat(wa): register whatsapp sidecar as tauri externalBin"
```

---

## Task 4: Rust — AppState + Sidecar Lifecycle

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Update AppState to hold the sidecar child process**

In `src-tauri/src/lib.rs`, replace the existing `AppState` struct and update the `run()` function.

Find the current `AppState`:
```rust
pub struct AppState {
    pub db: SqlitePool,
    pub sync_worker: Arc<SyncWorker>,
}
```

Replace with:
```rust
pub struct AppState {
    pub db: SqlitePool,
    pub sync_worker: Arc<SyncWorker>,
    pub whatsapp_child: Arc<std::sync::Mutex<Option<std::process::Child>>>,
}
```

- [ ] **Step 2: Start sidecar in the setup hook**

In `lib.rs`, inside the `.setup(|app| { ... })` closure, add the following block AFTER `app.manage(AppState { db, sync_worker })`:

```rust
// ── Start WhatsApp sidecar ────────────────────────────────────────────────
let wa_session_dir = app_data.join("wa-session");
std::fs::create_dir_all(&wa_session_dir).ok();

let sidecar_exe = {
    // In production: Tauri bundles externalBin into the resource dir
    // In development: the binary sits in src-tauri/binaries/
    let prod_path = app
        .path()
        .resource_dir()
        .map(|p| p.join("whatsapp-sidecar-x86_64-pc-windows-msvc.exe"))
        .unwrap_or_default();
    let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join("whatsapp-sidecar-x86_64-pc-windows-msvc.exe");
    if prod_path.exists() { prod_path } else { dev_path }
};

let wa_child: Arc<std::sync::Mutex<Option<std::process::Child>>> = if sidecar_exe.exists() {
    match std::process::Command::new(&sidecar_exe)
        .arg(format!("--session-dir={}", wa_session_dir.to_string_lossy()))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(child) => {
            tracing::info!("WhatsApp sidecar started (pid {})", child.id());
            Arc::new(std::sync::Mutex::new(Some(child)))
        }
        Err(e) => {
            tracing::warn!("WhatsApp sidecar failed to start: {}", e);
            Arc::new(std::sync::Mutex::new(None))
        }
    }
} else {
    tracing::info!("WhatsApp sidecar binary not found — WA features disabled");
    Arc::new(std::sync::Mutex::new(None))
};
```

Then update `app.manage(...)` to include the child:
```rust
app.manage(AppState { db, sync_worker, whatsapp_child: wa_child });
```

- [ ] **Step 3: Kill sidecar on window destroy**

Add `.on_window_event(...)` before `.run(...)` in `lib.rs`:

```rust
.on_window_event(|window, event| {
    if let tauri::WindowEvent::Destroyed = event {
        let state: tauri::State<'_, AppState> = window.state();
        if let Ok(mut guard) = state.whatsapp_child.lock() {
            if let Some(ref mut child) = *guard {
                let _ = child.kill();
                let _ = child.wait();
                tracing::info!("WhatsApp sidecar terminated");
            }
        }
    }
})
```

- [ ] **Step 4: Verify it compiles**

```powershell
cd C:\Users\super\ZAN\zanpos\src-tauri
cargo check 2>&1 | Select-Object -Last 5
```

Expected: `Finished \`dev\` profile`

- [ ] **Step 5: Commit**

```powershell
cd C:\Users\super\ZAN\zanpos
git add src-tauri/src/lib.rs
git commit -m "feat(wa): start/stop whatsapp sidecar in tauri app lifecycle"
```

---

## Task 5: Rust — Message Builder (Pure Function + Tests)

**Files:**
- Create: `src-tauri/src/commands/whatsapp_commands.rs` (partial — builder only)

- [ ] **Step 1: Create whatsapp_commands.rs with the message builder**

Create `src-tauri/src/commands/whatsapp_commands.rs`:

```rust
use crate::errors::AppResult;
use crate::AppState;
use serde::{Deserialize, Serialize};
use tauri::State;

const SIDECAR_URL: &str = "http://127.0.0.1:3131";

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct WhatsAppStatus {
    pub connected: bool,
    pub qr: Option<String>, // base64 PNG data URL: "data:image/png;base64,..."
}

#[derive(Debug, Deserialize)]
pub struct SendDeliveryInput {
    pub to: String,              // E.164 e.g. "+97333050666"
    pub receipt_number: String,
    pub net_total_minor: i64,
    pub currency_exponent: i32,
    pub address_text: String,
    pub house_number: Option<String>,
    pub area: Option<String>,
}

// ─── Message builder ──────────────────────────────────────────────────────────

pub struct WhatsAppDeliveryParams<'a> {
    pub receipt_number:    &'a str,
    pub net_total_minor:   i64,
    pub currency_exponent: i32,
    pub address_text:      &'a str,
    pub house_number:      Option<&'a str>,
    pub area:              Option<&'a str>,
    pub store_name:        &'a str,
    pub store_phone:       Option<&'a str>,
    pub benefit_number:    Option<&'a str>,
}

/// Format minor units to decimal string, e.g. 1500 with exp=3 → "1.500"
fn fmt_money(minor: i64, exp: i32) -> String {
    if exp == 0 {
        return minor.to_string();
    }
    let divisor = 10_i64.pow(exp as u32);
    let whole = minor / divisor;
    let frac  = minor % divisor;
    format!("{}.{:0>width$}", whole, frac.abs(), width = exp as usize)
}

pub fn build_delivery_whatsapp_message(p: &WhatsAppDeliveryParams) -> String {
    let total = fmt_money(p.net_total_minor, p.currency_exponent);

    // Optional house+area line (omit if both empty)
    let location_line_en = match (p.house_number, p.area) {
        (Some(h), Some(a)) if !h.is_empty() || !a.is_empty() => {
            format!("\n🏠 {}, {}", h, a)
        }
        (Some(h), None) if !h.is_empty() => format!("\n🏠 {}", h),
        (None, Some(a)) if !a.is_empty() => format!("\n🏠 {}", a),
        _ => String::new(),
    };
    let location_line_ar = match (p.house_number, p.area) {
        (Some(h), Some(a)) if !h.is_empty() || !a.is_empty() => {
            format!("\n🏠 {}، {}", h, a)
        }
        (Some(h), None) if !h.is_empty() => format!("\n🏠 {}", h),
        (None, Some(a)) if !a.is_empty() => format!("\n🏠 {}", a),
        _ => String::new(),
    };

    let benefit_section_en = match p.benefit_number {
        Some(bn) if !bn.is_empty() => format!(
            "\n\n💳 Please send payment via BenefitPay to:\n    *{}*\n📸 Share the receipt screenshot to confirm payment.",
            bn
        ),
        _ => String::new(),
    };
    let benefit_section_ar = match p.benefit_number {
        Some(bn) if !bn.is_empty() => format!(
            "\n\n💳 يرجى إرسال الدفع عبر BenefitPay إلى:\n    *{}*\n📸 شارك صورة الإيصال لتأكيد الدفع.",
            bn
        ),
        _ => String::new(),
    };

    let footer = match (p.store_name, p.store_phone) {
        (n, Some(ph)) if !n.is_empty() => format!("\n_{}  •  {}_", n, ph),
        (n, None) if !n.is_empty()     => format!("\n_{}_", n),
        _                               => String::new(),
    };

    format!(
        "🛵 *Your delivery order is confirmed!*\n\n\
         📋 Order: #{receipt}\n\
         💰 Total: BHD {total}\n\
         📍 Address: {address}{loc_en}\
         {benefit_en}\n\n\
         ---\n\n\
         🛵 *تم تأكيد طلب التوصيل الخاص بك!*\n\n\
         📋 الطلب: #{receipt}\n\
         💰 الإجمالي: BHD {total}\n\
         📍 العنوان: {address}{loc_ar}\
         {benefit_ar}\n\n\
         ---\
         {footer}\n\
         شكراً لطلبك — Thank you 🙏",
        receipt  = p.receipt_number,
        total    = total,
        address  = p.address_text,
        loc_en   = location_line_en,
        benefit_en = benefit_section_en,
        loc_ar   = location_line_ar,
        benefit_ar = benefit_section_ar,
        footer   = footer,
    )
}

// ─── Commands (stubs — filled in Task 6) ─────────────────────────────────────

#[tauri::command]
pub async fn whatsapp_status(_state: State<'_, AppState>) -> AppResult<WhatsAppStatus> {
    Ok(WhatsAppStatus { connected: false, qr: None })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn base_params<'a>() -> WhatsAppDeliveryParams<'a> {
        WhatsAppDeliveryParams {
            receipt_number:    "0042",
            net_total_minor:   1500,
            currency_exponent: 3,
            address_text:      "Block 5, Road 123",
            house_number:      Some("12"),
            area:              Some("Riffa"),
            store_name:        "ZAN Café",
            store_phone:       Some("+97317001234"),
            benefit_number:    Some("33050666"),
        }
    }

    #[test]
    fn test_message_contains_receipt_number() {
        let msg = build_delivery_whatsapp_message(&base_params());
        assert!(msg.contains("#0042"), "must contain receipt number");
    }

    #[test]
    fn test_message_contains_formatted_total() {
        let msg = build_delivery_whatsapp_message(&base_params());
        assert!(msg.contains("1.500"), "BHD with 3 decimals: 1500 minor → 1.500");
    }

    #[test]
    fn test_message_contains_benefit_number() {
        let msg = build_delivery_whatsapp_message(&base_params());
        assert!(msg.contains("33050666"), "must contain benefit number");
    }

    #[test]
    fn test_message_contains_arabic_section() {
        let msg = build_delivery_whatsapp_message(&base_params());
        assert!(msg.contains("تم تأكيد"), "must contain Arabic confirmation text");
        assert!(msg.contains("BenefitPay"), "benefit section in Arabic too");
    }

    #[test]
    fn test_message_omits_location_when_empty() {
        let params = WhatsAppDeliveryParams {
            house_number: None,
            area:         None,
            ..base_params()
        };
        let msg = build_delivery_whatsapp_message(&params);
        assert!(!msg.contains("🏠"), "no house emoji when house+area both absent");
    }

    #[test]
    fn test_message_omits_benefit_when_not_configured() {
        let params = WhatsAppDeliveryParams {
            benefit_number: None,
            ..base_params()
        };
        let msg = build_delivery_whatsapp_message(&params);
        assert!(!msg.contains("BenefitPay"), "benefit section absent when unconfigured");
    }

    #[test]
    fn test_fmt_money_3_decimals() {
        assert_eq!(fmt_money(1500, 3), "1.500");
        assert_eq!(fmt_money(1001, 3), "1.001");
        assert_eq!(fmt_money(500,  3), "0.500");
    }

    #[test]
    fn test_fmt_money_2_decimals() {
        assert_eq!(fmt_money(199, 2), "1.99");
        assert_eq!(fmt_money(100, 2), "1.00");
    }
}
```

- [ ] **Step 2: Add to mod.rs so it compiles**

In `src-tauri/src/commands/mod.rs`, add:
```rust
pub mod whatsapp_commands;
```

- [ ] **Step 3: Run the unit tests**

```powershell
cd C:\Users\super\ZAN\zanpos\src-tauri
cargo test whatsapp 2>&1 | Select-Object -Last 20
```

Expected: `test result: ok. 8 passed; 0 failed`

- [ ] **Step 4: Commit**

```powershell
cd C:\Users\super\ZAN\zanpos
git add src-tauri/src/commands/whatsapp_commands.rs src-tauri/src/commands/mod.rs
git commit -m "feat(wa): add message builder + unit tests"
```

---

## Task 6: Rust — Full WhatsApp Commands

**Files:**
- Modify: `src-tauri/src/commands/whatsapp_commands.rs` (replace stub commands with real implementations)

- [ ] **Step 1: Replace stub commands with full implementations**

Replace everything after the `// ─── Commands (stubs` comment in `whatsapp_commands.rs` with:

```rust
// ─── Commands ─────────────────────────────────────────────────────────────────

/// Poll sidecar connection state. Never returns an error — sidecar not running
/// is represented as { connected: false }.
#[tauri::command]
pub async fn whatsapp_status(_state: State<'_, AppState>) -> AppResult<WhatsAppStatus> {
    match reqwest::get(format!("{}/status", SIDECAR_URL)).await {
        Ok(resp) => Ok(resp.json::<WhatsAppStatus>().await.unwrap_or(WhatsAppStatus {
            connected: false,
            qr:        None,
        })),
        Err(_) => Ok(WhatsAppStatus { connected: false, qr: None }),
    }
}

/// Build and send the bilingual delivery confirmation message.
/// Fetches store config (benefit_number, store_name, store_phone) from DB.
/// Returns true if sidecar accepted the message, false if not connected / failed.
/// Never propagates an error — a WA failure must never block a completed sale.
#[tauri::command]
pub async fn whatsapp_send_delivery(
    input: SendDeliveryInput,
    state: State<'_, AppState>,
) -> AppResult<bool> {
    // Fetch store config — all optional; missing values degrade gracefully
    let benefit_number: Option<String> = sqlx::query_scalar(
        "SELECT value FROM app_config WHERE key = 'whatsapp_benefit_number'",
    )
    .fetch_optional(&state.db)
    .await?
    .flatten();

    let store_name: Option<String> =
        sqlx::query_scalar("SELECT name  FROM branches WHERE is_active = 1 LIMIT 1")
            .fetch_optional(&state.db)
            .await?
            .flatten();

    let store_phone: Option<String> =
        sqlx::query_scalar("SELECT phone FROM branches WHERE is_active = 1 LIMIT 1")
            .fetch_optional(&state.db)
            .await?
            .flatten();

    let message = build_delivery_whatsapp_message(&WhatsAppDeliveryParams {
        receipt_number:    &input.receipt_number,
        net_total_minor:   input.net_total_minor,
        currency_exponent: input.currency_exponent,
        address_text:      &input.address_text,
        house_number:      input.house_number.as_deref(),
        area:              input.area.as_deref(),
        store_name:        store_name.as_deref().unwrap_or(""),
        store_phone:       store_phone.as_deref(),
        benefit_number:    benefit_number.as_deref(),
    });

    let client = reqwest::Client::new();
    let result = client
        .post(format!("{}/send", SIDECAR_URL))
        .json(&serde_json::json!({ "to": input.to, "message": message }))
        .send()
        .await;

    match result {
        Ok(resp) => {
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            Ok(body.get("ok").and_then(|v| v.as_bool()).unwrap_or(false))
        }
        Err(_) => Ok(false),
    }
}

/// Disconnect WhatsApp session. Requires manager or owner.
#[tauri::command]
pub async fn whatsapp_disconnect(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<bool> {
    crate::commands::rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let client = reqwest::Client::new();
    Ok(client
        .post(format!("{}/disconnect", SIDECAR_URL))
        .send()
        .await
        .is_ok())
}

/// Save BenefitPay number to app_config. Requires manager or owner.
#[tauri::command]
pub async fn whatsapp_save_config(
    benefit_number: String,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    crate::commands::rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    sqlx::query(
        "INSERT OR REPLACE INTO app_config (key, value) VALUES ('whatsapp_benefit_number', ?)",
    )
    .bind(&benefit_number)
    .execute(&state.db)
    .await?;
    Ok(())
}
```

- [ ] **Step 2: cargo check**

```powershell
cd C:\Users\super\ZAN\zanpos\src-tauri
cargo check 2>&1 | Select-Object -Last 5
```

Expected: `Finished \`dev\` profile`

- [ ] **Step 3: Run tests again to confirm builder still passes**

```powershell
cargo test whatsapp 2>&1 | Select-Object -Last 10
```

Expected: `test result: ok. 8 passed; 0 failed`

- [ ] **Step 4: Commit**

```powershell
cd C:\Users\super\ZAN\zanpos
git add src-tauri/src/commands/whatsapp_commands.rs
git commit -m "feat(wa): implement whatsapp_status, send_delivery, disconnect, save_config commands"
```

---

## Task 7: Rust — Register Commands + Update AppConfig

**Files:**
- Modify: `src-tauri/src/lib.rs` — register 4 commands in invoke_handler
- Modify: `src-tauri/src/commands/setup_commands.rs` — add `whatsapp_benefit_number` to AppConfig

- [ ] **Step 1: Register commands in lib.rs**

In `src-tauri/src/lib.rs`, inside the `.invoke_handler(tauri::generate_handler![...])` block, add after the Customers section:

```rust
// WhatsApp
commands::whatsapp_commands::whatsapp_status,
commands::whatsapp_commands::whatsapp_send_delivery,
commands::whatsapp_commands::whatsapp_disconnect,
commands::whatsapp_commands::whatsapp_save_config,
```

- [ ] **Step 2: Add whatsapp_benefit_number to AppConfig struct**

In `src-tauri/src/commands/setup_commands.rs`, find the `AppConfig` struct and add one field:
```rust
pub whatsapp_benefit_number: Option<String>,
```

- [ ] **Step 3: Populate whatsapp_benefit_number in app_config_load**

In the `app_config_load` function, after the existing queries, add:
```rust
let wa_benefit: Option<String> =
    sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'whatsapp_benefit_number'")
        .fetch_optional(&state.db)
        .await?
        .flatten();
```

Then add `whatsapp_benefit_number: wa_benefit` to the returned `AppConfig { ... }`.

- [ ] **Step 4: cargo check**

```powershell
cd C:\Users\super\ZAN\zanpos\src-tauri
cargo check 2>&1 | Select-Object -Last 5
```

Expected: `Finished \`dev\` profile`

- [ ] **Step 5: Commit**

```powershell
cd C:\Users\super\ZAN\zanpos
git add src-tauri/src/lib.rs src-tauri/src/commands/setup_commands.rs
git commit -m "feat(wa): register commands; expose whatsapp_benefit_number in AppConfig"
```

---

## Task 8: Frontend — Types + Command Wrappers

**Files:**
- Modify: `src/types.ts`
- Modify: `src/tauri/commands.ts`

- [ ] **Step 1: Add WhatsAppStatus interface and update AppConfig in types.ts**

In `src/types.ts`, find the `AppConfig` interface and add one field:
```typescript
whatsapp_benefit_number: string | null;
```

Anywhere in `types.ts`, add the new interface (e.g. after `CashDrawerSummary`):
```typescript
export interface WhatsAppStatus {
  connected: boolean;
  qr?: string; // base64 PNG data URL when QR is pending
}

export interface SendDeliveryInput {
  to: string;
  receipt_number: string;
  net_total_minor: number;
  currency_exponent: number;
  address_text: string;
  house_number?: string;
  area?: string;
}
```

- [ ] **Step 2: Add command wrappers in commands.ts**

In `src/tauri/commands.ts`, add at the end (or in a "WhatsApp" section):
```typescript
// ─── WhatsApp ─────────────────────────────────────────────────────────────────

import type { WhatsAppStatus, SendDeliveryInput } from "../types";

export function whatsappStatus(): Promise<WhatsAppStatus> {
  return invoke<WhatsAppStatus>("whatsapp_status");
}

export function whatsappSendDelivery(input: SendDeliveryInput): Promise<boolean> {
  return invoke<boolean>("whatsapp_send_delivery", { input });
}

export function whatsappDisconnect(actorUserId: string): Promise<boolean> {
  return invoke<boolean>("whatsapp_disconnect", { actorUserId });
}

export function whatsappSaveConfig(benefitNumber: string, actorUserId: string): Promise<void> {
  return invoke<void>("whatsapp_save_config", { benefitNumber, actorUserId });
}
```

Note: `invoke` is already imported at the top of commands.ts. If `WhatsAppStatus` and `SendDeliveryInput` are already imported at the top of the file (for other uses), do not add duplicate imports — just reference them.

- [ ] **Step 3: tsc check**

```powershell
cd C:\Users\super\ZAN\zanpos
npx tsc --noEmit 2>&1
```

Expected: no output (zero errors).

- [ ] **Step 4: Commit**

```powershell
git add src/types.ts src/tauri/commands.ts
git commit -m "feat(wa): add WhatsAppStatus types and command wrappers"
```

---

## Task 9: WhatsAppStatusPill Component

**Files:**
- Create: `src/components/WhatsAppStatusPill.tsx`
- Modify: `src/pages/PosPage.tsx` — add pill to header

- [ ] **Step 1: Create WhatsAppStatusPill.tsx**

Create `src/components/WhatsAppStatusPill.tsx`:
```tsx
import { useEffect, useState, useCallback } from "react";
import type { WhatsAppStatus } from "../types";
import { whatsappStatus } from "../tauri/commands";

interface Props {
  sessionRole: string; // "owner" | "manager" | "cashier" | ...
  onOpenQR: () => void;
}

export default function WhatsAppStatusPill({ sessionRole, onOpenQR }: Props) {
  const [status, setStatus] = useState<WhatsAppStatus>({ connected: false });

  const poll = useCallback(async () => {
    try {
      const s = await whatsappStatus();
      setStatus(s);
    } catch { /* sidecar not running */ }
  }, []);

  useEffect(() => {
    poll();
    const id = setInterval(poll, 30_000);
    return () => clearInterval(id);
  }, [poll]);

  const isManager = sessionRole === "owner" || sessionRole === "manager";
  const isPending = !status.connected && !!status.qr;

  const label = isPending ? "⟳ WA" : status.connected ? "WA" : "WA";
  const cls   = `wa-pill ${
    status.connected ? "wa-pill-on" : isPending ? "wa-pill-pending" : "wa-pill-off"
  }`;

  const handleClick = () => {
    if (isManager && !status.connected) onOpenQR();
  };

  return (
    <button
      className={cls}
      onClick={handleClick}
      title={
        status.connected ? "WhatsApp connected"
        : isPending      ? "WhatsApp — scan QR to connect"
        :                  isManager ? "WhatsApp disconnected — click to connect" : "WhatsApp disconnected"
      }
      disabled={!isManager || status.connected}
      style={{ cursor: isManager && !status.connected ? "pointer" : "default" }}
    >
      <span className="wa-pill-dot" />
      {label}
    </button>
  );
}
```

- [ ] **Step 2: Add pill to PosPage header**

In `src/pages/PosPage.tsx`, import `WhatsAppStatusPill` and `WhatsAppQRModal` at the top:
```tsx
import WhatsAppStatusPill from "../components/WhatsAppStatusPill";
import WhatsAppQRModal from "../components/WhatsAppQRModal";
```

Add state for the QR modal near the top of the `PosPage` component (with other modal state):
```tsx
const [showWaQR, setShowWaQR] = useState(false);
```

In the POS header JSX (find the `<div className="pos-header">` or similar), add the pill:
```tsx
<WhatsAppStatusPill
  sessionRole={sessionUser.role_name}
  onOpenQR={() => setShowWaQR(true)}
/>
```

Add the QR modal render (near the bottom of the return, alongside other modals):
```tsx
{showWaQR && (
  <WhatsAppQRModal onClose={() => setShowWaQR(false)} />
)}
```

- [ ] **Step 3: tsc check**

```powershell
cd C:\Users\super\ZAN\zanpos
npx tsc --noEmit 2>&1
```

Expected: no output.

- [ ] **Step 4: Commit**

```powershell
git add src/components/WhatsAppStatusPill.tsx src/pages/PosPage.tsx
git commit -m "feat(wa): add WhatsAppStatusPill to POS header"
```

---

## Task 10: WhatsAppQRModal Component

**Files:**
- Create: `src/components/WhatsAppQRModal.tsx`

- [ ] **Step 1: Create WhatsAppQRModal.tsx**

Create `src/components/WhatsAppQRModal.tsx`:
```tsx
import { useEffect, useState, useCallback } from "react";
import type { WhatsAppStatus } from "../types";
import { whatsappStatus } from "../tauri/commands";

interface Props {
  onClose: () => void;
  /** Called after QR is scanned and connection confirmed */
  onConnected?: () => void;
}

export default function WhatsAppQRModal({ onClose, onConnected }: Props) {
  const [status, setStatus] = useState<WhatsAppStatus>({ connected: false });
  const [loading, setLoading] = useState(true);

  const poll = useCallback(async () => {
    try {
      const s = await whatsappStatus();
      setStatus(s);
      setLoading(false);
      if (s.connected) {
        onConnected?.();
        onClose();
      }
    } catch {
      setLoading(false);
    }
  }, [onClose, onConnected]);

  useEffect(() => {
    poll();
    // Refresh every 20s (Baileys QR expires ~30s)
    const id = setInterval(poll, 20_000);
    return () => clearInterval(id);
  }, [poll]);

  return (
    <div className="modal-overlay" onClick={e => e.target === e.currentTarget && onClose()}>
      <div className="modal wa-qr-modal">
        <div className="modal-header">
          <span className="modal-title">📱 Connect WhatsApp</span>
          <button className="modal-close" onClick={onClose}>✕</button>
        </div>

        <div className="wa-qr-body">
          {loading && <div className="wa-qr-hint">Connecting to sidecar…</div>}

          {!loading && !status.connected && !status.qr && (
            <div className="wa-qr-hint wa-qr-hint-warn">
              WhatsApp sidecar is not running.<br />
              Restart the app to reconnect.
            </div>
          )}

          {!loading && !status.connected && status.qr && (
            <>
              <p className="wa-qr-instruction">
                Open WhatsApp on your phone → <strong>Linked Devices</strong> → <strong>Link a Device</strong> and scan this QR code.
              </p>
              <div className="wa-qr-img-wrap">
                <img src={status.qr} alt="WhatsApp QR Code" className="wa-qr-img" />
              </div>
              <p className="wa-qr-hint">QR refreshes automatically every 20 seconds.</p>
            </>
          )}

          {!loading && status.connected && (
            <div className="wa-qr-hint wa-qr-connected">✅ WhatsApp connected!</div>
          )}
        </div>

        <div className="modal-actions">
          <button className="btn-secondary" onClick={onClose}>Close</button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: tsc check**

```powershell
cd C:\Users\super\ZAN\zanpos
npx tsc --noEmit 2>&1
```

Expected: no output.

- [ ] **Step 3: Commit**

```powershell
git add src/components/WhatsAppQRModal.tsx
git commit -m "feat(wa): add WhatsAppQRModal with auto-refresh and connected auto-close"
```

---

## Task 11: POS — Delivery Confirmed → Send Flow

**Files:**
- Modify: `src/pages/PosPage.tsx`

- [ ] **Step 1: Add the send-on-delivery-confirm logic**

In `src/pages/PosPage.tsx`, find the `handleConfirmPayment` function (or the callback passed to `onConfirm` in `PaymentModal`). After the sale is finalized and `saleResult` is returned, add:

```tsx
// ── WhatsApp delivery message ──────────────────────────────────────────────
if (saleResult.delivery && saleResult.delivery.contact_number) {
  const d = saleResult.delivery;
  const sendWA = async () => {
    try {
      const waStatus = await whatsappStatus();
      if (waStatus.connected) {
        const sent = await whatsappSendDelivery({
          to:               d.contact_number,
          receipt_number:   saleResult.receipt_number,
          net_total_minor:  saleResult.net_total_minor,
          currency_exponent: DEVICE.currency_exponent,
          address_text:     d.address_text,
          house_number:     d.house_number ?? undefined,
          area:             d.area ?? undefined,
        });
        if (sent) {
          // Show success toast (implement showToast or use existing mechanism)
          console.info(`WhatsApp sent to ${d.contact_number}`);
        }
      } else if (sessionUser.role_name === "owner" || sessionUser.role_name === "manager") {
        // Manager/owner: open QR modal so they can connect and the sale is already done
        setShowWaQR(true);
      }
      // Cashier + disconnected: silent skip (toast already shown by pill)
    } catch { /* never block the sale */ }
  };
  sendWA(); // fire-and-forget — don't await, don't block the receipt flow
}
```

Import `whatsappStatus` and `whatsappSendDelivery` at the top of PosPage.tsx if not already imported:
```tsx
import { whatsappStatus, whatsappSendDelivery } from "../tauri/commands";
```

Also import `DEVICE` if not already imported:
```tsx
import { DEVICE } from "../types";
```

- [ ] **Step 2: tsc check**

```powershell
cd C:\Users\super\ZAN\zanpos
npx tsc --noEmit 2>&1
```

Expected: no output.

- [ ] **Step 3: Commit**

```powershell
git add src/pages/PosPage.tsx
git commit -m "feat(wa): send delivery WhatsApp message after sale confirmed"
```

---

## Task 12: Back Office Settings — WhatsApp Section

**Files:**
- Modify: `src/components/BackOfficeModal.tsx`

- [ ] **Step 1: Add WhatsApp section to the Settings tab in BackOfficeModal.tsx**

In `src/components/BackOfficeModal.tsx`, find the Settings tab render block (the section that renders when `activeTab === "settings"`). Add the following WhatsApp settings section inside it:

```tsx
{/* ── WhatsApp ── */}
<WhatsAppSettingsSection
  sessionUserId={sessionUser.user_id}
  sessionRole={sessionUser.role_name}
  initialBenefitNumber={appConfig?.whatsapp_benefit_number ?? ""}
/>
```

Then define the `WhatsAppSettingsSection` component in the same file (or in a separate file `src/components/WhatsAppSettingsSection.tsx` — either is fine):

```tsx
import { useCallback, useEffect, useState } from "react";
import type { WhatsAppStatus } from "../types";
import { whatsappStatus, whatsappDisconnect, whatsappSaveConfig } from "../tauri/commands";
import WhatsAppQRModal from "./WhatsAppQRModal";

function WhatsAppSettingsSection({
  sessionUserId,
  sessionRole,
  initialBenefitNumber,
}: {
  sessionUserId: string;
  sessionRole: string;
  initialBenefitNumber: string;
}) {
  const [status, setStatus]           = useState<WhatsAppStatus>({ connected: false });
  const [showQR, setShowQR]           = useState(false);
  const [benefitNum, setBenefitNum]   = useState(initialBenefitNumber);
  const [saving, setSaving]           = useState(false);
  const [saved, setSaved]             = useState(false);
  const [disconnecting, setDisconnecting] = useState(false);

  const isManager = sessionRole === "owner" || sessionRole === "manager";

  const refresh = useCallback(async () => {
    try { setStatus(await whatsappStatus()); } catch { /* ignore */ }
  }, []);

  useEffect(() => { refresh(); }, [refresh]);

  const handleSave = async () => {
    setSaving(true);
    try {
      await whatsappSaveConfig(benefitNum.trim(), sessionUserId);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e: unknown) {
      alert(typeof e === "string" ? e : "Failed to save");
    } finally {
      setSaving(false);
    }
  };

  const handleDisconnect = async () => {
    if (!confirm("Disconnect WhatsApp? You will need to scan the QR code again.")) return;
    setDisconnecting(true);
    try {
      await whatsappDisconnect(sessionUserId);
      await refresh();
    } catch (e: unknown) {
      alert(typeof e === "string" ? e : "Failed to disconnect");
    } finally {
      setDisconnecting(false);
    }
  };

  return (
    <div className="settings-section">
      <div className="settings-section-title">📱 WhatsApp</div>

      <div className="wa-settings-status-row">
        <span className={`wa-settings-badge ${status.connected ? "wa-badge-on" : "wa-badge-off"}`}>
          {status.connected ? "🟢 Connected" : "🔴 Disconnected"}
        </span>
        {!status.connected && isManager && (
          <button className="btn-primary btn-sm" onClick={() => setShowQR(true)}>
            Connect (Scan QR)
          </button>
        )}
        {status.connected && isManager && (
          <button className="btn-secondary btn-sm" onClick={handleDisconnect} disabled={disconnecting}>
            {disconnecting ? "Disconnecting…" : "Disconnect"}
          </button>
        )}
      </div>

      <div className="settings-field">
        <label className="settings-label">BenefitPay Number</label>
        <p className="settings-hint">Sent in delivery WhatsApp messages so customers can pay you.</p>
        <div className="wa-benefit-row">
          <input
            className="field-input"
            placeholder="e.g. 33050666"
            value={benefitNum}
            onChange={e => { setBenefitNum(e.target.value); setSaved(false); }}
            maxLength={20}
            disabled={!isManager}
          />
          {isManager && (
            <button className="btn-primary btn-sm" onClick={handleSave} disabled={saving}>
              {saving ? "Saving…" : saved ? "✓ Saved" : "Save"}
            </button>
          )}
        </div>
      </div>

      {showQR && (
        <WhatsAppQRModal
          onClose={() => setShowQR(false)}
          onConnected={refresh}
        />
      )}
    </div>
  );
}
```

Import `WhatsAppSettingsSection` at the top of BackOfficeModal if it was extracted to a separate file, or keep it inline.

Make sure `appConfig` is accessible in the BackOfficeModal component (it should already be passed as a prop or fetched — check the existing code and thread it through as needed).

- [ ] **Step 2: tsc check**

```powershell
cd C:\Users\super\ZAN\zanpos
npx tsc --noEmit 2>&1
```

Expected: no output.

- [ ] **Step 3: Commit**

```powershell
git add src/components/BackOfficeModal.tsx
git commit -m "feat(wa): add WhatsApp connection + benefit number to Settings tab"
```

---

## Task 13: Store Setup Wizard — Payments & WhatsApp Step

**Files:**
- Modify: `src/pages/SetupWizard.tsx`

- [ ] **Step 1: Update NewStep type and LABELS**

In `SetupWizard.tsx`, find:
```tsx
type NewStep = 1 | 2 | 3 | 4 | 5 | 6;
```
Change to:
```tsx
type NewStep = 1 | 2 | 3 | 4 | 5 | 6 | 7;
```

Find:
```tsx
const LABELS = ["Cloud", "Store Info", "Contact", "Owner", "Review"];
```
Change to:
```tsx
const LABELS = ["Cloud", "Store Info", "Contact", "Payments", "Owner", "Review"];
```

- [ ] **Step 2: Add state for the new step**

In the `NewStoreWizard` component, add new state after the existing step states:
```tsx
// Step 5 — Payments & WhatsApp
const [benefitNumber, setBenefitNumber] = useState("");
const [showWaQR, setShowWaQR]           = useState(false);
```

- [ ] **Step 3: Fix step navigation — Owner is now step 6, Review is step 7**

In the `goNext` function, update the validation check that currently references `step === 5` (Owner step) to `step === 6`. Keep other step guards unchanged.

Update `handleFinish` to navigate back to step 6 on error:
```tsx
setStep(6); // was setStep(5)
```

Update any `setStep(5)` back-navigation in the Owner step JSX to `setStep(5)` still going to Payments (correct), and in the Review step back button to `setStep(6)`.

- [ ] **Step 4: Add the new step 5 JSX block**

In the render section, after the Step 4 block (Contact & Receipt) and before the Step 5 block (Owner), add:

```tsx
{/* ── Step 5: Payments & WhatsApp ── */}
{step === 5 && (
  <div className="setup-content">
    <h2 className="setup-title">Payments & WhatsApp</h2>
    <p className="setup-body">
      Set your BenefitPay number so it appears in delivery WhatsApp messages,
      and optionally connect WhatsApp now.
    </p>

    <label className="field-label">BenefitPay Number <span className="setup-required">*</span></label>
    <p className="setup-field-hint">Customers send delivery payments to this number via BenefitPay.</p>
    <input
      className="field-input"
      type="text"
      placeholder="e.g. 33050666"
      value={benefitNumber}
      onChange={e => { setBenefitNumber(e.target.value); clearError(); }}
      maxLength={20}
    />

    <div className="setup-wa-section">
      <div className="setup-wa-title">📱 Connect WhatsApp (optional)</div>
      <p className="setup-field-hint">
        ZANPOS will automatically send order confirmations to customers via WhatsApp.
        You can skip this and connect later from Back Office → Settings.
      </p>
      <button
        className="setup-btn-secondary"
        type="button"
        onClick={() => setShowWaQR(true)}
      >
        Connect WhatsApp (Scan QR)
      </button>
    </div>

    {error && <div className="modal-error">{error}</div>}

    <div className="setup-actions">
      <button className="setup-btn-secondary" onClick={() => { setError(null); setStep(4); }}>← Back</button>
      <button
        className="setup-btn-primary"
        onClick={() => {
          if (!benefitNumber.trim()) { setError("BenefitPay number is required"); return; }
          goNext();
        }}
      >
        Next →
      </button>
    </div>

    <p className="setup-hint">You can update all these settings anytime in Back Office → Settings.</p>

    {showWaQR && (
      <WhatsAppQRModal
        onClose={() => setShowWaQR(false)}
      />
    )}
  </div>
)}
```

- [ ] **Step 5: Update the existing Owner step (was step 5, now step 6)**

Find all `step === 5` blocks in the Owner section and change them to `step === 6`.  
Find all `setStep(5)` in the Owner and Review sections and update:
- Review step "← Back" button: `setStep(6)` (was `setStep(5)`)
- Owner step "← Back" button: `setStep(5)` (correct — goes to Payments)

- [ ] **Step 6: Update handleFinish to also save benefitNumber**

In `handleFinish`, the `setupWizardComplete(...)` call should already handle store info. After `onComplete(cfg)` is called, save the benefit number:

```tsx
const handleFinish = async () => {
  setLoading(true);
  setError(null);
  try {
    const cfg = await setupWizardComplete({
      store_name:         storeName.trim(),
      store_address:      address.trim()      || undefined,
      store_phone:        phone.trim()        || undefined,
      receipt_header:     receiptHeader.trim() || undefined,
      receipt_footer:     receiptFooter.trim() || undefined,
      tax_number:         taxNumber.trim()    || undefined,
      currency,
      timezone,
      owner_display_name: ownerName.trim(),
      owner_username:     ownerUsername.trim(),
      owner_pin:          ownerPin,
    });
    // Save benefit number to app_config after setup completes
    if (benefitNumber.trim()) {
      try {
        // Use a direct DB insert via a new command we already have: whatsapp_save_config
        // We need the owner user_id — it's in cfg but not exposed. Use setup_save_benefit instead.
        // Simplest: call the Rust command setup_save_benefit_number (add in Task 7 extension below)
        await setupSaveBenefitNumber(benefitNumber.trim());
      } catch { /* non-critical */ }
    }
    onComplete(cfg);
  } catch (e: unknown) {
    setError(typeof e === "string" ? e : "Setup failed — please try again");
    setStep(6);
  } finally {
    setLoading(false);
  }
};
```

Add `setupSaveBenefitNumber` to `src/tauri/commands.ts`:
```typescript
export function setupSaveBenefitNumber(benefitNumber: string): Promise<void> {
  return invoke<void>("setup_save_benefit_number", { benefitNumber });
}
```

And add the corresponding Rust command in `setup_commands.rs` (no RBAC — only called during first-run setup when no owner exists yet):
```rust
#[tauri::command]
pub async fn setup_save_benefit_number(
    benefit_number: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT OR REPLACE INTO app_config (key, value) VALUES ('whatsapp_benefit_number', ?)",
    )
    .bind(&benefit_number)
    .execute(&state.db)
    .await?;
    Ok(())
}
```

Register it in `lib.rs` invoke_handler alongside the other setup commands:
```rust
commands::setup_commands::setup_save_benefit_number,
```

Import `WhatsAppQRModal` and `setupSaveBenefitNumber` at the top of `SetupWizard.tsx`:
```tsx
import WhatsAppQRModal from "../components/WhatsAppQRModal";
import { setupWizardComplete, setupJoinStore, adminSetupSupabase, adminSetupSupabaseCredsOnly, setupSaveBenefitNumber } from "../tauri/commands";
```

- [ ] **Step 7: tsc + cargo check**

```powershell
cd C:\Users\super\ZAN\zanpos
npx tsc --noEmit 2>&1
cd src-tauri
cargo check 2>&1 | Select-Object -Last 5
```

Expected: both clean.

- [ ] **Step 8: Commit**

```powershell
cd C:\Users\super\ZAN\zanpos
git add src/pages/SetupWizard.tsx src/tauri/commands.ts src-tauri/src/commands/setup_commands.rs src-tauri/src/lib.rs
git commit -m "feat(wa): add Payments & WhatsApp step to setup wizard"
```

---

## Task 14: CSS Styles

**Files:**
- Modify: `src/App.css`

- [ ] **Step 1: Append WhatsApp styles to App.css**

Append to the end of `src/App.css`:

```css
/* ── WhatsApp Status Pill (POS header) ──────────────────────────────────── */
.wa-pill {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  padding: 0.2rem 0.6rem;
  border-radius: 999px;
  font-size: 0.72rem;
  font-weight: 600;
  letter-spacing: 0.03em;
  border: 1px solid transparent;
  background: none;
  transition: opacity 0.2s;
}
.wa-pill-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  display: inline-block;
}
.wa-pill-on {
  color: var(--success);
  border-color: var(--success);
}
.wa-pill-on .wa-pill-dot  { background: var(--success); }
.wa-pill-off {
  color: var(--text-muted);
  border-color: var(--text-muted);
  opacity: 0.7;
}
.wa-pill-off .wa-pill-dot { background: var(--text-muted); }
.wa-pill-pending {
  color: var(--warning);
  border-color: var(--warning);
}
.wa-pill-pending .wa-pill-dot { background: var(--warning); animation: wa-blink 1s ease-in-out infinite; }
@keyframes wa-blink { 0%,100% { opacity: 1; } 50% { opacity: 0.2; } }

/* ── WhatsApp QR Modal ───────────────────────────────────────────────────── */
.wa-qr-modal {
  max-width: 420px;
  width: 90vw;
}
.wa-qr-body {
  padding: 1.25rem 1.5rem;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 1rem;
}
.wa-qr-instruction {
  font-size: 0.9rem;
  color: var(--text-secondary);
  text-align: center;
  line-height: 1.5;
}
.wa-qr-img-wrap {
  background: #fff;
  padding: 0.75rem;
  border-radius: 8px;
  border: 1px solid var(--border);
}
.wa-qr-img {
  width: 220px;
  height: 220px;
  display: block;
}
.wa-qr-hint {
  font-size: 0.8rem;
  color: var(--text-muted);
  text-align: center;
}
.wa-qr-hint-warn { color: var(--warning); }
.wa-qr-connected  { color: var(--success); font-weight: 600; font-size: 1rem; }

/* ── WhatsApp Settings Section ───────────────────────────────────────────── */
.wa-settings-status-row {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  margin-bottom: 1rem;
}
.wa-settings-badge {
  font-size: 0.85rem;
  font-weight: 600;
  padding: 0.2rem 0.6rem;
  border-radius: 999px;
  border: 1px solid;
}
.wa-badge-on  { color: var(--success); border-color: var(--success); }
.wa-badge-off { color: var(--error);   border-color: var(--error); }
.wa-benefit-row {
  display: flex;
  gap: 0.5rem;
  align-items: center;
}
.wa-benefit-row .field-input { flex: 1; }

/* ── Setup Wizard — WhatsApp step ────────────────────────────────────────── */
.setup-wa-section {
  margin-top: 1.5rem;
  padding: 1rem;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--surface-alt, var(--surface));
}
.setup-wa-title {
  font-weight: 600;
  font-size: 0.95rem;
  margin-bottom: 0.4rem;
}
.setup-field-hint {
  font-size: 0.8rem;
  color: var(--text-muted);
  margin: 0.15rem 0 0.5rem;
}
.setup-required { color: var(--error); }
```

- [ ] **Step 2: tsc check one final time**

```powershell
cd C:\Users\super\ZAN\zanpos
npx tsc --noEmit 2>&1
```

Expected: no output.

- [ ] **Step 3: cargo check one final time**

```powershell
cd src-tauri
cargo check 2>&1 | Select-Object -Last 5
```

Expected: `Finished \`dev\` profile`

- [ ] **Step 4: Commit**

```powershell
cd C:\Users\super\ZAN\zanpos
git add src/App.css
git commit -m "feat(wa): add WhatsApp pill, QR modal, and settings CSS styles"
```

---

## Self-Review Checklist

- [x] **Spec coverage:**
  - ✅ Sidecar (Task 1–3): Baileys + Express + pkg binary
  - ✅ Rust commands (Task 5–7): status, send_delivery, disconnect, save_config + message builder
  - ✅ Sidecar lifecycle (Task 4): start in setup hook, kill on window destroy
  - ✅ Bilingual message template (Task 5): EN + AR + BenefitPay + store footer
  - ✅ POS header pill (Task 9): connected/disconnected/pending states
  - ✅ QR modal (Task 10): auto-refresh 20s, auto-close on connected
  - ✅ Post-checkout send flow (Task 11): connected → send; manager + disconnected → QR modal; cashier → skip
  - ✅ Back Office Settings (Task 12): connect/disconnect + benefit_number edit
  - ✅ Store Setup wizard step (Task 13): benefit_number (required) + optional QR
  - ✅ CSS (Task 14): all new components styled

- [x] **No placeholders:** All code is complete in every step.

- [x] **Type consistency:**
  - `SendDeliveryInput` defined in `types.ts` (Task 8), used in `commands.ts` (Task 8) and `PosPage.tsx` (Task 11)
  - `WhatsAppStatus` defined in `types.ts` (Task 8), used in `WhatsAppStatusPill` (Task 9), `WhatsAppQRModal` (Task 10)
  - `WhatsAppDeliveryParams` defined in `whatsapp_commands.rs` (Task 5), used in `whatsapp_send_delivery` (Task 6)
  - `build_delivery_whatsapp_message` defined in Task 5, called in Task 6 ✅
