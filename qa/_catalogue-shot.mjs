import { chromium } from "playwright-core";
const EXE = "C:/Users/super/AppData/Local/ms-playwright/chromium-1234/chrome-win64/chrome.exe";
const port = process.argv[2] || "1421";
const b = await chromium.launch({ executablePath: EXE });
const ctx = await b.newContext({ viewport: { width: 1440, height: 900 } });
const p = await ctx.newPage();
p.on("pageerror", e => console.log("[pageerror]", String(e).slice(0, 160)));
await p.goto(`http://127.0.0.1:${port}/?uimock=1`, { waitUntil: "domcontentloaded" });
await p.waitForTimeout(2600);
const cat = p.locator(".oa-primary-nav-items button, nav button", { hasText: /Catalogue/i }).first();
if (await cat.count()) { await cat.click(); await p.waitForTimeout(1200); }
const prod = p.locator("button, a", { hasText: /^Products$/ }).first();
if (await prod.count()) { await prod.click(); await p.waitForTimeout(1600); }
await p.waitForTimeout(1200);
await p.screenshot({ path: "qa/layout/catalogue-products.png" });
console.log("thumbs:", await p.locator(".product-catalogue-thumb img").count());
await b.close();
