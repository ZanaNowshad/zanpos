import playwright from "../storefront/node_modules/@playwright/test/index.js";
const { chromium } = playwright;
const b = await chromium.launch();
const p = await b.newPage({ viewport: { width: 1440, height: 900 } });
const errs = [];
p.on("console", m => { if (m.type()==="error" && !/unregisterListener/.test(m.text())) errs.push(m.text().slice(0,120)); });
await p.goto("http://localhost:1420/?uimock=1", { waitUntil: "networkidle" });
await p.waitForSelector(".oa-nav-item");
const names = ["Today","Sell","Catalogue","Purchasing","Customers","Team","Insights","Review","System"];
let bad = 0;
for (let i = 0; i < 9; i++) {
  await p.locator(".oa-nav-item").nth(i).click();
  await p.waitForTimeout(800);
  const crashed = await p.locator(".error-boundary-msg").count() > 0;
  const of = await p.evaluate(() => document.documentElement.scrollWidth > document.documentElement.clientWidth + 1);
  if (crashed || of) { bad++; console.log(`FAIL ${names[i]} crashed=${crashed} overflow=${of}`); }
}
console.log(bad ? `${bad} domain(s) failed` : "all 9 domains OK");
if (errs.length) console.log("console errors:", errs.slice(0,3));
await b.close();
