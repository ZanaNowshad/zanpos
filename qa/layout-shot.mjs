// One-off screenshot of a named surface, for eyeballing what the audit reports.
//   node qa/layout-shot.mjs <w> <h> <surface> [lang]
// surface: new-product | command-palette | till | login | <primary nav label>
import { chromium } from "playwright-core";
import { mkdirSync } from "node:fs";

const EXE = "C:/Users/super/AppData/Local/ms-playwright/chromium-1234/chrome-win64/chrome.exe";
const [, , w = "1280", h = "800", surface = "new-product", lang = "en"] = process.argv;

const main = async () => {
  mkdirSync("qa/layout", { recursive: true });
  const browser = await chromium.launch({ executablePath: EXE });
  const ctx = await browser.newContext({ viewport: { width: +w, height: +h } });
  await ctx.addInitScript((l) => { localStorage.setItem("zanpos_language", l); }, lang);
  const page = await ctx.newPage();
  const url = surface === "login" ? "http://127.0.0.1:1420/" : "http://127.0.0.1:1420/?uimock=1";
  await page.goto(url, { waitUntil: "domcontentloaded" });
  await page.waitForTimeout(1500);

  if (surface === "new-product") {
    await page.locator(".oa-primary-nav nav button").nth(2).click();
    await page.waitForTimeout(600);
    await page.locator(".oa-topbar-actions button").last().click();
    await page.waitForTimeout(800);
  } else if (surface === "command-palette") {
    await page.keyboard.press("Control+k");
    await page.waitForTimeout(600);
  } else if (surface === "till" || surface === "sign-in") {
    await page.locator(".oa-back-btn").first().click();
    await page.waitForSelector(".login-screen", { timeout: 6000 });
    await page.waitForTimeout(700);
    if (surface === "till") {
      await page.locator(".user-card").first().click();
      await page.waitForTimeout(500);
      await page.keyboard.type("1234", { delay: 60 });
      await page.keyboard.press("Enter");
      await page.waitForSelector(".pos-layout", { timeout: 8000 });
      await page.waitForTimeout(1200);
    }
  } else if (surface !== "login") {
    await page.locator(`.oa-primary-nav nav button:has-text("${surface}")`).first().click();
    await page.waitForTimeout(800);
  }

  const out = `qa/layout/shot-${w}x${h}-${lang}-${surface}.png`;
  await page.screenshot({ path: out });
  console.log(out);
  await browser.close();
};

main().catch((e) => { console.error(e); process.exit(1); });
