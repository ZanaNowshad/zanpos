import { chromium } from "./../storefront/node_modules/@playwright/test/index.mjs";
const slug = process.argv[2] || "purchasing-before";
const b = await chromium.launch();
const pg = await b.newPage({ viewport: { width: 1536, height: 1024 } });
const errs = [];
pg.on("console", m => { if (m.type() === "error") errs.push(m.text()); });
pg.on("pageerror", e => errs.push(String(e)));
await pg.goto("http://localhost:1420/?uimock=1", { waitUntil: "networkidle" });
await pg.waitForTimeout(800);
await pg.locator('button:has-text("Purchasing")').first().click();
await pg.waitForTimeout(1400);
const probe = await pg.evaluate(() => {
  const tbl = document.querySelector(".zp-table");
  return {
    h1: document.querySelector("h1")?.textContent ?? null,
    subtitle: document.querySelector(".zp-page-subtitle, .zp-page-header p")?.textContent ?? null,
    subnav: [...document.querySelectorAll(".zp-workspace-nav a, .zp-workspace-nav button, nav button")].map(n=>n.textContent.trim()).filter(Boolean).slice(0,12),
    toolbar: [...document.querySelectorAll(".zp-toolbar *")].filter(n=>["BUTTON","SELECT","INPUT"].includes(n.tagName)).map(n=>n.tagName+":"+(n.textContent.trim()||n.placeholder||"")).slice(0,12),
    headerActions: [...document.querySelectorAll(".zp-page-header button, .zp-page-actions button")].map(n=>n.textContent.trim()),
    headers: tbl ? [...tbl.querySelectorAll("thead th")].map(t=>t.textContent.trim()) : null,
    rows: tbl ? tbl.querySelectorAll("tbody tr").length : 0,
    firstRow: tbl ? [...(tbl.querySelector("tbody tr")?.children ?? [])].map(c=>c.textContent.trim()) : null,
    metrics: [...document.querySelectorAll(".zp-cat-counts > div, .zp-metric, .oa-kpi")].map(d=>d.textContent.trim()).slice(0,8),
    empty: document.querySelector(".zp-empty")?.textContent.trim().slice(0,140) ?? null,
    overflow: document.documentElement.scrollWidth > document.documentElement.clientWidth,
  };
});
await pg.screenshot({ path: `qa/parity/${slug}.png` });
console.log(JSON.stringify({ probe, errs }, null, 1));
await b.close();
