import playwright from "../storefront/node_modules/@playwright/test/index.js";
const { chromium } = playwright;
const b = await chromium.launch();
const nav = (p,i) => p.evaluate(i => document.querySelectorAll(".zp-set-nav-item")[i]?.click(), i);
async function go(p) {
  await p.goto("http://localhost:1420/?uimock=1", { waitUntil: "networkidle" });
  await p.waitForSelector(".oa-nav-item");
  await p.locator(".oa-nav-item").nth(8).click();          // System domain
  await p.waitForTimeout(800);
  await p.evaluate(() => {
    const s = [...document.querySelectorAll(".zp-section-nav-item")].find(x => /setting/i.test(x.textContent||""));
    s?.click();
  });
  await p.waitForTimeout(1100);
}
for (const [label,w,h] of [["1920",1920,1080],["1440",1440,900],["1024",1024,800],["768",768,1024]]) {
  const p = await b.newPage({ viewport: { width: w, height: h } });
  const errs = [];
  p.on("pageerror", e => errs.push(e.message.slice(0,120)));
  await go(p);
  await p.screenshot({ path: `qa-artifacts/set-store-${label}.png` });
  const groups = await p.locator(".zp-set-nav-item").count();
  const of = await p.evaluate(() => document.documentElement.scrollWidth > document.documentElement.clientWidth + 1);
  console.log(`${label}: groups=${groups} overflow=${of} errs=${errs.length}`);
  if (label === "1440") {
    for (const [i,name] of [[1,"sales"],[3,"hardware"],[5,"ai"],[6,"data"],[8,"advanced"]]) {
      await nav(p,i); await p.waitForTimeout(1100);
      await p.screenshot({ path: `qa-artifacts/set-${name}.png` });
    }
    await p.fill(".zp-set-search input", "printer");
    await p.waitForTimeout(500);
    await p.screenshot({ path: "qa-artifacts/set-search.png" });
  }
  await p.close();
}
console.log("done");
await b.close();
