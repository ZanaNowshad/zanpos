// The procedures and the quick actions must be reachable on every chat surface,
// before and after a conversation starts.
//   node qa/zanai-quickactions.mjs [port]
//
// The launcher used to render only on the fullscreen Assistant and only before
// the first message, so the till and the docked copilot — where somebody is
// actually standing when they need "end-of-day reconciliation" — never showed
// it at all.
import { chromium } from "playwright-core";

const EXE = "C:/Users/super/AppData/Local/ms-playwright/chromium-1234/chrome-win64/chrome.exe";
const port = process.argv[2] || "1421";

const checks = [];
const check = (name, pass, detail = "") => {
  checks.push({ pass });
  console.log(`${pass ? "PASS" : "FAIL"}  ${name}${detail ? `  — ${detail}` : ""}`);
};

const overflowRight = (page, sel) =>
  page.evaluate(s => {
    const el = document.querySelector(s);
    const box = document.querySelector(".chat-messages");
    if (!el || !box) return null;
    return Math.round(el.getBoundingClientRect().right - box.getBoundingClientRect().right);
  }, sel);

async function surface(page, label) {
  const cards = await page.locator(".oa-wf-card").count();
  check(`${label}: the procedures are there before anything is asked`, cards >= 12, `${cards} cards`);

  const spill = await overflowRight(page, ".oa-wf-card");
  check(`${label}: no procedure card spills out of the pane`,
    spill !== null && spill <= 0, `${spill}px`);

  // Nested scrolling would swallow the touch gesture on a till.
  const nested = await page.evaluate(() =>
    getComputedStyle(document.querySelector(".oa-wf-launcher")).overflowY);
  check(`${label}: the launcher does not scroll inside the scrolling chat`, nested === "visible", nested);

  await page.locator(".oa-wf-card").first().click();
  await page.waitForTimeout(1500);

  const launcherGone = await page.locator(".oa-wf-card").count();
  check(`${label}: picking a procedure sends it and clears the launcher`, launcherGone === 0);

  const chips = await page.locator(".chat-quick-chip").count();
  check(`${label}: the quick actions stay after the conversation starts`, chips >= 6, `${chips} chips`);

  const oneRow = await page.evaluate(() => {
    const bar = document.querySelector(".chat-quick-bar");
    if (!bar) return null;
    const tops = new Set([...bar.children].map(c => Math.round(c.getBoundingClientRect().top)));
    return { rows: tops.size, height: Math.round(bar.getBoundingClientRect().height) };
  });
  check(`${label}: they cost one row, not three`,
    oneRow !== null && oneRow.rows === 1, JSON.stringify(oneRow));

  // And a way back to the full list.
  const reopen = page.locator(".chat-quick-chip", { hasText: "All procedures" });
  check(`${label}: there is a way back to the procedures`, await reopen.count() === 1);
  await reopen.click();
  await page.waitForTimeout(400);
  check(`${label}: reopening shows them again`, (await page.locator(".oa-wf-card").count()) >= 12);
}

const main = async () => {
  const browser = await chromium.launch({ executablePath: EXE });

  // ── The till widget ────────────────────────────────────────────────────
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
  await t.waitForTimeout(600);
  await surface(t, "till");
  await t.screenshot({ path: "qa/layout/zanai-quickactions-till.png" });
  await till.close();

  // ── The docked copilot ─────────────────────────────────────────────────
  const office = await browser.newContext({ viewport: { width: 1512, height: 900 } });
  const o = await office.newPage();
  await o.goto(`http://127.0.0.1:${port}/?uimock=1`, { waitUntil: "domcontentloaded" });
  await o.waitForTimeout(2600);
  await o.locator('.oa-tool-btn:has-text("ZanAI")').first().click();
  await o.waitForSelector("textarea.chat-input-v2", { timeout: 8000 });
  await o.waitForTimeout(700);
  await surface(o, "dock");
  await o.screenshot({ path: "qa/layout/zanai-quickactions-dock.png" });

  // ── The fullscreen assistant ───────────────────────────────────────────
  // Expanding the dock to read the cards properly is the obvious next move, so
  // the launcher has to survive the move rather than close on remount.
  await o.locator(".oa-dock-btn").first().click();
  await o.waitForTimeout(900);
  const carried = await o.locator(".oa-wf-card").count();
  check("assistant: the reopened launcher survives expanding the dock", carried >= 12, `${carried} cards`);

  const across = await o.evaluate(() => {
    const g = document.querySelector(".oa-wf-grid");
    return g ? getComputedStyle(g).gridTemplateColumns.split(" ").length : null;
  });
  check("assistant: they lay out several across when there is room", across >= 2, `${across} columns`);
  await o.screenshot({ path: "qa/layout/zanai-quickactions-assistant.png" });

  await office.close();

  await browser.close();
  const failed = checks.filter(c => !c.pass).length;
  console.log(`\n${checks.length - failed}/${checks.length} checks passed`);
  process.exit(failed === 0 ? 0 : 1);
};

main().catch(e => { console.error(e); process.exit(1); });
