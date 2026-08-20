// Renders each checkout journey on the till at its real spec.
//   node qa/payment-journeys.mjs [port]
import { chromium } from "playwright-core";
const EXE = "C:/Users/super/AppData/Local/ms-playwright/chromium-1234/chrome-win64/chrome.exe";
const port = process.argv[2] || "1421";
const b = await chromium.launch({ executablePath: EXE });
const ctx = await b.newContext({ viewport: { width: 1024, height: 728 }, hasTouch: true });
const p = await ctx.newPage();
p.on("pageerror", e => console.log("[pageerror]", String(e).slice(0, 180)));
await p.goto(`http://127.0.0.1:${port}/?uimock=1`, { waitUntil: "domcontentloaded" });
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
// Receipt journey (F6), then switch inside the modal.
await p.keyboard.press("F6");
await p.waitForTimeout(1000);
for (const [label, shot] of [["Receipt", "receipt"], ["Delivery", "delivery"], ["Digital", "digital"]]) {
  const opt = p.locator(".pm-journey-opt", { hasText: new RegExp(`^${label}$`) }).first();
  if (await opt.count()) { await opt.click(); await p.waitForTimeout(800); }
  const due = await p.locator(".pm-due-value").first().textContent().catch(() => "?");
  const cta = await p.locator(".pm-complete-btn, .pm-confirm-btn, button:has-text('Take'), button:has-text('Confirm'), button:has-text('Send')").first().textContent().catch(() => "?");
  console.log(`${label.padEnd(9)} due=${(due || "").trim()}  cta="${(cta || "").trim().slice(0, 46)}"`);
  await p.screenshot({ path: `qa/layout/pay-${shot}.png` });
}
await b.close();
