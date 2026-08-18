import playwright from "../storefront/node_modules/@playwright/test/index.js";
const { chromium } = playwright;
const b = await chromium.launch();
async function go(p) {
  await p.goto("http://localhost:1420/?uimock=1", { waitUntil: "networkidle" });
  await p.waitForSelector(".oa-nav-item");
  await p.locator(".oa-nav-item").nth(7).click();
  await p.waitForTimeout(1000);
}
for (const [label, w, h] of [["1920",1920,1080],["1440",1440,900],["1024",1024,800],["768",768,1024]]) {
  const p = await b.newPage({ viewport: { width: w, height: h } });
  await go(p);
  await p.screenshot({ path: `qa-artifacts/rev2-actions-${label}.png` });
  if (label === "1440") {
    // Executed action → undo affordance
    await p.evaluate(() => (document.querySelectorAll(".zp-chip-clear")[1])?.click());
    await p.waitForTimeout(900);
    await p.locator(".zp-row-activator").first().click();
    await p.waitForTimeout(900);
    await p.screenshot({ path: "qa-artifacts/rev2-undoable.png" });
    const undoBtn = p.locator('.zp-review-detail button:has-text("Undo")');
    if (await undoBtn.count()) {
      await undoBtn.first().click();
      await p.waitForTimeout(700);
      await p.screenshot({ path: "qa-artifacts/rev2-undo-confirm.png" });
      await p.keyboard.press("Escape");
    }
  }
  // History section
  await p.evaluate(() => (document.querySelectorAll(".zp-review-tab")[2])?.click());
  await p.waitForTimeout(900);
  await p.screenshot({ path: `qa-artifacts/rev2-history-${label}.png` });
  if (label === "1440") {
    await p.locator(".zp-row-activator").first().click();
    await p.waitForTimeout(700);
    await p.screenshot({ path: "qa-artifacts/rev2-history-detail.png" });
    await p.evaluate(() => (document.querySelectorAll(".zp-review-tab")[1])?.click());
    await p.waitForTimeout(900);
    await p.screenshot({ path: "qa-artifacts/rev2-conflicts.png" });
  }
  await p.close();
}
console.log("done");
await b.close();
