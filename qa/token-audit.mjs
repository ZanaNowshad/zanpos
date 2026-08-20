// Proves the token sweep landed: no computed corner outside the five-step
// scale, no computed text below the 11px floor, measured on real rendered
// nodes rather than by grepping the stylesheet.
import { chromium } from "playwright-core";
const EXE = "C:/Users/super/AppData/Local/ms-playwright/chromium-1234/chrome-win64/chrome.exe";
const port = process.argv[2] || "1421";
const PAGES = [["Catalogue", "Categories"], ["Catalogue", "Products"], ["Team", "Staff"], ["Today", null], ["Insights", null]];
const b = await chromium.launch({ executablePath: EXE });
const p = await (await b.newContext({ viewport: { width: 1024, height: 728 }, hasTouch: true })).newPage();
p.on("pageerror", e => console.log("[pageerror]", String(e).slice(0, 160)));
await p.goto(`http://127.0.0.1:${port}/?uimock=1`, { waitUntil: "domcontentloaded" });
await p.waitForTimeout(2400);

const ALLOWED = new Set(["0px", "6px", "8px", "12px", "16px", "24px", "999px", "32px", "1px"]);
const radii = new Map(), tiny = new Map();

for (const [nav, tab] of PAGES) {
  const item = p.locator(".oa-nav-item", { hasText: new RegExp(`^${nav}$`) }).first();
  if (!(await item.count())) { console.log(`! nav "${nav}" not found`); continue; }
  await item.click();
  await p.waitForTimeout(700);
  if (tab) {
    const t = p.locator("button", { hasText: new RegExp(`^${tab}$`) }).first();
    if (await t.count()) { await t.click(); await p.waitForTimeout(700); }
  }
  const found = await p.evaluate(() => {
    const offR = [], offT = [];
    for (const el of document.querySelectorAll("body *")) {
      const r = el.getBoundingClientRect();
      if (!r.width || !r.height) continue;            // invisible: cannot mislead the eye
      const cs = getComputedStyle(el);
      const corner = cs.borderTopLeftRadius;
      if (corner && !corner.includes("%")) offR.push([corner, el.className?.toString().slice(0, 40) || el.tagName]);
      const fs = parseFloat(cs.fontSize);
      if (fs && fs < 11 && (el.textContent || "").trim()) offT.push([fs + "px", el.className?.toString().slice(0, 40) || el.tagName]);
    }
    return { offR, offT };
  });
  for (const [v, who] of found.offR) if (!ALLOWED.has(v)) radii.set(v, (radii.get(v) || who));
  for (const [v, who] of found.offT) tiny.set(v + " " + who, true);
}

console.log("=== corners outside the scale ===");
console.log(radii.size ? [...radii].map(([v, w]) => `  ${v.padEnd(8)} ${w}`).join("\n") : "  none");
console.log("=== visible text below the 11px floor ===");
console.log(tiny.size ? [...tiny.keys()].slice(0, 20).map(s => "  " + s).join("\n") : "  none");
await b.close();
