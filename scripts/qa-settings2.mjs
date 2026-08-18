import playwright from "../storefront/node_modules/@playwright/test/index.js";
const { chromium } = playwright;
const b = await chromium.launch();
async function go(p) {
  await p.goto("http://localhost:1420/?uimock=1", { waitUntil: "networkidle" });
  await p.waitForSelector(".oa-nav-item");
  await p.locator(".oa-nav-item").nth(8).click();
  await p.waitForTimeout(700);
  await p.evaluate(() => [...document.querySelectorAll(".zp-section-nav-item")]
    .find(x => /setting/i.test(x.textContent||""))?.click());
  await p.waitForTimeout(1100);
}
const nav = (p,i) => p.evaluate(i => document.querySelectorAll(".zp-set-nav-item")[i]?.click(), i);
for (const [label,w,h] of [["1440",1440,900],["1024",1024,800],["768",768,1024]]) {
  const p = await b.newPage({ viewport: { width: w, height: h } });
  await go(p);
  const of = await p.evaluate(() => document.documentElement.scrollWidth > document.documentElement.clientWidth + 1);
  const navCols = await p.evaluate(() => document.querySelectorAll(".zp-section-nav, .zp-set-nav").length);
  console.log(`${label}: overflow=${of} navColumns=${navCols}`);
  await p.screenshot({ path: `qa-artifacts/set2-store-${label}.png` });
  if (label === "1440") {
    await p.fill(".zp-set-search input", "printer");
    await p.waitForTimeout(500);
    await p.screenshot({ path: "qa-artifacts/set2-search.png" });
    await p.fill(".zp-set-search input", "");
    await nav(p,5); await p.waitForTimeout(1200);   // ZanAI
    await p.screenshot({ path: "qa-artifacts/set2-ai.png" });
    await nav(p,8); await p.waitForTimeout(1200);   // Advanced
    await p.screenshot({ path: "qa-artifacts/set2-advanced.png" });
  }
  await p.close();
}
console.log("done");
await b.close();
