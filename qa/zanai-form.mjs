// ZanAI's interactive forms, driven end to end.
//   node qa/zanai-form.mjs [port]
//
// Two surfaces, because they impose opposite constraints. The till widget is a
// ~360px column on a 1024x768 panel, where a four-column purchase bill has to
// restack or the submit button leaves the screen. The Assistant tab is wide,
// where the same bill should read as a grid. Both are checked here, along with
// the round trip that is the whole point: values typed into boxes leave as a
// message the model can act on.
import { chromium } from "playwright-core";

const EXE = "C:/Users/super/AppData/Local/ms-playwright/chromium-1234/chrome-win64/chrome.exe";
const port = process.argv[2] || "1420";

const checks = [];
const check = (name, pass, detail = "") => {
  checks.push({ name, pass, detail });
  console.log(`${pass ? "PASS" : "FAIL"}  ${name}${detail ? `  — ${detail}` : ""}`);
};

/** Does the element sit entirely inside its scroll container's visible box? */
const overflow = (page, inner, outer) =>
  page.evaluate(([a, b]) => {
    const el = document.querySelector(a);
    const box = document.querySelector(b);
    if (!el || !box) return null;
    const r = el.getBoundingClientRect();
    const o = box.getBoundingClientRect();
    return { right: Math.round(r.right - o.right), bottom: Math.round(r.bottom - o.bottom) };
  }, [inner, outer]);

async function ask(page, text) {
  const composer = page.locator("textarea.chat-input-v2").first();
  await composer.click();
  await composer.fill(text);
  await page.keyboard.press("Enter");
  await page.waitForSelector(".aiform", { timeout: 8000 });
  await page.waitForTimeout(400);
}

async function tillWidget(browser) {
  const ctx = await browser.newContext({ viewport: { width: 1024, height: 768 }, hasTouch: true });
  const page = await ctx.newPage();
  await page.goto(`http://127.0.0.1:${port}/?uimock=1`, { waitUntil: "domcontentloaded" });
  await page.waitForTimeout(2400);

  await page.locator(".oa-back-btn").first().click();
  await page.waitForSelector(".login-screen", { timeout: 8000 });
  await page.waitForTimeout(500);
  await page.locator(".user-card").first().click();
  await page.waitForTimeout(400);
  await page.keyboard.type("1234", { delay: 50 });
  await page.keyboard.press("Enter");
  await page.waitForSelector(".pos-layout", { timeout: 10000 });
  await page.waitForTimeout(1000);

  await page.locator(".zanai-pos-launcher").first().click();
  await page.waitForTimeout(700);

  // ── A two-field ask ────────────────────────────────────────────────────
  await ask(page, "price update");
  const labels = await page.locator(".aiform-label").allTextContents();
  check("till: the ask arrives as labelled boxes", labels.length === 2,
    labels.join(" / "));

  const modes = await page.locator(".aiform-input").evaluateAll(
    els => els.map(el => el.getAttribute("inputmode")));
  check("till: money and barcode open the numeric keypad",
    modes.includes("numeric") && modes.includes("decimal"), modes.join(","));

  // Both axes. The first cut only measured the right edge and passed while the
  // buttons sat clipped below the fold of a 250px-tall message list.
  const submitInside = await overflow(page, ".aiform-submit", ".chat-messages");
  check("till: the submit button is on screen, not clipped below the fold",
    submitInside !== null && submitInside.right <= 0 && submitInside.bottom <= 0,
    submitInside ? `${submitInside.right}px right, ${submitInside.bottom}px below` : "not found");

  // Focus lands in the first box, so the operator's first keystroke does not
  // end up in the chat composer behind the form.
  const focused = await page.evaluate(() => document.activeElement?.className ?? "");
  check("till: the first box already has focus", focused.includes("aiform-input"), focused);

  await page.screenshot({ path: "qa/layout/zanai-form-till-fields.png" });

  // ── The round trip ─────────────────────────────────────────────────────
  const boxes = page.locator(".aiform-input");
  await boxes.nth(0).fill("6291001234567");
  await boxes.nth(1).fill("3.500");
  await page.locator(".aiform-submit").click();
  await page.waitForTimeout(1400);

  const sent = await page.locator(".bubble-text-plain").last().innerText();
  check("till: answers leave keyed by the names the model asked for",
    sent.includes("barcode: 6291001234567") && sent.includes("new_price: 3.500"),
    JSON.stringify(sent));

  const formGone = await page.locator(".aiform").count();
  check("till: the answered form is taken off the bubble", formGone === 0, `${formGone} left`);

  const reply = await page.locator(".chat-bubble.assistant").last().innerText();
  check("till: the reply is one line, not a re-ask", reply.includes("3.500") && reply.length < 90,
    JSON.stringify(reply));

  // ── One-tap answers ────────────────────────────────────────────────────
  // The only element type never exercised in a browser: choices answer on
  // their own, so they must be reachable, tappable, and must send the value
  // rather than the label the operator read.
  await ask(page, "apply to which branch");
  const choices = await page.locator(".aiform-choice").count();
  const submitAlongside = await page.locator(".aiform-submit").count();
  check("till: a decision arrives as tappable buttons", choices === 3, `${choices} buttons`);
  check("till: a choice-only form offers nothing to submit", submitAlongside === 0);

  const styled = await page.evaluate(() => ({
    primary: !!document.querySelector(".aiform-choice-primary"),
    danger: !!document.querySelector(".aiform-choice-danger"),
  }));
  // A thumb aiming for "cancel" must not land on the destructive option.
  check("till: the recommended and destructive options look different",
    styled.primary && styled.danger, JSON.stringify(styled));

  const tapTarget = await page.locator(".aiform-choice").first().boundingBox();
  check("till: a choice is big enough for a thumb",
    !!tapTarget && tapTarget.height >= 44, `${Math.round(tapTarget?.height ?? 0)}px tall`);

  await page.locator(".aiform-choice").first().click();
  await page.waitForTimeout(1200);
  const choiceSent = await page.locator(".bubble-text-plain").last().innerText();
  check("till: the tapped choice sends its value, not its label",
    choiceSent.includes("choice: this_branch"), JSON.stringify(choiceSent));

  // ── A purchase bill on the narrow surface ──────────────────────────────
  await ask(page, "purchase bill entry");
  const stacked = await page.evaluate(() => {
    const cells = document.querySelector(".aiform-grid-cells");
    if (!cells) return null;
    const tracks = getComputedStyle(cells).gridTemplateColumns.split(" ").map(parseFloat);
    return { tracks, narrowest: Math.min(...tracks) };
  });
  check("till: a four-column bill wraps rather than shrinking",
    stacked !== null && stacked.tracks.length <= 2 && stacked.narrowest >= 110,
    JSON.stringify(stacked));

  const rowInside = await overflow(page, ".aiform-grid-row", ".chat-messages");
  check("till: no bill row runs past the widget edge",
    rowInside !== null && rowInside.right <= 0,
    rowInside ? `${rowInside.right}px` : "not found");

  const firstLabel = await page.locator(".aiform-grid-cell .aiform-label").first();
  check("till: every cell still says what it is", await firstLabel.isVisible(),
    await firstLabel.innerText());

  // Landing at the very end of the thread would show a bare input box with the
  // question that produced it scrolled off the top.
  const heading = await page.locator(".aiform-head h4").last().isVisible();
  check("till: the form's own heading is on screen, not scrolled past", heading);

  await page.screenshot({ path: "qa/layout/zanai-form-till-bill.png" });

  // An empty required cell has to stop the submit, not sail through as blank.
  await page.locator(".aiform-submit").click();
  await page.waitForTimeout(300);
  const blocked = await page.locator(".aiform-error").count();
  const invalidCells = await page.locator(".aiform-input-invalid").count();
  check("till: the two missing barcodes block submission and are marked",
    blocked === 1 && invalidCells === 2, `${blocked} message, ${invalidCells} marked`);
  await page.screenshot({ path: "qa/layout/zanai-form-till-required.png" });

  await ctx.close();
}

async function officeAssistant(browser) {
  const ctx = await browser.newContext({ viewport: { width: 1512, height: 900 } });
  const page = await ctx.newPage();
  await page.goto(`http://127.0.0.1:${port}/?uimock=1`, { waitUntil: "domcontentloaded" });
  await page.waitForTimeout(2600);

  await page.locator('.oa-tool-btn:has-text("ZanAI")').first().click();
  await page.waitForSelector("textarea.chat-input-v2", { timeout: 8000 });
  await page.waitForTimeout(700);

  await ask(page, "purchase bill entry from this delivery note");
  const grid = await page.evaluate(() => {
    const cells = document.querySelector(".aiform-grid-cells");
    if (!cells) return null;
    const tracks = getComputedStyle(cells).gridTemplateColumns.split(" ").map(parseFloat);
    return {
      tracks,
      narrowest: Math.min(...tracks),
      rows: document.querySelectorAll(".aiform-grid-row").length,
      fields: document.querySelectorAll(".aiform-fields .aiform-field").length,
    };
  });
  /* The docked copilot is the narrow case the first grid failed: a four-column
     bill came out as 30px columns headed "BA RC OD E". Nothing may shrink below
     a width a value fits in, whatever the surface. */
  check("office: no dock column is narrower than a value fits in",
    grid !== null && grid.narrowest >= 110, JSON.stringify(grid));
  check("office: every extracted line is editable", grid !== null && grid.rows === 4);
  check("office: the header fields came prefilled", grid !== null && grid.fields === 3);

  const dockOverflow = await overflow(page, ".aiform-grid-row", ".chat-messages");
  check("office: no bill row runs past the dock edge",
    dockOverflow !== null && dockOverflow.right <= 0,
    dockOverflow ? `${dockOverflow.right}px` : "not found");

  const prefilled = await page.locator(".aiform-grid-row input").first().inputValue();
  check("office: what was read off the bill is already in the box",
    prefilled === "6291001234567", prefilled);

  await page.screenshot({ path: "qa/layout/zanai-form-office-bill.png", fullPage: false });

  // Adding a line the document missed, then removing one that should not be
  // entered — the two edits a manager actually makes against a delivery.
  const before = await page.locator(".aiform-grid-row").count();
  await page.locator(".aiform-row-add").click();
  await page.waitForTimeout(200);
  const added = await page.locator(".aiform-grid-row").count();
  await page.locator(".aiform-row-remove").first().click();
  await page.waitForTimeout(200);
  const after = await page.locator(".aiform-grid-row").count();
  check("office: lines can be added and dropped",
    added === before + 1 && after === added - 1, `${before} → ${added} → ${after}`);

  // ── Fullscreen assistant: the same form, four times the width ──────────
  await page.locator(".oa-dock-btn").first().click();
  await page.waitForTimeout(900);
  const wide = await page.evaluate(() => {
    const cells = document.querySelector(".aiform-grid-cells");
    if (!cells) return null;
    const tracks = getComputedStyle(cells).gridTemplateColumns.split(" ").map(parseFloat);
    return { tracks, width: Math.round(cells.getBoundingClientRect().width) };
  });
  // The wrap is what makes one component serve both surfaces: given room, the
  // four columns sit side by side instead of stacking into a tall column.
  check("assistant: the same bill lays out four across when there is room",
    wide !== null && wide.tracks.length === 4, JSON.stringify(wide));
  await page.screenshot({ path: "qa/layout/zanai-form-assistant-bill.png" });

  await ctx.close();
}

const main = async () => {
  const browser = await chromium.launch({ executablePath: EXE });
  try {
    await tillWidget(browser);
    await officeAssistant(browser);
  } finally {
    await browser.close();
  }
  const failed = checks.filter(c => !c.pass);
  console.log(`\n${checks.length - failed.length}/${checks.length} checks passed`);
  process.exit(failed.length === 0 ? 0 : 1);
};

main().catch(e => { console.error(e); process.exit(1); });
