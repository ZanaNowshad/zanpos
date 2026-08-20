// Pressing a blocked "complete sale" must say what is wrong and move the caret
// to the first field that needs fixing — never sit there dead.
import { chromium } from "playwright-core";
const EXE = "C:/Users/super/AppData/Local/ms-playwright/chromium-1234/chrome-win64/chrome.exe";
const b = await chromium.launch({ executablePath: EXE });
const p = await (await b.newContext({ viewport: { width: 1024, height: 728 }, hasTouch: true })).newPage();
p.on("pageerror", e => console.log("[pageerror]", String(e).slice(0, 180)));
await p.goto("http://127.0.0.1:1421/?uimock=1", { waitUntil: "domcontentloaded" });
await p.waitForTimeout(2400);
await p.locator(".oa-back-btn").first().click();
await p.waitForSelector(".login-screen", { timeout: 8000 });
await p.waitForTimeout(600);
await p.locator(".user-card").first().click();
await p.waitForTimeout(500);
await p.keyboard.type("1234", { delay: 60 });
await p.keyboard.press("Enter");
await p.waitForSelector(".pos-layout", { timeout: 10000 });
await p.waitForTimeout(1400);
await p.keyboard.type("6280123456781", { delay: 5 });
await p.keyboard.press("Enter");
await p.waitForTimeout(500);
await p.keyboard.press("F7");                       // Delivery: nothing filled in
await p.waitForTimeout(1000);

const confirm = p.locator(".pm-confirm-btn");
console.log("disabled attr :", await confirm.isDisabled());
console.log("blockers shown:", await p.locator(".pm-blockers li").count());
await confirm.click();
await p.waitForTimeout(600);
const items = await p.locator(".pm-blockers li").allTextContents();
console.log("after press   :", items.length, "reason(s)");
for (const t of items) console.log("   •", t.trim());
console.log("focused now   :", await p.evaluate(() => document.activeElement?.className || "?"));
await p.screenshot({ path: "qa/layout/pay-blocked.png" });
await b.close();
