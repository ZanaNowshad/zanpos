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
await p.waitForTimeout(1500);
// Scan a basket so the cart is real.
for (const code of ["6280123456781","6280987654321","6280555123451","6280444333221","6280777888991","6280222111334"]) {
  await p.keyboard.type(code, { delay: 5 });
  await p.keyboard.press("Enter");
  await p.waitForTimeout(450);
}
await p.waitForTimeout(900);
await p.screenshot({ path: "qa/layout/till-new-1024x728.png" });
console.log("rows:", await p.locator(".till-row").count());
console.log("total:", (await p.locator(".till-grand-value").first().textContent().catch(()=>null)) || "?");
await b.close();
