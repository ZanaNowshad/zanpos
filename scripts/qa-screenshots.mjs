/**
 * Visual QA capture for ZANPOS Command.
 *
 * Drives the real frontend against the dev-only Tauri mock (?uimock=1) and
 * writes PNGs so the redesign can be inspected as pixels, not just DOM
 * geometry. The native Tauri window cannot be screenshotted by the agent
 * tooling (the dev binary is not an installed app, so it is masked), which is
 * why capture goes through a browser instead.
 *
 *   node scripts/qa-screenshots.mjs [baseUrl] [outDir]
 *
 * Requires the dev server to be running on the given port.
 */
import playwright from "../storefront/node_modules/@playwright/test/index.js";
import { mkdir } from "node:fs/promises";

const { chromium } = playwright;
import path from "node:path";

const BASE = process.argv[2] ?? "http://localhost:1420";
const OUT = process.argv[3] ?? "qa-artifacts";

/** Rail order must match src/navigation/config.tsx. */
const DOMAINS = [
  ["today", 0], ["sell", 1], ["catalogue", 2], ["purchasing", 3],
  ["customers", 4], ["team", 5], ["insights", 6], ["review", 7], ["system", 8],
];

const VIEWPORTS = [
  ["1920", 1920, 1080],
  ["1440", 1440, 900],
  ["1280", 1280, 800],
  ["1024", 1024, 800],
  ["768", 768, 1024],
];

async function gotoApp(page) {
  await page.goto(`${BASE}/?uimock=1`, { waitUntil: "networkidle" });
  await page.waitForSelector(".oa-nav-item", { timeout: 15000 });
  await page.waitForTimeout(500);
}

async function selectDomain(page, index) {
  await page.locator(".oa-nav-item").nth(index).click();
  await page.waitForTimeout(700);
}

async function main() {
  await mkdir(OUT, { recursive: true });
  const browser = await chromium.launch();
  const shots = [];

  // 1. Every domain at 1920 — the primary review size.
  const page = await browser.newPage({ viewport: { width: 1920, height: 1080 } });
  await gotoApp(page);
  for (const [name, idx] of DOMAINS) {
    await selectDomain(page, idx);
    const file = path.join(OUT, `domain-${name}-1920.png`);
    await page.screenshot({ path: file });
    shots.push(file);
  }

  // 2. Products across every breakpoint.
  for (const [label, w, h] of VIEWPORTS) {
    await page.setViewportSize({ width: w, height: h });
    await selectDomain(page, 2);
    await page.waitForTimeout(500);
    const file = path.join(OUT, `products-${label}.png`);
    await page.screenshot({ path: file });
    shots.push(file);
  }

  // 3. Products empty states, driven from real application state.
  await page.setViewportSize({ width: 1440, height: 900 });
  await selectDomain(page, 2);

  await page.fill(".zp-search input", "zzzznotfound");
  await page.waitForTimeout(1200);
  await page.screenshot({ path: path.join(OUT, "products-no-results.png") });
  shots.push(path.join(OUT, "products-no-results.png"));

  await page.evaluate(() => {
    const orig = window.__TAURI_INTERNALS__.invoke;
    window.__TAURI_INTERNALS__.invoke = (c, a) =>
      c === "admin_list_products" ? Promise.resolve({ items: [], total: 0 }) : orig(c, a);
  });
  await page.fill(".zp-search input", "");
  await page.waitForTimeout(1400);
  await page.screenshot({ path: path.join(OUT, "products-first-use.png") });
  shots.push(path.join(OUT, "products-first-use.png"));

  await page.evaluate(() => {
    const orig = window.__TAURI_INTERNALS__.invoke;
    window.__TAURI_INTERNALS__.invoke = (c, a) =>
      c === "admin_list_products" ? Promise.reject(new Error("Database is locked")) : orig(c, a);
  });
  await page.fill(".zp-search input", "milk");
  await page.waitForTimeout(1400);
  await page.screenshot({ path: path.join(OUT, "products-degraded.png") });
  shots.push(path.join(OUT, "products-degraded.png"));

  // 4. Dark theme sanity at 1440.
  await page.reload({ waitUntil: "networkidle" });
  await page.waitForSelector(".oa-nav-item");
  await page.evaluate(() => document.documentElement.setAttribute("data-theme", "dark"));
  await page.waitForTimeout(400);
  await page.screenshot({ path: path.join(OUT, "today-dark-1440.png") });
  shots.push(path.join(OUT, "today-dark-1440.png"));

  await browser.close();
  console.log(shots.join("\n"));
}

main().catch(err => { console.error(err); process.exit(1); });
