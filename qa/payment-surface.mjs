// The right column is one input zone: order summary at rest, keypad while a
// field is focused. Proves the transition on the Delivery journey.
import { chromium } from "playwright-core";
const EXE = "C:/Users/super/AppData/Local/ms-playwright/chromium-1234/chrome-win64/chrome.exe";
const b = await chromium.launch({ executablePath: EXE });
const ctx = await b.newContext({ viewport: { width: 1024, height: 728 }, hasTouch: true });
const p = await ctx.newPage();
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
for (const code of ["6280123456781", "6280777888991"]) {
  await p.keyboard.type(code, { delay: 5 });
  await p.keyboard.press("Enter");
  await p.waitForTimeout(420);
}
await p.keyboard.press("F7");                       // Delivery journey
await p.waitForTimeout(1100);

const state = async (tag) => {
  const summary = await p.locator(".pm-summary").count();
  const pad = await p.locator(".dialpad").count();
  const field = (await p.locator(".pm-entry-field span").first().textContent().catch(() => "")) || "";
  console.log(`${tag.padEnd(18)} summary=${summary} keypad=${pad} field="${field.trim()}"`);
  await p.screenshot({ path: `qa/layout/pay-surface-${tag.replace(/\W+/g, "-")}.png` });
};

await state("at rest");
await p.locator(".pm-contact-input").first().click();
await p.waitForTimeout(700);
await state("phone focused");
const done = p.locator(".pm-done-btn");
if (await done.count()) { await done.click(); await p.waitForTimeout(700); }
await state("after Done");
await b.close();
