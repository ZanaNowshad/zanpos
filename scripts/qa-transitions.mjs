import playwright from "../storefront/node_modules/@playwright/test/index.js";
const { chromium } = playwright;
const b = await chromium.launch();
const NAMES = ["Actions", "Conflicts", "History"];
const tab = (p,i) => p.evaluate(i => document.querySelectorAll(".zp-review-tab")[i]?.click(), i);
const row = p => p.evaluate(() => document.querySelectorAll(".zp-row-activator")[0]?.click());
let fails = 0, runs = 0;

async function open(w=1440,h=900) {
  const p = await b.newPage({ viewport: { width: w, height: h } });
  p.__console = [];
  p.on("console", m => { if (m.type()==="error") p.__console.push(m.text().slice(0,160)); });
  await p.goto("http://localhost:1420/?uimock=1", { waitUntil: "networkidle" });
  await p.waitForSelector(".oa-nav-item");
  await p.locator(".oa-nav-item").nth(7).click();
  await p.waitForTimeout(800);
  return p;
}
async function check(p, label) {
  runs++;
  const crashed = await p.locator(".error-boundary-msg").count() > 0;
  const mounted = await p.locator(".zp-review-tab").count() === 3;
  if (crashed || !mounted) { fails++; console.log(`FAIL ${label} crashed=${crashed} mounted=${mounted}`); }
}

// All six transitions × {no selection, with selection}
for (const from of [0,1,2]) for (const to of [0,1,2]) {
  if (from === to) continue;
  for (const withSel of [false, true]) {
    const p = await open();
    await tab(p, from); await p.waitForTimeout(700);
    if (withSel) { await row(p); await p.waitForTimeout(600); }
    await tab(p, to); await p.waitForTimeout(900);
    await check(p, `${NAMES[from]}→${NAMES[to]}${withSel?" (selected)":""}`);
    await p.close();
  }
}
// Responsive: every section at each width
for (const [w,h] of [[1440,900],[1024,800],[768,1024]]) {
  for (const i of [0,1,2]) {
    const p = await open(w,h);
    await tab(p, i); await p.waitForTimeout(900);
    await check(p, `${NAMES[i]} @${w}`);
    const of = await p.evaluate(() => document.documentElement.scrollWidth > document.documentElement.clientWidth + 1);
    if (of) { fails++; console.log(`FAIL overflow ${NAMES[i]} @${w}`); }
    await p.close();
  }
}
console.log(`\n${runs - fails}/${runs} passed`);
await b.close();
process.exit(fails ? 1 : 0);
