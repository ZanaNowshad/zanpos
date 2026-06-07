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

const fs      = require("fs");
const path    = require("path");
const express = require("express");
const QRCode  = require("qrcode");
const {
  default: makeWASocket,
  useMultiFileAuthState,
  DisconnectReason,
  fetchLatestBaileysVersion,
  Browsers,
} = require("@whiskeysockets/baileys");
const pino = require("pino");

const crypto = require("node:crypto");

// ── Parse CLI args ─────────────────────────────────────────────────────────────
const args = process.argv.slice(2);
const sessionDir = (() => {
  const flag = args.find(a => a.startsWith("--session-dir="));
  return flag ? flag.split("=").slice(1).join("=") : "./wa-session";
})();
const PORT = 3131;

// ── Shared-secret authentication ───────────────────────────────────────────────
const SIDECAR_TOKEN = crypto.randomBytes(32).toString("hex");
const TOKEN_FILE = path.join(sessionDir, ".sidecar_token");
try {
  fs.mkdirSync(sessionDir, { recursive: true });
  fs.writeFileSync(TOKEN_FILE, SIDECAR_TOKEN);
} catch (e) {
  console.error("[wa-sidecar] Failed to write sidecar token:", e.message);
  process.exit(1);
}
console.log(`[wa-sidecar] Auth token written to ${TOKEN_FILE}`);

// ── State ─────────────────────────────────────────────────────────────────────
let sock           = null;
let qrDataUrl      = null;   // base64 PNG data URL, null when connected or idle
let isConnected    = false;
let isStarting     = false;
/** Contacts accumulated from Baileys contacts.upsert events.
 *  Keyed by JID (e.g. "97333050666@s.whatsapp.net"), value { id, name }.
 *  Persisted to sessionDir/contacts.json so contacts survive sidecar restarts.
 *  Without persistence, re-connects with saved auth skip contacts.upsert and
 *  the import endpoint would return 0 contacts. */
let contactsMap    = {};

const contactsFile = path.join(sessionDir, "contacts.json");

// ── Load persisted contacts on startup ───────────────────────────────────────
try {
  if (fs.existsSync(contactsFile)) {
    contactsMap = JSON.parse(fs.readFileSync(contactsFile, "utf8"));
    console.log(`[wa-sidecar] Loaded ${Object.keys(contactsMap).length} contacts from cache`);
  }
} catch (e) {
  console.warn("[wa-sidecar] Could not load contacts cache:", e.message);
  contactsMap = {};
}

function saveContacts() {
  try {
    fs.writeFileSync(contactsFile, JSON.stringify(contactsMap));
  } catch (e) {
    console.warn("[wa-sidecar] Could not save contacts cache:", e.message);
  }
}

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
      // Per Baileys docs: contacts arrive inside the history sync, which WhatsApp
      // only sends to a *desktop* client with syncFullHistory enabled. The previous
      // ["ZANPOS","Chrome",...] (web) browser + no syncFullHistory meant
      // messaging-history.set carried no contacts → Import Contacts returned empty.
      browser:            Browsers.macOS("Desktop"),
      syncFullHistory:    true,
    });

    sock.ev.on("creds.update", saveCreds);

    // messaging-history.set — the correct event per the Baileys docs.
    // Fires after every successful connect carrying the initial bulk sync:
    // { chats, contacts, messages, syncType }.  This is the ONLY reliable
    // way to receive the full contact list; contacts.set is an internal
    // store event and is not part of the public Baileys API.
    sock.ev.on("messaging-history.set", ({ contacts: histContacts }) => {
      if (!histContacts || histContacts.length === 0) return;
      for (const c of histContacts) {
        if (!c.id) continue;
        const name = c.notify || c.name || c.id.split("@")[0] || c.id;
        contactsMap[c.id] = { id: c.id, name };
      }
      saveContacts();
      console.log(`[wa-sidecar] messaging-history.set: ${histContacts.length} contacts received, total=${Object.keys(contactsMap).length}`);
    });

    // contacts.upsert — fires when a new contact is added to the address book in real time.
    sock.ev.on("contacts.upsert", (contacts) => {
      for (const c of contacts) {
        if (!c.id) continue;
        const name = c.notify || c.name || c.id.split("@")[0] || c.id;
        contactsMap[c.id] = { id: c.id, name };
      }
      saveContacts();
      console.log(`[wa-sidecar] contacts.upsert: ${contacts.length} added, total=${Object.keys(contactsMap).length}`);
    });

    // contacts.update — fires when an existing contact's name/details change.
    sock.ev.on("contacts.update", (updates) => {
      for (const u of updates) {
        if (!u.id) continue;
        const existing = contactsMap[u.id];
        const name = u.notify || u.name || (existing && existing.name) || u.id.split("@")[0] || u.id;
        contactsMap[u.id] = { id: u.id, name };
      }
      saveContacts();
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

// Auth middleware: require X-Sidecar-Token for all endpoints
app.use((req, res, next) => {
  const token = req.headers["x-sidecar-token"];
  if (!token || token !== SIDECAR_TOKEN) {
    return res.status(401).json({ ok: false, error: "unauthorized" });
  }
  next();
});

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

/** POST /send-document
 *  body: { to: "+97333050666", caption: "...", document_base64: "...", mimetype: "application/pdf", filename: "receipt.pdf" }
 *  → { ok: bool }
 *  Sends a document (e.g. PDF) with an optional caption as a single WhatsApp message. */
app.post("/send-document", async (req, res) => {
  const { to, caption, document_base64, mimetype, filename } = req.body ?? {};
  if (!to || !document_base64) {
    return res.status(400).json({ ok: false, error: "to and document_base64 required" });
  }
  if (!isConnected || !sock) {
    return res.json({ ok: false, error: "not connected" });
  }
  try {
    const jid = to.replace(/^\+/, "") + "@s.whatsapp.net";
    const docBuffer = Buffer.from(document_base64, "base64");
    await sock.sendMessage(jid, {
      document: docBuffer,
      mimetype: mimetype || "application/pdf",
      fileName: filename || "receipt.pdf",
      caption: caption || undefined,
    });
    res.json({ ok: true });
  } catch (err) {
    console.error("[wa-sidecar] send-document failed:", err.message);
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
  // Clear in-memory and persisted contacts — user is logging out
  contactsMap = {};
  try { fs.unlinkSync(contactsFile); } catch (_) { /* may not exist */ }
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
