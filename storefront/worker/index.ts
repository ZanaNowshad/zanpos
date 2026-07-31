export interface Env {
  CATALOG: R2Bucket;
  PUBLISH_SECRET: string;
  ASSETS?: Fetcher;
}

const JSON_HEADERS = {
  "content-type": "application/json; charset=utf-8",
  "cache-control": "no-store",
};
const MAX_DRIFT_SECONDS = 300;
const MAX_CATALOG_BYTES = 2_000_000;
const MAX_IMAGE_BYTES = 8_000_000;
const MAX_TELEMETRY_BYTES = 262_144;
// Backups are whole databases, so they need their own far larger ceiling —
// kept well under the Workers request-body limit.
const MAX_BACKUP_BYTES = 64 * 1024 * 1024;
// A heartbeat is a small status ping, not a data upload — bounded tightly.
const MAX_HEARTBEAT_BYTES = 8_192;
const ID_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/;
const DATE_PATTERN = /^\d{4}-\d{2}-\d{2}$/;

const json = (value: unknown, status = 200, headers: HeadersInit = {}) =>
  new Response(JSON.stringify(value), { status, headers: { ...JSON_HEADERS, ...headers } });

const bytesToHex = (bytes: ArrayBuffer) =>
  [...new Uint8Array(bytes)].map((byte) => byte.toString(16).padStart(2, "0")).join("");

const sha256 = async (body: ArrayBuffer | string) => {
  const bytes = typeof body === "string" ? new TextEncoder().encode(body) : body;
  return bytesToHex(await crypto.subtle.digest("SHA-256", bytes));
};

export async function signRequest(secret: string, timestamp: string, method: string, pathname: string, body: ArrayBuffer | string) {
  const key = await crypto.subtle.importKey(
    "raw", new TextEncoder().encode(secret), { name: "HMAC", hash: "SHA-256" }, false, ["sign"],
  );
  const canonical = `${timestamp}\n${method.toUpperCase()}\n${pathname}\n${await sha256(body)}`;
  return bytesToHex(await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(canonical)));
}

const equalConstantTime = (left: string, right: string) => {
  if (left.length !== right.length) return false;
  let mismatch = 0;
  for (let index = 0; index < left.length; index += 1) mismatch |= left.charCodeAt(index) ^ right.charCodeAt(index);
  return mismatch === 0;
};

const imageBytesMatch = (contentType: string, body: ArrayBuffer) => {
  const bytes = new Uint8Array(body);
  const ascii = (start: number, end: number) =>
    String.fromCharCode(...bytes.slice(start, end));
  if (contentType === "image/png") {
    return bytes.length >= 8
      && [137, 80, 78, 71, 13, 10, 26, 10].every((value, index) => bytes[index] === value);
  }
  if (contentType === "image/jpeg") {
    return bytes.length >= 3 && bytes[0] === 255 && bytes[1] === 216 && bytes[2] === 255;
  }
  if (contentType === "image/webp") {
    return bytes.length >= 12 && ascii(0, 4) === "RIFF" && ascii(8, 12) === "WEBP";
  }
  if (contentType === "image/avif") {
    const brand = ascii(8, 12);
    return bytes.length >= 12 && ascii(4, 8) === "ftyp" && (brand === "avif" || brand === "avis");
  }
  return false;
};

async function authenticate(request: Request, env: Env, body: ArrayBuffer) {
  if (!env.PUBLISH_SECRET) return false;
  const timestamp = request.headers.get("x-zanpos-timestamp");
  const signature = request.headers.get("x-zanpos-signature")?.toLowerCase();
  const idempotency = request.headers.get("x-idempotency-key");
  if (!timestamp || !signature || !idempotency || !ID_PATTERN.test(idempotency)) return false;
  const seconds = Number(timestamp);
  if (!Number.isInteger(seconds) || Math.abs(Date.now() / 1000 - seconds) > MAX_DRIFT_SECONDS) return false;
  const expected = await signRequest(env.PUBLISH_SECRET, timestamp, request.method, new URL(request.url).pathname, body);
  return equalConstantTime(signature, expected);
}

const validLocalized = (value: unknown) => {
  if (!value || typeof value !== "object") return false;
  const item = value as Record<string, unknown>;
  return typeof item.en === "string" && item.en.length > 0 && item.en.length <= 200
    && typeof item.ar === "string" && item.ar.length > 0 && item.ar.length <= 200;
};

function validateCatalog(value: unknown, version: string) {
  if (!value || typeof value !== "object") return "Catalog must be an object";
  const data = value as Record<string, unknown>;
  if (data.version !== version || !ID_PATTERN.test(version)) return "Catalog version must match the route";
  if (typeof data.updatedAt !== "string" || !Number.isFinite(Date.parse(data.updatedAt))) return "updatedAt must be an ISO date";
  const store = data.store as Record<string, unknown> | undefined;
  if (!store || !validLocalized(store.name) || typeof store.phone !== "string" || !/^\+?\d{7,15}$/.test(store.phone)) return "Store metadata is invalid";
  const currency = data.currency as Record<string, unknown> | undefined;
  if (!currency || typeof currency.code !== "string" || !/^[A-Z]{3}$/.test(currency.code)
    || !Number.isInteger(currency.decimals) || Number(currency.decimals) < 0 || Number(currency.decimals) > 6) return "Currency is invalid";
  if (!Array.isArray(data.categories) || data.categories.length > 100) return "Categories are invalid";
  if (!Array.isArray(data.products) || data.products.length > 10_000) return "Products are invalid";
  const categoryIds = new Set<string>();
  for (const raw of data.categories) {
    const item = raw as Record<string, unknown>;
    if (typeof item.id !== "string" || !ID_PATTERN.test(item.id) || categoryIds.has(item.id) || !validLocalized(item.name)) return "A category is invalid";
    categoryIds.add(item.id);
  }
  const productIds = new Set<string>();
  for (const raw of data.products) {
    const item = raw as Record<string, unknown>;
    if (typeof item.id !== "string" || !ID_PATTERN.test(item.id) || productIds.has(item.id) || !validLocalized(item.name)
      || typeof item.categoryId !== "string" || !categoryIds.has(item.categoryId)
      || !Number.isSafeInteger(item.priceMinor) || Number(item.priceMinor) < 0 || typeof item.available !== "boolean") return "A product is invalid";
    productIds.add(item.id);
  }
  return null;
}

async function r2Response(object: R2ObjectBody | null, cacheControl: string) {
  if (!object) return json({ error: "Not found" }, 404);
  const headers = new Headers();
  object.writeHttpMetadata(headers);
  headers.set("etag", object.httpEtag);
  headers.set("cache-control", cacheControl);
  return new Response(object.body, { headers });
}

async function publishCatalog(request: Request, env: Env, version: string, body: ArrayBuffer) {
  if (body.byteLength > MAX_CATALOG_BYTES) return json({ error: "Catalog is too large" }, 413);
  let catalog: unknown;
  try { catalog = JSON.parse(new TextDecoder().decode(body)); } catch { return json({ error: "Catalog JSON is invalid" }, 400); }
  const invalid = validateCatalog(catalog, version);
  if (invalid) return json({ error: invalid }, 400);
  const key = `releases/${version}.json`;
  const existing = await env.CATALOG.get(key);
  const digest = await sha256(body);
  if (existing) {
    const current = await existing.arrayBuffer();
    return (await sha256(current)) === digest
      ? json({ version, alreadyPublished: true })
      : json({ error: "Version already exists with different content" }, 409);
  }
  await env.CATALOG.put(key, body, { httpMetadata: { contentType: "application/json; charset=utf-8" }, customMetadata: { digest } });
  return json({ version, digest }, 201);
}

async function commitRelease(env: Env, version: string) {
  const release = await env.CATALOG.get(`releases/${version}.json`);
  if (!release) return json({ error: "Release does not exist" }, 404);
  const pointer = await env.CATALOG.get("public/catalog/latest.json");
  if (pointer) {
    const current = await pointer.json<{ version: string }>();
    if (current.version === version) return json({ version, alreadyCommitted: true });
  }
  await env.CATALOG.put("public/catalog/latest.json", JSON.stringify({ version }), { httpMetadata: { contentType: "application/json" } });
  return json({ version, committed: true });
}

async function publicCatalog(env: Env) {
  const pointer = await env.CATALOG.get("public/catalog/latest.json");
  if (!pointer) return json({ error: "Catalog is not published" }, 404);
  const { version } = await pointer.json<{ version: string }>();
  if (!ID_PATTERN.test(version)) return json({ error: "Catalog pointer is invalid" }, 500);
  return r2Response(await env.CATALOG.get(`releases/${version}.json`), "public, max-age=60, stale-while-revalidate=86400");
}

// Static update manifest for the Tauri updater (src-tauri/tauri.conf.json,
// src-tauri/src/commands/updater_commands.rs). Public and unauthenticated —
// the updater carries no credentials — so the response is rebuilt field by
// field rather than proxying the R2 object: whatever else might be stored
// alongside the manifest never reaches the client.
async function updateManifest(env: Env) {
  const object = await env.CATALOG.get("public/updates/latest.json");
  if (!object) return json({ error: "Not found" }, 404);
  let raw: unknown;
  try { raw = await object.json(); } catch { return json({ error: "Not found" }, 404); }
  if (!raw || typeof raw !== "object") return json({ error: "Not found" }, 404);
  const manifest = raw as Record<string, unknown>;
  if (typeof manifest.version !== "string" || typeof manifest.url !== "string" || typeof manifest.signature !== "string") {
    return json({ error: "Not found" }, 404);
  }
  return json({
    version: manifest.version,
    notes: typeof manifest.notes === "string" ? manifest.notes : "",
    pub_date: typeof manifest.pub_date === "string" ? manifest.pub_date : "",
    url: manifest.url,
    signature: manifest.signature,
    critical: manifest.critical === true,
  });
}

// Serve the storefront SPA from R2 (`site/**` keys) when no ASSETS binding is
// present. ZANPOS's one-click deploy uploads the built SPA to R2 so the worker
// can be deployed via the plain Cloudflare API (no wrangler, no asset manifest).
async function siteFromR2(env: Env, pathname: string) {
  let key = pathname.replace(/^\/+/, "");
  if (key === "" || key.endsWith("/")) key += "index.html";
  // Only serve safe, known paths; anything else falls back to the SPA shell.
  if (!/^[A-Za-z0-9][A-Za-z0-9/._-]{0,255}$/.test(key) || key.includes("..")) key = "index.html";
  let object = await env.CATALOG.get(`site/${key}`);
  if (!object && !key.includes(".")) object = await env.CATALOG.get("site/index.html"); // SPA fallback
  if (!object) return json({ error: "Storefront is not deployed" }, 404);
  const cache = key === "index.html" || key.endsWith(".html")
    ? "public, max-age=60"
    : "public, max-age=31536000, immutable";
  return r2Response(object, cache);
}

interface TelemetryPayload {
  store: string;
  table: string;
  date: string;
  records: unknown[];
}

// Diagnostics/analytics batch upload. Deliberately one small R2 object per
// (store, date, idempotency key) rather than one shared append-only file per
// day — a retried batch overwrites the same key harmlessly instead of racing
// a read-modify-write against concurrent uploads. Reuses the same CATALOG R2
// bucket and HMAC auth as catalog publishing; no new binding or secret.
async function telemetryUpload(request: Request, env: Env) {
  const length = Number(request.headers.get("content-length") ?? 0);
  if (length > MAX_TELEMETRY_BYTES) return json({ error: "Payload is too large" }, 413);
  const body = await request.arrayBuffer();
  if (!await authenticate(request, env, body)) return json({ error: "Unauthorized" }, 401);

  let payload: Partial<TelemetryPayload>;
  try { payload = JSON.parse(new TextDecoder().decode(body)); } catch { return json({ error: "Telemetry JSON is invalid" }, 400); }
  if (typeof payload.store !== "string" || !ID_PATTERN.test(payload.store)) return json({ error: "store is invalid" }, 400);
  if (payload.table !== "diagnostics" && payload.table !== "analytics_events") return json({ error: "table is invalid" }, 400);
  if (typeof payload.date !== "string" || !DATE_PATTERN.test(payload.date)) return json({ error: "date is invalid" }, 400);
  if (!Array.isArray(payload.records)) return json({ error: "records must be an array" }, 400);

  // authenticate() already validated this against ID_PATTERN.
  const idempotency = request.headers.get("x-idempotency-key");
  const key = `telemetry/${payload.store}/${payload.date}/${idempotency}.jsonl`;
  const lines = payload.records.map((record) => JSON.stringify(record)).join("\n");
  await env.CATALOG.put(key, lines, { httpMetadata: { contentType: "application/x-ndjson; charset=utf-8" } });
  return json({ ok: true });
}

// Encrypted off-site database backup. The body is XChaCha20-Poly1305
// ciphertext produced on the till (src-tauri/src/backup.rs); the worker never
// sees plaintext and holds no decryption key — it is storage, not a trusted
// party. One object per (store, date): a same-day retry overwrites rather than
// accumulating copies of a database.
async function backupUpload(request: Request, env: Env) {
  const length = Number(request.headers.get("content-length") ?? 0);
  if (length > MAX_BACKUP_BYTES) return json({ error: "Payload is too large" }, 413);
  const body = await request.arrayBuffer();
  if (body.byteLength > MAX_BACKUP_BYTES) return json({ error: "Payload is too large" }, 413);
  if (!await authenticate(request, env, body)) return json({ error: "Unauthorized" }, 401);

  // Both go into the object key, so both are pattern-checked — neither can
  // contain a slash and walk out of the backups/ prefix.
  const store = request.headers.get("x-zanpos-store");
  const date = request.headers.get("x-zanpos-date");
  if (!store || !ID_PATTERN.test(store)) return json({ error: "store is invalid" }, 400);
  if (!date || !DATE_PATTERN.test(date)) return json({ error: "date is invalid" }, 400);

  await env.CATALOG.put(`backups/${store}/${date}.enc`, body, {
    httpMetadata: { contentType: "application/octet-stream" },
  });
  return json({ ok: true });
}

interface LicenseHeartbeatPayload {
  store: string;
  date: string;
  licenseKey?: string;
  tier?: string;
  state?: string;
}

// Advisory-only licensing ping. ABSOLUTE RULE (product spec, mirrored from
// src-tauri/src/license/mod.rs): the ABSENCE of a heartbeat must NEVER cause
// lockout. This endpoint records that a till phoned home with its current
// entitlement so the vendor can see adoption/expiry trends — it grants no
// entitlement and revokes none. A store that never calls this, or whose
// calls always fail (offline, network error, auth failure), keeps selling
// exactly as it always did. Same HMAC auth as the other POST routes so a
// heartbeat can't be forged or replayed — that is the only thing the auth
// check guards here, not licensing itself.
async function licenseHeartbeat(request: Request, env: Env) {
  const length = Number(request.headers.get("content-length") ?? 0);
  if (length > MAX_HEARTBEAT_BYTES) return json({ error: "Payload is too large" }, 413);
  const body = await request.arrayBuffer();
  if (!await authenticate(request, env, body)) return json({ error: "Unauthorized" }, 401);

  let payload: Partial<LicenseHeartbeatPayload>;
  try { payload = JSON.parse(new TextDecoder().decode(body)); } catch { return json({ error: "Heartbeat JSON is invalid" }, 400); }
  if (typeof payload.store !== "string" || !ID_PATTERN.test(payload.store)) return json({ error: "store is invalid" }, 400);
  if (typeof payload.date !== "string" || !DATE_PATTERN.test(payload.date)) return json({ error: "date is invalid" }, 400);

  // authenticate() already validated this against ID_PATTERN.
  const idempotency = request.headers.get("x-idempotency-key");
  const key = `license-heartbeats/${payload.store}/${payload.date}/${idempotency}.json`;
  await env.CATALOG.put(key, JSON.stringify(payload), { httpMetadata: { contentType: "application/json; charset=utf-8" } });
  return json({ ok: true });
}

async function route(request: Request, env: Env) {
  const url = new URL(request.url);
  if (request.method === "GET" && url.pathname === "/api/catalog") return publicCatalog(env);
  const imageRead = url.pathname.match(/^\/api\/images\/([A-Za-z0-9][A-Za-z0-9._-]{0,127})$/);
  if (request.method === "GET" && imageRead) return r2Response(await env.CATALOG.get(`public/images/${imageRead[1]}`), "public, max-age=31536000, immutable");
  if (request.method === "GET" && url.pathname === "/api/updates/latest.json") return updateManifest(env);
  if (request.method === "POST" && url.pathname === "/api/telemetry") return telemetryUpload(request, env);
  if (request.method === "POST" && url.pathname === "/api/backup") return backupUpload(request, env);
  if (request.method === "POST" && url.pathname === "/api/license/heartbeat") return licenseHeartbeat(request, env);

  if (!url.pathname.startsWith("/api/publish/")) {
    if (env.ASSETS) return env.ASSETS.fetch(request);
    if (request.method === "GET" || request.method === "HEAD") return siteFromR2(env, url.pathname);
    return json({ error: "Not found" }, 404);
  }
  const length = Number(request.headers.get("content-length") ?? 0);
  if (length > MAX_IMAGE_BYTES) return json({ error: "Payload is too large" }, 413);
  const body = await request.arrayBuffer();
  if (!await authenticate(request, env, body)) return json({ error: "Unauthorized" }, 401);

  const catalogMatch = url.pathname.match(/^\/api\/publish\/catalog\/([A-Za-z0-9][A-Za-z0-9._-]{0,127})$/);
  if (request.method === "PUT" && catalogMatch) return publishCatalog(request, env, catalogMatch[1], body);
  const commitMatch = url.pathname.match(/^\/api\/publish\/releases\/([A-Za-z0-9][A-Za-z0-9._-]{0,127})\/commit$/);
  if (request.method === "POST" && commitMatch) return commitRelease(env, commitMatch[1]);
  const imageMatch = url.pathname.match(/^\/api\/publish\/images\/([A-Za-z0-9][A-Za-z0-9._-]{0,127})$/);
  if (request.method === "PUT" && imageMatch) {
    if (!body.byteLength || body.byteLength > MAX_IMAGE_BYTES) return json({ error: "Image size is invalid" }, 413);
    const contentType = request.headers.get("content-type") ?? "";
    if (!/^image\/(avif|jpeg|png|webp)$/.test(contentType)) return json({ error: "Image type is invalid" }, 415);
    if (!imageBytesMatch(contentType, body)) return json({ error: "Image bytes do not match the declared type" }, 415);
    const key = `public/images/${imageMatch[1]}`;
    const existing = await env.CATALOG.get(key);
    const digest = await sha256(body);
    if (existing?.customMetadata?.digest === digest) return json({ id: imageMatch[1], alreadyPublished: true });
    if (existing) return json({ error: "Image id already exists with different content" }, 409);
    await env.CATALOG.put(key, body, { httpMetadata: { contentType }, customMetadata: { digest } });
    return json({ id: imageMatch[1], url: `/api/images/${imageMatch[1]}`, digest }, 201);
  }
  return json({ error: "Not found" }, 404);
}

export default {
  async fetch(request: Request, env: Env) {
    try { return await route(request, env); }
    catch { return json({ error: "Internal server error" }, 500); }
  },
} satisfies ExportedHandler<Env>;
