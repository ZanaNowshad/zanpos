"use strict";

// ── Polyfill globalThis.crypto for pkg's bundled Node 18.5.0 ──────────────────
// pkg bundles an old Node 18 runtime that doesn't expose WebCrypto on globalThis.
// Baileys v6 uses `globalThis.crypto.subtle` directly, so we patch it first.
if (!globalThis.crypto) {
  const nodeCrypto = require("node:crypto");
  if (nodeCrypto.webcrypto) {
    globalThis.crypto = nodeCrypto.webcrypto;
  }
}

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
let sock           = null;
let qrDataUrl      = null;   // base64 PNG data URL, null when connected or idle
let isConnected    = false;
let isStarting     = false;
/** Contacts accumulated from Baileys contacts.upsert events.
 *  Keyed by JID (e.g. "97333050666@s.whatsapp.net"), value { id, name }.
 *  Persists across reconnects so contacts accumulate over the sidecar's lifetime. */
let contactsMap    = {};

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

    // Accumulate contacts from Baileys — fires with all contacts shortly after connect
    sock.ev.on("contacts.upsert", (contacts) => {
      for (const c of contacts) {
        // Prefer the push name ("notify"), fall back to address-book name, then bare phone
        const name = c.notify || c.name || c.id.split("@")[0] || c.id;
        contactsMap[c.id] = { id: c.id, name };
      }
    });

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

/** GET /contacts → [{ id: string, name: string }]
 *  Returns all contacts accumulated via contacts.upsert events.
 *  Only individual-user JIDs (@s.whatsapp.net) are included — groups are excluded. */
app.get("/contacts", (_req, res) => {
  const individual = Object.values(contactsMap).filter(c =>
    typeof c.id === "string" && c.id.endsWith("@s.whatsapp.net"),
  );
  res.json(individual);
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
