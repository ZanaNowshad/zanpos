// Captures admin pages at the till's real spec, and measures how much vertical
// space the shell spends before content begins.
import { chromium } from "playwright-core";
const EXE = "C:/Users/super/AppData/Local/ms-playwright/chromium-1234/chrome-win64/chrome.exe";
const port = process.argv[2] || "1421";
const PAGES = [["Catalogue", "Categories"], ["Catalogue", "Products"], ["Team", "Staff"], ["Today", null]];
const b = await chromium.launch({ executablePath: EXE });
const p = await (await b.newContext({ viewport: { width: 1024, height: 728 }, hasTouch: true })).newPage();
p.on("pageerror", e => console.log("[pageerror]", String(e).slice(0, 160)));
await p.goto(`http://127.0.0.1:${port}/?uimock=1`, { waitUntil: "domcontentloaded" });
await p.waitForTimeout(2400);
for (const [nav, tab] of PAGES) {
  await p.locator(".oa-nav-item", { hasText: new RegExp(`^${nav}$`) }).first().click();
  await p.waitForTimeout(800);
  if (tab) {
    const t = p.locator("button", { hasText: new RegExp(`^${tab}$`) }).first();
    if (await t.count()) { await t.click(); await p.waitForTimeout(800); }
  }
  const chrome = await p.evaluate(() => {
    const content = document.querySelector(".zp-domain-content .oa-embedded-tab, .zp-domain-content section");
    const top = content ? Math.round(content.getBoundingClientRect().top) : -1;
    const crumbs = document.querySelectorAll(".zp-breadcrumb").length;
    const h1 = document.querySelector(".oa-title");
    const h1Visible = h1 ? !h1.className.includes("zp-visually-hidden") : false;
    return { top, crumbs, h1Visible, h1Text: h1?.textContent ?? "" };
  });
  const label = `${nav}${tab ? "/" + tab : ""}`;
  console.log(`${label.padEnd(22)} content starts y=${chrome.top}  breadcrumbs=${chrome.crumbs}  h1 shown=${chrome.h1Visible} "${chrome.h1Text}"`);
  await p.screenshot({ path: `qa/layout/admin-${label.replace(/\W+/g, "-")}.png` });
}
await b.close();
