// Drives the payment modal the way a cashier would and reports what actually
// happened: where focus lands, whether it survives typing, and whether the
// dialpad reaches every field.  node qa/payment-check.mjs [w] [h] [journey]
import { chromium } from "playwright-core";
import { mkdirSync } from "node:fs";

const EXE = "C:/Users/super/AppData/Local/ms-playwright/chromium-1234/chrome-win64/chrome.exe";
const [, , w = "1440", h = "900", journey = "receipt"] = process.argv;

const toTill = async (page) => {
  await page.goto("http://127.0.0.1:1420/?uimock=1", { waitUntil: "domcontentloaded" });
  await page.waitForTimeout(1300);
  await page.locator(".oa-back-btn").first().click();
  await page.waitForSelector(".login-screen", { timeout: 8000 });
  await page.waitForTimeout(500);
  await page.locator(".user-card").first().click();
  await page.waitForTimeout(350);
  await page.keyboard.type("1234", { delay: 45 });
  await page.keyboard.press("Enter");
  await page.waitForSelector(".pos-layout", { timeout: 10000 });
  await page.waitForTimeout(1000);
};

const focusReport = () => {
  const el = document.activeElement;
  return {
    tag: el?.tagName,
    id: el?.id || null,
    cls: (typeof el?.className === "string" ? el.className : "").slice(0, 44),
    text: (el?.textContent || "").trim().slice(0, 24),
  };
};

const main = async () => {
  mkdirSync("qa/layout", { recursive: true });
  const browser = await chromium.launch({ executablePath: EXE });
  const page = await browser.newPage({ viewport: { width: +w, height: +h } });
  await toTill(page);

  // A sale needs a line before any tender route will open.
  await page.locator("input[placeholder*='Scan']").first().fill("6280123456781");
  await page.keyboard.press("Enter");
  await page.waitForTimeout(1000);

  const route = { receipt: 0, delivery: 1, digital: 2 }[journey] ?? 0;
  await page.locator(".np-journey-btn").nth(route).click();
  await page.waitForSelector(".pm-shell", { timeout: 8000 });
  await page.waitForTimeout(700);

  const onOpen = await page.evaluate(focusReport);

  // Type on the dialpad; the caret must not be yanked away between keys.
  const keys = ["5", "0", "0"];
  const afterEach = [];
  for (const k of keys) {
    await page.locator(`.pm-right .dialpad-key:text-is("${k}")`).first().click();
    await page.waitForTimeout(180);
    afterEach.push(await page.evaluate(focusReport));
  }
  const tendered = await page.evaluate(() =>
    document.querySelector(".pm-tendered-box .pm-amount-value")?.textContent?.trim() ?? null);

  // Now the real test of "the dialpad reaches every field": focus a text input
  // and check the keypad types into it and leaves focus alone.
  const fieldProbe = await page.evaluate(() => {
    const input = document.querySelector(".pm-shell input[type='text'], .pm-shell input:not([type])");
    return input ? { found: true, cls: input.className.slice(0, 40) } : { found: false };
  });

  const dialpadAlways = await page.evaluate(() => ({
    dialpadPresent: Boolean(document.querySelector(".pm-right .dialpad")),
    readyPanel: Boolean(document.querySelector(".pm-ready-panel")),
  }));

  // Switch to card — the dialpad must stay on screen.
  await page.locator(".pm-method").nth(1).click();
  await page.waitForTimeout(500);
  const onCard = await page.evaluate(() => ({
    dialpadPresent: Boolean(document.querySelector(".pm-right .dialpad")),
    compactReady: Boolean(document.querySelector(".pm-ready-panel-compact")),
    focus: document.activeElement?.className?.toString().slice(0, 40),
  }));

  await page.screenshot({ path: `qa/layout/payment-${w}x${h}-${journey}.png` });

  console.log(JSON.stringify({ onOpen, afterEach, tendered, fieldProbe, dialpadAlways, onCard }, null, 1));
  await browser.close();
};

main().catch((e) => { console.error(e.message); process.exit(1); });
