// Scanning into the ZanAI composer must behave like typing into it.
//   node qa/zanai-scan.mjs [port]
//
// Drives the real till: sign in, open the chat, write half a sentence, then
// fire a barcode at scanner speed. Asserts the digits land in the composer and
// that the scanner's terminating Enter did not send the half-written message.
import { chromium } from "playwright-core";

const EXE = "C:/Users/super/AppData/Local/ms-playwright/chromium-1234/chrome-win64/chrome.exe";
const port = process.argv[2] || "1421";
const BARCODE = "628001";

const main = async () => {
  const browser = await chromium.launch({ executablePath: EXE });
  const ctx = await browser.newContext({ viewport: { width: 1024, height: 728 }, hasTouch: true });
  const page = await ctx.newPage();
  await page.goto(`http://127.0.0.1:${port}/?uimock=1`, { waitUntil: "domcontentloaded" });
  await page.waitForTimeout(2400);

  // Back office → till.
  await page.locator(".oa-back-btn").first().click();
  await page.waitForSelector(".login-screen", { timeout: 8000 });
  await page.waitForTimeout(600);
  await page.locator(".user-card").first().click();
  await page.waitForTimeout(500);
  await page.keyboard.type("1234", { delay: 60 });
  await page.keyboard.press("Enter");
  await page.waitForSelector(".pos-layout", { timeout: 10000 });
  await page.waitForTimeout(1200);

  // Open the chat widget.
  const launcher = page.locator(".zanai-pos-launcher").first();
  await launcher.click();
  await page.waitForTimeout(900);

  const composer = page.locator("textarea.chat-input-v2").first();
  await composer.click();
  await composer.type("do we have more of ", { delay: 55 });

  // The cashier stops typing and reaches for the scanner. The guard resets its
  // burst buffer after 120ms of quiet, so without this pause the already-typed
  // words (spaces and all) are still in the buffer and the scan cannot be
  // recognised as one.
  await page.waitForTimeout(400);

  // Scanner speed: characters back to back, then the trigger's own Enter.
  await page.keyboard.type(BARCODE, { delay: 0 });
  await page.keyboard.press("Enter");
  await page.waitForTimeout(900);

  const draft = await composer.inputValue();
  await page.screenshot({ path: "qa/layout/zanai-scan.png" });

  // A sent message clears the composer, so a draft still holding both the
  // sentence and the barcode proves the scan typed in and nothing was sent.
  const kept = draft.includes(BARCODE);
  const notSent = draft.includes("do we have more of");
  console.log(`composer draft : ${JSON.stringify(draft)}`);
  console.log(`barcode kept   : ${kept}`);
  console.log(`message unsent : ${notSent}`);

  await browser.close();
  const ok = kept && notSent;
  console.log(ok ? "\nPASS — the scan typed into the chat and nothing was sent" : "\nFAIL");
  process.exit(ok ? 0 : 1);
};

main().catch((e) => { console.error(e); process.exit(1); });
