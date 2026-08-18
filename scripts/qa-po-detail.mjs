import playwright from "../storefront/node_modules/@playwright/test/index.js";
const { chromium } = playwright;
const b = await chromium.launch();
for (const [label, w, h] of [["1440",1440,900],["1024",1024,800],["768",768,1024]]) {
  const p = await b.newPage({ viewport: { width: w, height: h } });
  await p.goto("http://localhost:1420/?uimock=1", { waitUntil: "networkidle" });
  await p.waitForSelector(".oa-nav-item");
  await p.locator(".oa-nav-item").nth(3).click();
  await p.waitForTimeout(900);
  await p.locator(".zp-row-activator").first().click();   // select PO
  await p.waitForTimeout(900);
  await p.screenshot({ path: `qa-artifacts/po-detail-${label}.png` });
  await p.close();
}
console.log("done");
await b.close();
