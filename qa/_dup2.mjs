import { chromium } from "playwright-core";
const EXE = "C:/Users/super/AppData/Local/ms-playwright/chromium-1234/chrome-win64/chrome.exe";
const b = await chromium.launch({ executablePath: EXE });
const p = await (await b.newContext({ viewport:{width:1024,height:728}, hasTouch:true })).newPage();
await p.goto("http://127.0.0.1:1421/?uimock=1", { waitUntil: "domcontentloaded" });
await p.waitForTimeout(2400);
await p.locator(".oa-nav-item", { hasText: /^Catalogue$/ }).first().click();
await p.waitForTimeout(900);
await p.locator("button", { hasText: /^Duplicates$/ }).first().click();
await p.waitForTimeout(1500);
console.log(await p.evaluate(() => {
  const pick = (sel) => {
    const el = document.querySelector(sel);
    if (!el) return `${sel}: MISSING`;
    const cs = getComputedStyle(el);
    const r = el.getBoundingClientRect();
    return `${sel}: ${Math.round(r.width)}x${Math.round(r.height)} color=${cs.color} bg=${cs.backgroundColor} font=${cs.fontSize} display=${cs.display} overflow=${cs.overflow} opacity=${cs.opacity} vis=${cs.visibility}`;
  };
  return [
    pick(".dup-group"), pick(".dup-group-head"), pick(".dup-rows"),
    pick(".dup-row"), pick(".dup-row-info"), pick(".dup-row-name"),
  ].join("\n");
}));
await b.close();
