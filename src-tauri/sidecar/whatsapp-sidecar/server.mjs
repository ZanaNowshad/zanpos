// ZANPOS WhatsApp sidecar — Baileys v7 HTTP bridge (ESM).
//
// Spike port of the v6 server.js to Baileys v7-rc13. Same HTTP contract
// (9 endpoints, X-Sidecar-Token, port 3131, --session-dir) so the Rust side
// is unchanged. Key v7 differences handled here:
//   • ESM + makeWASocket default import (v7 dropped CommonJS).
//   • No globalThis.crypto shim — Node >=20 exposes WebCrypto natively.
//   • LID addressing: incoming keys may be @lid; resolve to the phone-number
//     JID via the key's *Alt field (primary), getPNForLID (fallback), then
//     pushName (last resort) so /messages keeps emitting senderName + a PN.

import fs from "fs";
import path from "path";
import express from "express";
import QRCode from "qrcode";
import pino from "pino";
import { createWorker } from "tesseract.js";
import makeWASocket, {
  useMultiFileAuthState,
  DisconnectReason,
  fetchLatestBaileysVersion,
  Browsers,
  downloadMediaMessage,
  normalizeMessageContent,
  getContentType,
  isLidUser,
  isPnUser,
  jidNormalizedUser,
  jidDecode,
} from "@whiskeysockets/baileys";
import crypto from "node:crypto";
import http from "node:http";

// Survive transient Baileys/WebSocket errors; the connection.update handler
// does the orderly reconnect. Without this an unhandled rejection kills the
// process and the Rust watchdog respawns it in a loop.
process.on("uncaughtException", (e) => console.error("[wa-sidecar] uncaughtException:", (e && e.message) || e));
process.on("unhandledRejection", (e) => console.error("[wa-sidecar] unhandledRejection:", (e && e.message) || e));

// ── CLI args ─────────────────────────────────────────────────────────────────
const args = process.argv.slice(2);
const sessionDir = (() => {
  const flag = args.find((a) => a.startsWith("--session-dir="));
  return flag ? flag.split("=").slice(1).join("=") : "./wa-session";
})();
const PORT = Number(process.env.WA_SIDECAR_PORT) || 3131;

// ── Shared-secret auth ───────────────────────────────────────────────────────
const SIDECAR_TOKEN = crypto.randomBytes(32).toString("hex");
const TOKEN_FILE = path.join(sessionDir, ".sidecar_token");
const PID_FILE = path.join(sessionDir, ".sidecar.pid");
try {
  fs.mkdirSync(sessionDir, { recursive: true });
  fs.writeFileSync(TOKEN_FILE, SIDECAR_TOKEN);
  fs.writeFileSync(PID_FILE, String(process.pid));
} catch (e) {
  console.error("[wa-sidecar] Failed to write sidecar token:", e.message);
  process.exit(1);
}
// Remove PID file on clean exit so the launcher knows no cleanup is needed.
process.on("exit", () => { try { fs.unlinkSync(PID_FILE); } catch (_) {} });
console.log(`[wa-sidecar] PID ${process.pid} — auth token written to ${TOKEN_FILE}`);

// ── State ────────────────────────────────────────────────────────────────────
let sock = null;
let qrDataUrl = null;
let isConnected = false;
let isStarting = false;
let reconnectAttempts = 0;
let reconnectTimer = null;

function scheduleReconnect(reason) {
  if (reconnectTimer) return;
  reconnectAttempts += 1;
  const delay = Math.min(30_000, 2_000 * 2 ** Math.min(reconnectAttempts - 1, 4));
  console.log(`[wa-sidecar] Reconnecting in ${Math.round(delay / 1000)}s (attempt ${reconnectAttempts}, ${reason})`);
  reconnectTimer = setTimeout(() => { reconnectTimer = null; startBaileys(); }, delay);
}

// Contacts keyed by normalized PN JID, persisted so they survive restarts.
let contactsMap = {};
const contactsFile = path.join(sessionDir, "contacts.json");

// Groups keyed by @g.us JID. Kept separately from contacts because Baileys does
// not always include group subjects in contact sync, and groupFetch can briefly
// return nothing while the session is still settling.
let groupsMap = {};
const groupsFile = path.join(sessionDir, "groups.json");

// Incoming messages ring buffer, drained by the POS via GET /messages?after=<seq>.
let inbox = [];
let msgSeq = 0;
const INBOX_CAP = 300;

// Images downloaded + decrypted immediately on receipt (WhatsApp media URLs
// expire and the proto is lost on restart). GET /media just reads the file.
const mediaDir = path.join(sessionDir, "media");
try { fs.mkdirSync(mediaDir, { recursive: true }); } catch (_) { /* ignore */ }
const mediaMimeFile = path.join(sessionDir, "media-mime.json");
let mediaMime = {};
let mediaOrder = [];
const MEDIA_CAP = 40;
try {
  if (fs.existsSync(mediaMimeFile)) {
    const saved = JSON.parse(fs.readFileSync(mediaMimeFile, "utf8"));
    mediaMime = saved.mime || {};
    mediaOrder = (saved.order || []).filter(id => fs.existsSync(path.join(mediaDir, `${id}.bin`)));
  }
} catch (_) { /* start fresh */ }
function saveMediaMime() {
  try { fs.writeFileSync(mediaMimeFile, JSON.stringify({ mime: mediaMime, order: mediaOrder })); }
  catch (_) { /* non-fatal */ }
}

// ── Local OCR (Tesseract) for payment-screenshot verification ─────────────────
// Deterministic text extraction — NOT an LLM. The Rust side feeds the extracted
// text to the AI for amount/business-name matching. Lazily initialise one worker
// on first use (worker spin-up is expensive); traineddata is cached under the
// session dir so subsequent runs are fully offline.
const tessDir = path.join(sessionDir, "tessdata");
try { fs.mkdirSync(tessDir, { recursive: true }); } catch (_) { /* ignore */ }
let ocrWorker = null;
let ocrInitPromise = null;
function getOcrWorker() {
  if (ocrWorker) return Promise.resolve(ocrWorker);
  if (!ocrInitPromise) {
    ocrInitPromise = createWorker("eng", 1, { langPath: tessDir, cachePath: tessDir, gzip: true })
      .then((w) => { ocrWorker = w; return w; })
      .catch((e) => { ocrInitPromise = null; throw e; });
  }
  return ocrInitPromise;
}

const logger = pino({ level: "silent" });

function persistImage(id, msg, mimetype) {
  const file = path.join(mediaDir, `${id}.bin`);
  if (fs.existsSync(file)) { mediaMime[id] = mimetype || mediaMime[id] || "image/jpeg"; saveMediaMime(); return; }
  (async () => {
    try {
      const buf = await downloadMediaMessage(msg, "buffer", {}, { logger, reuploadRequest: sock && sock.updateMediaMessage });
      fs.writeFileSync(file, buf);
      mediaMime[id] = mimetype || "image/jpeg";
      mediaOrder.push(id);
      while (mediaOrder.length > MEDIA_CAP) {
        const old = mediaOrder.shift();
        try { fs.unlinkSync(path.join(mediaDir, `${old}.bin`)); } catch (_) { /* gone */ }
        delete mediaMime[old];
      }
      saveMediaMime();
    } catch (e) {
      console.error(`[wa-sidecar] image download failed for ${id}:`, e.message);
    }
  })();
}

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
  try { fs.writeFileSync(contactsFile, JSON.stringify(contactsMap)); }
  catch (e) { console.warn("[wa-sidecar] Could not save contacts cache:", e.message); }
}

/**
 * Keep every name a contact is known by, not just one.
 *
 * Baileys gives two different names and they are not interchangeable:
 *   c.name   — the address-book name, what the shop saved this person as
 *   c.notify — the pushName, what the person set on their own profile
 *
 * This used to collapse to `c.notify || c.name`, so the shop's own name for a
 * customer was thrown away and only the customer's chosen name was searchable.
 * A cashier who saved someone as "Ali Baqala" had to remember that WhatsApp
 * knows them as "Ali ⚡" before the till would find them — which is exactly the
 * thing nobody remembers with a queue waiting.
 *
 * So both are kept. `name` is the display name and now prefers the shop's own
 * spelling, because that is what appears on the receipt and what the operator
 * will type; the other names ride along so either one finds the person.
 *
 * Existing values are never overwritten with nothing: `contacts.update` sends
 * partial records, and an update carrying only a pushName must not erase the
 * saved name.
 */
function mergeContact(id, incoming, existing) {
  const prev = existing || {};
  const savedName = incoming.name || prev.savedName || null;
  const pushName = incoming.notify || prev.pushName || null;
  const verifiedName = incoming.verifiedName || prev.verifiedName || null;
  return {
    id,
    name: savedName || pushName || verifiedName || id.split("@")[0] || id,
    savedName,
    pushName,
    verifiedName,
  };
}
try {
  if (fs.existsSync(groupsFile)) {
    groupsMap = JSON.parse(fs.readFileSync(groupsFile, "utf8"));
    console.log(`[wa-sidecar] Loaded ${Object.keys(groupsMap).length} groups from cache`);
  }
} catch (e) {
  console.warn("[wa-sidecar] Could not load groups cache:", e.message);
  groupsMap = {};
}
function saveGroups() {
  try { fs.writeFileSync(groupsFile, JSON.stringify(groupsMap)); }
  catch (e) { console.warn("[wa-sidecar] Could not save groups cache:", e.message); }
}
function rememberGroup(id, name) {
  if (!id || !String(id).endsWith("@g.us")) return;
  groupsMap[id] = { id, name: name || (groupsMap[id] && groupsMap[id].name) || id };
}

function realMessage(message) {
  return normalizeMessageContent(message) || message;
}

function extractText(content) {
  if (!content) return "";
  const type = getContentType(content);
  switch (type) {
    case "conversation":         return content.conversation || "";
    case "extendedTextMessage":  return content.extendedTextMessage?.text || "";
    case "imageMessage":         return content.imageMessage?.caption || "📷 Photo";
    case "videoMessage":         return content.videoMessage?.caption || "🎬 [video]";
    case "documentMessage":      return content.documentMessage?.caption || `📄 ${content.documentMessage?.fileName || "[document]"}`;
    case "audioMessage":         return content.audioMessage?.ptt ? "🎙️ [voice message]" : "🎵 [audio]";
    case "stickerMessage":       return "🪧 [sticker]";
    case "contactMessage":       return `👤 ${content.contactMessage?.displayName || "[contact]"}`;
    case "contactsArrayMessage": return "👥 [contacts]";
    case "locationMessage":      return "📍 [location]";
    case "liveLocationMessage":  return "📍 [live location]";
    case "pollCreationMessage":
    case "pollCreationMessageV2":
    case "pollCreationMessageV3":
      return `📊 ${content[type]?.name || "[poll]"}`;
    case "orderMessage":
      return `🛒 ${content.orderMessage?.orderTitle || "[WhatsApp order]"}`;
    case "productMessage":
      return `🏷️ ${content.productMessage?.product?.title || "[product]"}`;
    default:
      if (type) console.log(`[wa-sidecar] unhandled message type: ${type}`);
      return type ? `[${type}]` : "[message]";
  }
}

/** v7 LID resolution: pick the phone-number (@s.whatsapp.net) JID for a sender.
 *  WhatsApp now addresses some chats by @lid; v7 puts the alternate (PN) address
 *  on key.*Alt and auto-learns the LID↔PN mapping. Order: primary if it's a PN →
 *  the *Alt if it's a PN → resolve the @lid via the mapping store → "" (unknown). */
async function resolvePnJid(primary, alt) {
  if (isPnUser(primary)) return primary;
  if (isPnUser(alt)) return alt;
  const lid = isLidUser(primary) ? primary : (isLidUser(alt) ? alt : "");
  if (lid) {
    try {
      const pn = await sock?.signalRepository?.lidMapping?.getPNForLID?.(lid);
      if (pn) return pn;
    } catch (_) { /* mapping not known yet */ }
  }
  return "";
}

// ── Auth state helpers ────────────────────────────────────────────────────────
// Files we own — never delete these when wiping Baileys auth state.
const OWN_FILES = new Set([".sidecar_token", ".sidecar.pid", "contacts.json", ".v7"]);

function clearAuthState() {
  try {
    for (const f of fs.readdirSync(sessionDir)) {
      if (OWN_FILES.has(f)) continue;
      const full = path.join(sessionDir, f);
      try {
        if (fs.statSync(full).isDirectory()) continue; // skip media/ etc.
        fs.unlinkSync(full);
      } catch (_) { /* ignore */ }
    }
    console.log("[wa-sidecar] Auth state cleared — next start will show QR");
  } catch (e) {
    console.error("[wa-sidecar] clearAuthState error:", e.message);
  }
}

// ── Baileys lifecycle ──────────────────────────────────────────────────────────
async function startBaileys() {
  if (isStarting) return;
  isStarting = true;
  try {
    let version;
    try {
      ({ version } = await fetchLatestBaileysVersion());
      console.log("[wa-sidecar] WhatsApp version from server:", version?.join("."));
    } catch (e) {
      console.warn("[wa-sidecar] fetchLatestBaileysVersion failed:", e.message, "— using bundled version");
    }
    const { state, saveCreds } = await useMultiFileAuthState(sessionDir);

    sock = makeWASocket({
      version,
      auth: state,
      logger,
      browser: Browsers.macOS("Desktop"),
      // syncFullHistory forces a heavy initial download that times out (408) on
      // large accounts; live messages still arrive via messages.upsert.
      syncFullHistory: false,
      markOnlineOnConnect: false,
      connectTimeoutMs: 60_000,
      keepAliveIntervalMs: 30_000,
      defaultQueryTimeoutMs: 60_000,
      retryRequestDelayMs: 1_500,
      qrTimeout: Number(process.env.WA_QR_TIMEOUT_MS) || 60_000,
      emitOwnEvents: false,
    });

    sock.ev.on("creds.update", saveCreds);

    sock.ev.on("messaging-history.set", ({ contacts: histContacts }) => {
      if (!histContacts || histContacts.length === 0) return;
      for (const c of histContacts) {
        if (!c.id) continue;
        const id = jidNormalizedUser(c.id);
        contactsMap[id] = mergeContact(id, c, contactsMap[id]);
      }
      saveContacts();
      console.log(`[wa-sidecar] messaging-history.set: ${histContacts.length} contacts, total=${Object.keys(contactsMap).length}`);
    });

    sock.ev.on("contacts.upsert", (contacts) => {
      for (const c of contacts) {
        if (!c.id) continue;
        const id = jidNormalizedUser(c.id);
        contactsMap[id] = mergeContact(id, c, contactsMap[id]);
      }
      saveContacts();
    });

    sock.ev.on("contacts.update", (updates) => {
      for (const u of updates) {
        if (!u.id) continue;
        const id = jidNormalizedUser(u.id);
        contactsMap[id] = mergeContact(id, u, contactsMap[id]);
      }
      saveContacts();
    });

    sock.ev.on("groups.upsert", (groups) => {
      for (const g of groups || []) rememberGroup(g.id, g.subject);
      saveGroups();
    });

    sock.ev.on("groups.update", (updates) => {
      for (const g of updates || []) rememberGroup(g.id, g.subject);
      saveGroups();
    });

    sock.ev.on("messages.upsert", async ({ messages, type }) => {
      if (type !== "notify" || !Array.isArray(messages)) return;
      for (const msg of messages) {
        const key = msg?.key;
        if (!key || key.fromMe) continue;
        const chatJid = key.remoteJid || "";
        if (!chatJid || chatJid === "status@broadcast") continue;

        const isGroup = chatJid.endsWith("@g.us");
        if (isGroup) rememberGroup(chatJid, groupsMap[chatJid]?.name || chatJid);
        const primarySender = isGroup ? (key.participant || "") : chatJid;
        const altSender = isGroup ? (key.participantAlt || "") : (key.remoteJidAlt || "");

        const content = realMessage(msg.message);
        const ctype = getContentType(content);
        const SKIP = new Set([
          "senderKeyDistributionMessage", "protocolMessage", "reactionMessage",
          "pollUpdateMessage", "messageContextInfo", "keepInChatMessage",
        ]);
        if (!ctype || SKIP.has(ctype)) continue;

        // Resolve the phone-number JID (handles @lid addressing). Falls back to
        // the raw primary sender if no PN is known yet (degraded, not broken).
        const pnJid = await resolvePnJid(primarySender, altSender);
        const idForName = jidNormalizedUser(pnJid || primarySender);
        const phone = jidDecode(pnJid || primarySender)?.user || idForName.split("@")[0];
        const senderName = (contactsMap[idForName] && contactsMap[idForName].name) || msg.pushName || phone;

        const text = extractText(content);
        const ts = Number(msg.messageTimestamp) || Math.floor(Date.now() / 1000);

        let mediaType = null;
        if (ctype === "imageMessage") {
          mediaType = "image";
          persistImage(key.id, msg, content.imageMessage && content.imageMessage.mimetype);
        }

        // WhatsApp commerce: a customer checking out from a linked catalog sends
        // an orderMessage carrying the orderId + a base64 token that getOrderDetails
        // needs. Capture both so the Rust side can resolve the full order later.
        let orderId = null;
        let orderToken = null;
        if (ctype === "orderMessage") {
          orderId = content.orderMessage?.orderId || null;
          orderToken = content.orderMessage?.token || null; // base64 token
        }

        inbox.push({
          seq: ++msgSeq,
          id: key.id,
          chatJid,
          isGroup,
          senderJid: pnJid || primarySender, // emit the PN form when we have it
          senderName,
          text,
          ts,
          mediaType,
          orderId,
          orderToken,
        });
      }
      if (inbox.length > INBOX_CAP) inbox = inbox.slice(-INBOX_CAP);
    });

    sock.ev.on("connection.update", async (update) => {
      const { connection, lastDisconnect, qr } = update;
      if (qr) {
        console.log("[wa-sidecar] QR received from WhatsApp — converting to data URL");
        try { qrDataUrl = await QRCode.toDataURL(qr); console.log("[wa-sidecar] QR ready for scanning"); } catch (e) { console.error("[wa-sidecar] QRCode.toDataURL failed:", e.message); }
      }
      if (connection === "open") {
        isConnected = true;
        qrDataUrl = null;
        isStarting = false;
        reconnectAttempts = 0;
        if (reconnectTimer) { clearTimeout(reconnectTimer); reconnectTimer = null; }
        console.log("[wa-sidecar] Connected");
      }
      if (connection === "close") {
        isConnected = false;
        isStarting = false;
        qrDataUrl = null; // drop the stale QR so /status doesn't serve a dead one during reconnect
        const code = lastDisconnect?.error?.output?.statusCode;
        const errMsg = lastDisconnect?.error?.message || lastDisconnect?.error?.toString?.() || "";
        console.log("[wa-sidecar] Disconnected, code:", code, errMsg ? `| ${errMsg}` : "");
        const terminal = [
          DisconnectReason.loggedOut,
          DisconnectReason.forbidden,
          DisconnectReason.badSession,
          DisconnectReason.connectionReplaced,
        ];
        if (terminal.includes(code)) {
          console.log("[wa-sidecar] Terminal disconnect (code", code, ") — clearing auth state for fresh QR");
          sock = null;
          clearAuthState();
          reconnectAttempts = 0;
          scheduleReconnect(`terminal ${code}`);
        } else {
          // 428 = connectionTerminated (normal QR-cycle timeout, keep-alive drop, server bump).
          // Do NOT clear auth on repeated 428s — a scan may be in flight when the cycle resets,
          // and wiping creds would undo a successful pairing. Just reconnect and let Baileys resume.
          scheduleReconnect(`code ${code}`);
        }
      }
    });
  } catch (err) {
    isStarting = false;
    console.error("[wa-sidecar] Failed to start Baileys:", err.message);
    scheduleReconnect("start failed");
  }
}

// ── Express endpoints (unchanged contract) ──────────────────────────────────────
const app = express();
app.use(express.json());
// /health is unauthenticated so the EADDRINUSE standby probe can reach it
app.get("/health", (_req, res) => res.json({ ok: true }));
app.use((req, res, next) => {
  if (req.headers["x-sidecar-token"] !== SIDECAR_TOKEN) return res.status(401).json({ ok: false, error: "unauthorized" });
  next();
});

app.get("/status", (_req, res) => res.json({ connected: isConnected, qr: qrDataUrl ?? undefined }));

app.post("/send", async (req, res) => {
  const { to, message } = req.body ?? {};
  if (!to || !message) return res.status(400).json({ ok: false, error: "to and message required" });
  if (!isConnected || !sock) return res.json({ ok: false, error: "not connected" });
  try {
    const jid = to.replace(/^\+/, "") + "@s.whatsapp.net";
    await sock.sendMessage(jid, { text: message });
    res.json({ ok: true });
  } catch (err) {
    console.error("[wa-sidecar] Send failed:", err.message);
    res.json({ ok: false, error: err.message });
  }
});

app.post("/send-document", async (req, res) => {
  const { to, caption, document_base64, mimetype, filename } = req.body ?? {};
  if (!to || !document_base64) return res.status(400).json({ ok: false, error: "to and document_base64 required" });
  if (!isConnected || !sock) return res.json({ ok: false, error: "not connected" });
  try {
    const jid = to.replace(/^\+/, "") + "@s.whatsapp.net";
    await sock.sendMessage(jid, {
      document: Buffer.from(document_base64, "base64"),
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

app.get("/contacts", (_req, res) => {
  const individual = Object.values(contactsMap).filter((c) => typeof c.id === "string" && c.id.endsWith("@s.whatsapp.net"));
  res.json(individual);
});

app.get("/messages", (req, res) => {
  const after = Number.parseInt(req.query.after, 10) || 0;
  res.json({ messages: inbox.filter((m) => m.seq > after), cursor: msgSeq });
});

app.get("/media", (req, res) => {
  const id = req.query.id && path.basename(String(req.query.id));
  const file = id && path.join(mediaDir, `${id}.bin`);
  if (!file || !fs.existsSync(file)) return res.json({ ok: false, error: "not available" });
  try {
    const buffer = fs.readFileSync(file);
    res.json({ ok: true, base64: buffer.toString("base64"), mimetype: mediaMime[id] || "image/jpeg" });
  } catch (err) {
    console.error("[wa-sidecar] /media failed:", err.message);
    res.json({ ok: false, error: err.message });
  }
});

// Run local OCR over a stored image and return the raw extracted text.
app.get("/ocr", async (req, res) => {
  const id = req.query.id && path.basename(String(req.query.id));
  const file = id && path.join(mediaDir, `${id}.bin`);
  if (!file || !fs.existsSync(file)) return res.json({ ok: false, error: "not available" });
  try {
    const worker = await getOcrWorker();
    const result = await worker.recognize(file);
    res.json({ ok: true, text: (result && result.data && result.data.text) || "" });
  } catch (err) {
    console.error("[wa-sidecar] /ocr failed:", err.message);
    res.json({ ok: false, error: err.message });
  }
});

// Generate a QR PNG data-URL for arbitrary text (e.g. the public storefront
// URL, printed for the shop counter). Reuses the same `qrcode` package that
// renders the WhatsApp pairing QR. Local-only like every other route.
app.get("/qr", async (req, res) => {
  const text = String(req.query.text || "");
  if (!text || text.length > 2048) return res.status(400).json({ ok: false, error: "text required (max 2048 chars)" });
  try {
    const dataUrl = await QRCode.toDataURL(text, { margin: 2, width: 480 });
    res.json({ ok: true, data_url: dataUrl });
  } catch (err) {
    console.error("[wa-sidecar] /qr failed:", err.message);
    res.json({ ok: false, error: err.message });
  }
});

app.get("/groups", async (_req, res) => {
  const cached = { ...groupsMap };
  if (!isConnected || !sock) return res.json(Object.values(cached));
  try {
    const all = await sock.groupFetchAllParticipating();
    for (const g of Object.values(all || {})) rememberGroup(g.id, g.subject || g.id);
    saveGroups();
    res.json(Object.values({ ...cached, ...groupsMap }));
  } catch (err) {
    console.error("[wa-sidecar] /groups failed:", err.message);
    res.json(Object.values(cached));
  }
});

// ── WhatsApp Business Catalog ─────────────────────────────────────────────────
// NOTE: these require the linked number to be a WhatsApp Business account with a
// catalog. Read routes are low-risk; write routes (product create/update/delete)
// are rate-limited by the Rust layer and carry ban risk on the WhatsApp side.

function ownJid() {
  try { return sock?.user?.id ? jidNormalizedUser(sock.user.id) : null; }
  catch (_) { return null; }
}

// Baileys catalog/order queries HANG indefinitely when the linked number is a
// personal WhatsApp account (or a Business account with no catalog) — WhatsApp
// simply never replies. Without this the Rust client eventually times out and
// the UI shows a cryptic "error sending request". Racing against a timer lets us
// return a clear, actionable message instead.
function withTimeout(promise, ms, label) {
  let timer;
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(
      () => reject(new Error(`${label} timed out after ${ms / 1000}s — is this a WhatsApp Business account with a catalog?`)),
      ms
    );
  });
  return Promise.race([promise, timeout]).finally(() => clearTimeout(timer));
}

// Robust productCreate. WhatsApp's product_catalog_add response is SLOW and
// sometimes dropped even though the write commits server-side. If productCreate
// times out, we DON'T blindly retry (that risks duplicates) — instead we read
// the catalog back and look for the product we just tried to create (matched by
// retailerId, else by name). If it's there, the write actually succeeded.
async function createProductRobust(spec, timeoutMs = 90_000) {
  try {
    return await withTimeout(sock.productCreate(spec), timeoutMs, "Creating product");
  } catch (err) {
    const slow = /timed out/i.test(err && err.message);
    if (!slow) throw err;
    // The response was slow — check whether it landed anyway.
    try {
      const jid = ownJid();
      const { products } = await withTimeout(
        sock.getCatalog({ jid, limit: 500 }), 60_000, "Verifying catalog"
      );
      const found = (products || []).find(p =>
        (spec.retailerId && p.retailerId === spec.retailerId) || p.name === spec.name
      );
      if (found) return found; // write actually succeeded
    } catch (_) { /* verification itself failed — fall through */ }
    throw new Error(
      `${err.message} (verification could not confirm the product landed — WhatsApp's catalog service is responding very slowly right now)`
    );
  }
}

// Normalize a Baileys Product into the flat shape the Rust layer expects.
function flattenProduct(p) {
  const imageUrls = p?.imageUrls && typeof p.imageUrls === "object"
    ? Object.values(p.imageUrls).filter(Boolean)
    : [];
  return {
    id: p?.id || "",
    name: p?.name || "",
    description: p?.description || null,
    price: typeof p?.price === "number" ? p.price : null, // minor units
    currency: p?.currency || null,
    image_urls: imageUrls,
    url: p?.url || null,
    retailer_id: p?.retailerId || null,
    is_hidden: !!p?.isHidden,
    availability: p?.availability || null,
  };
}

// ── DEBUG: raw catalog IQ probe ───────────────────────────────────────────────
// Baileys' getCatalog hangs with no response. This route fires several variants
// of the underlying `w:biz:catalog` IQ while capturing EVERY inbound frame, so
// we can tell the difference between:
//   (a) WhatsApp sends nothing        → server-side refusal / unsupported
//   (b) WhatsApp replies with a different id → Baileys response-routing bug
//   (c) WhatsApp replies with an error node   → actionable error message
function summarizeFrame(f) {
  if (!f || f instanceof Uint8Array) return { binary: true };
  const kids = Array.isArray(f.content)
    ? f.content.map(c => (c && c.tag) || typeof c).slice(0, 6)
    : undefined;
  return { tag: f.tag, attrs: f.attrs, children: kids };
}

app.get("/debug/catalog", async (_req, res) => {
  if (!isConnected || !sock) return res.json({ ok: false, error: "not connected" });
  const jid = ownJid();
  const S_WHATSAPP_NET = "s.whatsapp.net";
  const frames = [];
  const onFrame = (f) => { try { frames.push(summarizeFrame(f)); } catch (_) {} };
  sock.ws.on("frame", onFrame);

  const numNode = (tag, v) => ({ tag, attrs: {}, content: Buffer.from(String(v)) });
  const variants = [
    {
      name: "A: exact Baileys replica",
      node: { tag: "iq", attrs: { to: S_WHATSAPP_NET, type: "get", xmlns: "w:biz:catalog" },
        content: [{ tag: "product_catalog", attrs: { jid, allow_shop_source: "true" },
          content: [numNode("limit", 10), numNode("width", 100), numNode("height", 100)] }] },
    },
    {
      name: "B: + smax_id 35 (like getCollections)",
      node: { tag: "iq", attrs: { to: S_WHATSAPP_NET, type: "get", xmlns: "w:biz:catalog", smax_id: "35" },
        content: [{ tag: "product_catalog", attrs: { jid, allow_shop_source: "true" },
          content: [numNode("limit", 10), numNode("width", 100), numNode("height", 100)] }] },
    },
  ];

  // The earlier 8s probe proved WhatsApp DOES reply (an iq type=result arrived,
  // just after we'd given up). Give it a genuinely generous window now.
  const PROBE_TIMEOUT_MS = 60_000;
  const results = [];
  for (const v of variants) {
    const before = frames.length;
    const started = Date.now();
    try {
      const r = await sock.query(v.node, PROBE_TIMEOUT_MS);
      results.push({
        variant: v.name, outcome: "RESPONDED", ms: Date.now() - started,
        response: summarizeFrame(r),
        framesDuringCall: frames.slice(before),
      });
    } catch (e) {
      results.push({
        variant: v.name, outcome: "FAILED", ms: Date.now() - started,
        error: e && e.message, statusCode: e && e.output && e.output.statusCode,
        framesDuringCall: frames.slice(before),
      });
    }
  }

  sock.ws.off("frame", onFrame);
  res.json({ ok: true, ownJid: jid, results, totalFramesSeen: frames.length });
});

// DEBUG: create a product WITH an image (tests the "image is mandatory" theory).
// Body: { image_url } or { image_base64 }, optional name/price.
app.post("/debug/create-with-image", async (req, res) => {
  if (!isConnected || !sock) return res.json({ ok: false, error: "not connected" });
  const { image_url, image_base64, name, price } = req.body ?? {};
  if (!image_url && !image_base64) {
    return res.status(400).json({ ok: false, error: "image_url or image_base64 required" });
  }
  const frames = [];
  const onFrame = (f) => { try { frames.push(summarizeFrame(f)); } catch (_) {} };
  sock.ws.on("frame", onFrame);
  const started = Date.now();
  try {
    const image = image_url ? { url: image_url } : Buffer.from(image_base64, "base64");
    const product = await createProductRobust({
      name: name || "ZANPOS Image Test",
      description: "Diagnostic product created with an image.",
      price: Math.round(Number(price) || 100),
      currency: "BHD",
      isHidden: false,
      retailerId: "zanpos-diag-1",
      originCountryCode: undefined,
      images: [image],
    }, 90_000);
    sock.ws.off("frame", onFrame);
    res.json({ ok: true, ms: Date.now() - started, product: flattenProduct(product), frames });
  } catch (err) {
    sock.ws.off("frame", onFrame);
    console.error("[wa-sidecar] /debug/create-with-image failed:", err.message);
    res.json({ ok: false, ms: Date.now() - started, error: err.message, frames });
  }
});

// GET /catalog — own Business account's products (paginated)
app.get("/catalog", async (req, res) => {
  if (!isConnected || !sock) return res.json({ ok: false, error: "not connected" });
  try {
    const jid = req.query.jid || ownJid();
    if (!jid) return res.json({ ok: false, error: "own JID unavailable — not fully connected" });
    const limit = Math.min(Number(req.query.limit) || 50, 500);
    const cursor = req.query.cursor || undefined;
    const { products, nextPageCursor } = await withTimeout(
      sock.getCatalog({ jid, limit, cursor }), 60_000, "Reading catalog"
    );
    res.json({
      ok: true,
      products: (products || []).map(flattenProduct),
      next_page_cursor: nextPageCursor || null,
    });
  } catch (err) {
    console.error("[wa-sidecar] /catalog failed:", err.message);
    res.json({ ok: false, error: err.message });
  }
});

// GET /catalog/:jid — peek another business's public catalog
app.get("/catalog/:jid", async (req, res) => {
  if (!isConnected || !sock) return res.json({ ok: false, error: "not connected" });
  try {
    const rawJid = String(req.params.jid || "");
    const jid = rawJid.includes("@") ? rawJid : `${rawJid.replace(/^\+/, "")}@s.whatsapp.net`;
    const limit = Math.min(Number(req.query.limit) || 50, 500);
    const cursor = req.query.cursor || undefined;
    const { products, nextPageCursor } = await withTimeout(
      sock.getCatalog({ jid, limit, cursor }), 60_000, "Reading catalog"
    );
    res.json({
      ok: true,
      products: (products || []).map(flattenProduct),
      next_page_cursor: nextPageCursor || null,
    });
  } catch (err) {
    console.error("[wa-sidecar] /catalog/:jid failed:", err.message);
    res.json({ ok: false, error: err.message });
  }
});

// GET /collections — product collections (groupings)
app.get("/collections", async (req, res) => {
  if (!isConnected || !sock) return res.json({ ok: false, error: "not connected" });
  try {
    const jid = req.query.jid || ownJid() || undefined;
    const limit = Math.min(Number(req.query.limit) || 25, 100);
    const { collections } = await withTimeout(
      sock.getCollections(jid, limit), 60_000, "Reading collections"
    );
    res.json({
      ok: true,
      collections: (collections || []).map(c => ({
        id: c?.id || "",
        name: c?.name || "",
        product_count: Array.isArray(c?.products) ? c.products.length : 0,
      })),
    });
  } catch (err) {
    console.error("[wa-sidecar] /collections failed:", err.message);
    res.json({ ok: false, error: err.message });
  }
});

// POST /catalog/product — create a product
app.post("/catalog/product", async (req, res) => {
  if (!isConnected || !sock) return res.json({ ok: false, error: "not connected" });
  const { name, description, price, currency, images, retailer_id, is_hidden, origin_country } = req.body ?? {};
  if (!name) return res.status(400).json({ ok: false, error: "name required" });
  try {
    // images: array of base64 strings OR http(s) URLs. Baileys wants WAMediaUpload:
    //   Buffer for raw bytes, or { url } for a remote/asset URL.
    const media = (images || []).map(img =>
      typeof img === "string" && (img.startsWith("http") || img.startsWith("data:"))
        ? { url: img }
        : { url: undefined, ...(typeof img === "string" ? { buffer: Buffer.from(img, "base64") } : {}) }
    ).map(m => (m.buffer ? m.buffer : m));
    const product = await createProductRobust({
      name,
      description: description || "",
      price: Math.round(Number(price) || 0), // integer minor units
      currency: currency || "BHD",
      isHidden: !!is_hidden,
      retailerId: retailer_id || undefined,
      originCountryCode: origin_country || undefined,
      images: media,
    });
    res.json({ ok: true, product: flattenProduct(product) });
  } catch (err) {
    console.error("[wa-sidecar] POST /catalog/product failed:", err.message);
    res.json({ ok: false, error: err.message });
  }
});

// PUT /catalog/product/:id — update a product
app.put("/catalog/product/:id", async (req, res) => {
  if (!isConnected || !sock) return res.json({ ok: false, error: "not connected" });
  const id = String(req.params.id || "");
  if (!id) return res.status(400).json({ ok: false, error: "id required" });
  const { name, description, price, currency, images, retailer_id, is_hidden } = req.body ?? {};
  try {
    const media = (images || []).map(img =>
      typeof img === "string" && (img.startsWith("http") || img.startsWith("data:"))
        ? { url: img }
        : Buffer.from(img, "base64")
    );
    const update = {
      name, description: description || "",
      price: Math.round(Number(price) || 0),
      currency: currency || "BHD",
      isHidden: !!is_hidden,
      retailerId: retailer_id || undefined,
      images: media,
    };
    const product = await withTimeout(sock.productUpdate(id, update), 90_000, "Updating product");
    res.json({ ok: true, product: flattenProduct(product) });
  } catch (err) {
    console.error("[wa-sidecar] PUT /catalog/product failed:", err.message);
    res.json({ ok: false, error: err.message });
  }
});

// DELETE /catalog/product — delete product(s) by ID array
app.delete("/catalog/product", async (req, res) => {
  if (!isConnected || !sock) return res.json({ ok: false, error: "not connected" });
  const ids = Array.isArray(req.body?.ids) ? req.body.ids : [];
  if (ids.length === 0) return res.status(400).json({ ok: false, error: "ids array required" });
  try {
    const result = await withTimeout(sock.productDelete(ids), 60_000, "Deleting product");
    res.json({ ok: true, deleted: result?.deleted ?? ids.length });
  } catch (err) {
    console.error("[wa-sidecar] DELETE /catalog/product failed:", err.message);
    res.json({ ok: false, error: err.message });
  }
});

// GET /orders/:id?token=... — resolve a WhatsApp order into line items
app.get("/orders/:id", async (req, res) => {
  if (!isConnected || !sock) return res.json({ ok: false, error: "not connected" });
  const id = String(req.params.id || "");
  const token = req.query.token ? String(req.query.token) : "";
  if (!id || !token) return res.status(400).json({ ok: false, error: "id and token required" });
  try {
    const order = await withTimeout(
      sock.getOrderDetails(id, token), 12_000, "Reading order"
    );
    res.json({
      ok: true,
      order: {
        currency: order?.price?.currency || null,
        total: typeof order?.price?.total === "number" ? order.price.total : null,
        products: (order?.products || []).map(p => ({
          id: p?.id || "",
          name: p?.name || "",
          quantity: Number(p?.quantity) || 0,
          price: typeof p?.price === "number" ? p.price : null,
          currency: p?.currency || null,
          image_url: p?.imageUrl || null,
        })),
      },
    });
  } catch (err) {
    console.error("[wa-sidecar] /orders failed:", err.message);
    res.json({ ok: false, error: err.message });
  }
});

// POST /send-product — send a product from the catalog to a customer.
// Primary path: native product message. Fallback: image + formatted caption
// (robust against WhatsApp protocol quirks in the native product snapshot).
app.post("/send-product", async (req, res) => {
  if (!isConnected || !sock) return res.json({ ok: false, error: "not connected" });
  const { to, product_id } = req.body ?? {};
  if (!to || !product_id) return res.status(400).json({ ok: false, error: "to and product_id required" });
  try {
    const jid = String(to).replace(/^\+/, "") + "@s.whatsapp.net";
    const owner = ownJid();
    // Fetch our own catalog to populate the product card fields.
    let product = null;
    try {
      const { products } = await withTimeout(
        sock.getCatalog({ jid: owner, limit: 500 }), 60_000, "Reading catalog"
      );
      product = (products || []).find(p => p.id === product_id) || null;
    } catch (_) { /* fall through to id-only send */ }

    const imgUrl = product && product.imageUrls
      ? Object.values(product.imageUrls).filter(Boolean)[0]
      : null;

    try {
      await sock.sendMessage(jid, {
        product: {
          productImage: imgUrl ? { url: imgUrl } : undefined,
          productId: product_id,
          title: product?.name || undefined,
          description: product?.description || undefined,
          currencyCode: product?.currency || undefined,
          priceAmount1000: typeof product?.price === "number" ? product.price * 1000 : undefined,
          retailerId: product?.retailerId || undefined,
          url: product?.url || undefined,
        },
        businessOwnerJid: owner,
      });
      return res.json({ ok: true, mode: "product" });
    } catch (nativeErr) {
      console.warn("[wa-sidecar] native product message failed, falling back to image:", nativeErr.message);
      if (imgUrl) {
        const caption = product
          ? `*${product.name}*\n${product.currency || ""} ${(product.price ?? 0) / 1000}\n${product.description || ""}`.trim()
          : "Product";
        await sock.sendMessage(jid, { image: { url: imgUrl }, caption });
        return res.json({ ok: true, mode: "image_fallback" });
      }
      throw nativeErr;
    }
  } catch (err) {
    console.error("[wa-sidecar] /send-product failed:", err.message);
    res.json({ ok: false, error: err.message });
  }
});

app.post("/disconnect", (_req, res) => {
  if (sock) {
    try { sock.logout(); } catch (_) { /* ignore */ }
    sock = null;
  }
  isConnected = false; qrDataUrl = null;
  if (reconnectTimer) { clearTimeout(reconnectTimer); reconnectTimer = null; }
  reconnectAttempts = 0;
  // Clear Baileys auth state so the next connection shows a fresh QR.
  clearAuthState();
  // Clear our own runtime data.
  contactsMap = {};
  try { fs.unlinkSync(contactsFile); } catch (_) { /* may not exist */ }
  groupsMap = {};
  try { fs.unlinkSync(groupsFile); } catch (_) { /* may not exist */ }
  inbox = []; msgSeq = 0; mediaMime = {}; mediaOrder = [];
  try { for (const f of fs.readdirSync(mediaDir)) fs.unlinkSync(path.join(mediaDir, f)); } catch (_) { /* empty */ }
  try { fs.unlinkSync(mediaMimeFile); } catch (_) { /* may not exist */ }
  // Restart Baileys so the frontend immediately gets a QR on next /status poll.
  isStarting = false;
  setTimeout(() => startBaileys(), 500);
  res.json({ ok: true });
});

app.post("/shutdown", (_req, res) => {
  console.log("[wa-sidecar] Shutdown requested — closing gracefully");
  res.json({ ok: true });
  if (sock) { try { sock.end(); } catch (_) { /* ignore */ } }
  sock = null; isConnected = false;
  server.close(() => { console.log("[wa-sidecar] HTTP server closed"); process.exit(0); });
  setTimeout(() => { console.log("[wa-sidecar] Shutdown timeout — forcing exit"); process.exit(0); }, 3000);
});

// ── Start ────────────────────────────────────────────────────────────────────
const server = app.listen(PORT, "127.0.0.1", () => {
  console.log(`[wa-sidecar] Listening on 127.0.0.1:${PORT}, session: ${sessionDir}`);
  startBaileys();
});
server.on("error", (e) => {
  if (e && e.code === "EADDRINUSE") {
    // Probe the existing instance. If healthy, enter standby so the watchdog
    // sees a live process and stops restarting. If dead, retry the bind.
    console.error(`[wa-sidecar] Port ${PORT} in use — probing existing instance`);
    const probeHealth = () => {
      const req = http.get(
        { hostname: "127.0.0.1", port: PORT, path: "/health", timeout: 2000 },
        (res) => {
          res.resume();
          if (res.statusCode === 200) {
            console.log(`[wa-sidecar] Healthy sidecar on :${PORT} — entering standby`);
            // Ping primary every 30 s; leave standby if it dies.
            const iv = setInterval(() => {
              const ping = http.get(
                { hostname: "127.0.0.1", port: PORT, path: "/health", timeout: 2000 },
                (r) => { r.resume(); if (r.statusCode !== 200) { clearInterval(iv); process.exit(1); } }
              );
              ping.on("error", () => { clearInterval(iv); console.log("[wa-sidecar] Primary gone — leaving standby"); process.exit(1); });
              ping.end();
            }, 30000);
          } else {
            console.log(`[wa-sidecar] Existing server unhealthy (${res.statusCode}) — retrying bind in 2 s`);
            setTimeout(() => server.listen(PORT, "127.0.0.1"), 2000);
          }
        }
      );
      req.on("error", () => {
        console.log(`[wa-sidecar] Port blocked by non-responsive process — retrying bind in 2 s`);
        setTimeout(() => server.listen(PORT, "127.0.0.1"), 2000);
      });
      req.end();
    };
    probeHealth();
  } else {
    console.error("[wa-sidecar] HTTP server error:", (e && e.message) || e);
  }
});
process.on("SIGTERM", () => {
  console.log("[wa-sidecar] SIGTERM received — shutting down");
  if (sock) { try { sock.end(); } catch (_) { /* */ } }
  sock = null;
  server.close(() => process.exit(0));
  setTimeout(() => process.exit(0), 3000);
});
