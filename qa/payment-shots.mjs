// Screenshots of the payment modal at the till's real spec, one per state.
//   node qa/payment-shots.mjs [port]
import { chromium } from "playwright-core";
const EXE = "C:/Users/super/AppData/Local/ms-playwright/chromium-1234/chrome-win64/chrome.exe";
const port = process.argv[2] || "1421";
const b = await chromium.launch({ executablePath: EXE });
const ctx = await b.newContext({ viewport: { width: 1024, height: 728 }, hasTouch: true });
const p = await ctx.newPage();
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
await p.keyboard.press("F6");
await p.waitForSelector(".pm-shell", { timeout: 8000 });
await p.waitForTimeout(900);
await p.screenshot({ path: "qa/layout/pm-1-receipt.png" });

const journey = async label => {
  await p.locator(".pm-journey-opt", { hasText: new RegExp(`^${label}$`) }).first().click();
  await p.waitForTimeout(800);
};

await journey("Delivery");
await p.screenshot({ path: "qa/layout/pm-2-delivery.png" });

// The dropdown, merging saved customers with WhatsApp contacts and chats.
await p.locator("#payment-customer-phone").click();
await p.keyboard.type("36", { delay: 60 });
await p.waitForTimeout(1100);
await p.screenshot({ path: "qa/layout/pm-3-suggestions.png" });

// The address filled, with the letters up.
await p.locator("#payment-customer-phone").fill("");
await p.locator("#payment-customer-phone").click();
await p.keyboard.type("36001122", { delay: 25 });
await p.waitForTimeout(700);
await p.locator("#payment-road").click();
await p.keyboard.type("Road 2814", { delay: 25 });
await p.waitForTimeout(500);
const mode = p.locator(".pm-mode-btn");
if ((await mode.textContent()) === "ABC") await mode.click();
await p.waitForTimeout(500);
await p.screenshot({ path: "qa/layout/pm-4-keyboard.png" });

// The directory, with its own keys open.
await p.evaluate(() => document.activeElement?.blur());
await p.waitForTimeout(300);
await p.locator(".pm-directory-btn").click();
await p.waitForSelector(".pm-directory-dialog", { timeout: 4000 });
await p.waitForTimeout(700);
await p.locator(".pm-directory-keys-btn").click();
await p.waitForTimeout(500);
await p.screenshot({ path: "qa/layout/pm-5-directory.png" });
await p.keyboard.press("Escape");
await p.waitForTimeout(400);

await journey("Digital");
await p.screenshot({ path: "qa/layout/pm-6-digital.png" });
console.log("shots written to qa/layout/pm-*.png");
await b.close();
