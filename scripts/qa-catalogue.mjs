import { chromium } from "./../storefront/node_modules/@playwright/test/index.mjs";
const URL = "http://localhost:1420/?uimock=1";
const b = await chromium.launch();
const pg = await b.newPage({ viewport: { width: 1536, height: 1024 }, deviceScaleFactor: 1 });
const errs = [];
pg.on("console", m => { if (m.type() === "error") errs.push(m.text()); });
pg.on("pageerror", e => errs.push(String(e)));
await pg.goto(URL, { waitUntil: "networkidle" });
await pg.waitForTimeout(900);
// Navigate: Catalogue domain
const nav = pg.locator('button:has-text("Catalogue"), a:has-text("Catalogue")').first();
if (await nav.count()) { await nav.click(); await pg.waitForTimeout(1200); }
const probe = await pg.evaluate(() => {
  const el = document.querySelector(".zp-cat-counts");
  const tbl = document.querySelector(".zp-table");
  const doc = document.documentElement;
  return {
    theme: doc.getAttribute("data-theme"),
    h1: document.querySelector("h1")?.textContent ?? null,
    subtitle: document.querySelector(".zp-page-subtitle, .zp-page-header p")?.textContent ?? null,
    countsStrip: el ? [...el.querySelectorAll("div")].map(d => d.textContent.trim()) : null,
    headers: tbl ? [...tbl.querySelectorAll("thead th")].map(t => t.textContent.trim()) : null,
    rows: tbl ? tbl.querySelectorAll("tbody tr").length : 0,
    hOverflow: doc.scrollWidth > doc.clientWidth,
    // anything painting past the main column's right edge?
    spill: (() => {
      const main = document.querySelector(".zp-page, .oa-page, main");
      if (!main) return null;
      const r = main.getBoundingClientRect();
      return [...main.querySelectorAll("*")]
        .filter(n => n.getBoundingClientRect().right > r.right + 1)
        .slice(0, 6).map(n => n.className?.toString().slice(0, 60) || n.tagName);
    })(),
  };
});
await pg.screenshot({ path: "qa/parity/catalogue-final.png", fullPage: false });
console.log(JSON.stringify({ probe, errs }, null, 1));
await b.close();
