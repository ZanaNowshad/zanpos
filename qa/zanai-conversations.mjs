// Threads: new, listed, reopened, renamed, deleted — on every chat surface.
//   node qa/zanai-conversations.mjs [port]
//
// What this replaces was a text link reading "· Clear chat", the same weight as
// "· Export", whose only behaviour was to delete every message the operator had
// ever exchanged with ZanAI, for the whole branch, with no confirmation.
import { chromium } from "playwright-core";

const EXE = "C:/Users/super/AppData/Local/ms-playwright/chromium-1234/chrome-win64/chrome.exe";
const port = process.argv[2] || "1421";

const checks = [];
const check = (name, pass, detail = "") => {
  checks.push({ pass });
  console.log(`${pass ? "PASS" : "FAIL"}  ${name}${detail ? `  — ${detail}` : ""}`);
};

async function surface(page, label) {
  // The destructive link must be gone from under the composer.
  const links = await page.locator(".chat-clear-link").allTextContents();
  check(`${label}: the delete-everything link is gone from the composer`,
    !links.join(" ").toLowerCase().includes("clear"), JSON.stringify(links));

  const newChat = page.locator(".oa-conv-new");
  const history = page.locator(".oa-conv-history");
  check(`${label}: New chat and History are real buttons`,
    await newChat.count() === 1 && await history.count() === 1);

  const box = await newChat.boundingBox();
  check(`${label}: New chat is a thumb-sized target`,
    !!box && box.height >= 34, `${Math.round(box?.height ?? 0)}px`);

  // History lists past threads.
  await history.click();
  await page.waitForTimeout(350);
  const rows = await page.locator(".oa-conv-row").count();
  check(`${label}: past chats are listed`, rows >= 2, `${rows} rows`);
  const titles = await page.locator(".oa-conv-title").allTextContents();
  check(`${label}: each is named after what was asked`,
    titles.some(t => t.includes("VAT")), JSON.stringify(titles));

  // Reopening loads that thread's messages, not a flat window across threads.
  await page.locator(".oa-conv-row").first().locator(".oa-conv-open").click();
  await page.waitForTimeout(600);
  const transcript = await page.locator(".chat-bubble").allTextContents();
  check(`${label}: reopening restores that thread`,
    transcript.some(t => t.includes("Output VAT")), JSON.stringify(transcript.slice(0, 2)));
  check(`${label}: and does not drag in the other thread`,
    !transcript.some(t => t.includes("93 out of stock")));

  // New chat clears the view without destroying the thread.
  await newChat.click();
  await page.waitForTimeout(500);
  const afterNew = await page.locator(".chat-bubble").count();
  check(`${label}: New chat opens an empty thread`, afterNew === 0, `${afterNew} bubbles`);
  await history.click();
  await page.waitForTimeout(350);
  const stillThere = await page.locator(".oa-conv-row").count();
  check(`${label}: the old thread survives New chat`, stillThere >= 2, `${stillThere} rows`);

  // Deleting asks first.
  await page.locator(".oa-conv-row").first().locator('[aria-label="Delete chat"]').click();
  await page.waitForTimeout(250);
  check(`${label}: delete asks before it acts`,
    await page.locator(".oa-conv-danger").count() === 1);
  await page.locator(".oa-conv-danger").click();
  await page.waitForTimeout(600);
  const afterDelete = await page.locator(".oa-conv-row").count();
  check(`${label}: confirming removes just that one`,
    afterDelete === stillThere - 1, `${stillThere} → ${afterDelete}`);
}

const main = async () => {
  const browser = await chromium.launch({ executablePath: EXE });

  const till = await browser.newContext({ viewport: { width: 1024, height: 768 }, hasTouch: true });
  const t = await till.newPage();
  await t.goto(`http://127.0.0.1:${port}/?uimock=1`, { waitUntil: "domcontentloaded" });
  await t.waitForTimeout(2400);
  await t.locator(".oa-back-btn").first().click();
  await t.waitForSelector(".login-screen", { timeout: 8000 });
  await t.waitForTimeout(500);
  await t.locator(".user-card").first().click();
  await t.waitForTimeout(400);
  await t.keyboard.type("1234", { delay: 40 });
  await t.keyboard.press("Enter");
  await t.waitForSelector(".pos-layout", { timeout: 10000 });
  await t.waitForTimeout(900);
  await t.locator(".zanai-pos-launcher").first().click();
  await t.waitForSelector(".chat-panel", { timeout: 8000 });
  await t.waitForTimeout(700);
  await t.locator(".oa-conv-history").click();
  await t.waitForTimeout(400);
  await t.screenshot({ path: "qa/layout/zanai-conversations-till.png" });
  await t.locator(".oa-conv-history").click();
  await surface(t, "till");
  await till.close();

  const office = await browser.newContext({ viewport: { width: 1512, height: 900 } });
  const o = await office.newPage();
  await o.goto(`http://127.0.0.1:${port}/?uimock=1`, { waitUntil: "domcontentloaded" });
  await o.waitForTimeout(2600);
  await o.locator('.oa-tool-btn:has-text("ZanAI")').first().click();
  await o.waitForSelector("textarea.chat-input-v2", { timeout: 8000 });
  await o.waitForTimeout(700);
  await surface(o, "dock");
  await o.locator(".oa-conv-history").click();
  await o.waitForTimeout(400);
  await o.screenshot({ path: "qa/layout/zanai-conversations-dock.png" });
  await office.close();

  await browser.close();
  const failed = checks.filter(c => !c.pass).length;
  console.log(`\n${checks.length - failed}/${checks.length} checks passed`);
  process.exit(failed === 0 ? 0 : 1);
};

main().catch(e => { console.error(e); process.exit(1); });
