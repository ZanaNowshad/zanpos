import { chromium } from "playwright-core";
const EXE = "C:/Users/super/AppData/Local/ms-playwright/chromium-1234/chrome-win64/chrome.exe";
const b = await chromium.launch({ executablePath: EXE });
const ctx = await b.newContext({ viewport: { width: 1024, height: 728 }, hasTouch: true });
const p = await ctx.newPage();
p.on("pageerror", e => console.log("[pageerror]", String(e).slice(0,200)));
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
for (const code of ["6280123456781","6280987654321","6280555123451"]) {
  await p.keyboard.type(code, { delay: 5 });
  await p.keyboard.press("Enter");
  await p.waitForTimeout(400);
}
// 1. sidebar rail
await p.locator(".top-bar-sidebar-toggle").first().click();
await p.waitForTimeout(700);
await p.screenshot({ path: "qa/layout/till-rail.png" });
console.log("rail items:", await p.locator(".pos-sidebar-item").count());
// 2. More Options (F9)
await p.keyboard.press("F9");
await p.waitForTimeout(700);
await p.screenshot({ path: "qa/layout/till-more.png" });
console.log("more items:", await p.locator(".till-more-item").count());
await p.keyboard.press("Escape");
await p.locator(".till-more-close").click().catch(()=>{});
await p.waitForTimeout(500);
// 3. Qty pad (F3)
await p.keyboard.press("F3");
await p.waitForTimeout(700);
await p.screenshot({ path: "qa/layout/till-qtypad.png" });
console.log("qtypad:", await p.locator(".till-qtypad").count());
await b.close();
