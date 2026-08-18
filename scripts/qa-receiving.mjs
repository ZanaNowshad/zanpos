import playwright from "../storefront/node_modules/@playwright/test/index.js";
const { chromium } = playwright;
const b = await chromium.launch();
for (const [label, w, h] of [["1440",1440,900],["1024",1024,800],["768",768,1024]]) {
  const p = await b.newPage({ viewport: { width: w, height: h } });
  await p.goto("http://localhost:1420/?uimock=1", { waitUntil: "networkidle" });
  await p.waitForSelector(".oa-nav-item");
  await p.locator(".oa-nav-item").nth(3).click();
  await p.waitForTimeout(900);
  await p.locator(".zp-row-activator").first().click();
  await p.waitForTimeout(800);
  await p.locator(".zp-po-detail-actions button").first().click();  // Receive
  await p.waitForTimeout(900);
  await p.screenshot({ path: `qa-artifacts/receiving-${label}.png` });
  if (label === "1440") {
    // Validation: over-receipt on the first editable line.
    const inputs = p.locator(".zp-receive-input:not([disabled])");
    await inputs.first().fill("999");
    await p.waitForTimeout(500);
    await p.screenshot({ path: "qa-artifacts/receiving-validation.png" });
  }
  await p.close();
}
console.log("done");
await b.close();
