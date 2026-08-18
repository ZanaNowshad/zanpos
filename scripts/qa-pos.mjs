import playwright from "../storefront/node_modules/@playwright/test/index.js";
const { chromium } = playwright;
const [slug, wRaw, hRaw] = process.argv.slice(2);
const W = Number(wRaw ?? 1536), H = Number(hRaw ?? 1024);
const b = await chromium.launch();
const p = await b.newPage({ viewport: { width: W, height: H } });
const errs = [];
p.on("console", m => { if (m.type() === "error") errs.push(m.text().slice(0, 90)); });
p.on("pageerror", e => errs.push("PAGEERROR " + e.message.slice(0, 90)));

await p.goto("http://localhost:1420/?uimock=1", { waitUntil: "domcontentloaded" });
await p.evaluate(() => { localStorage.removeItem("zanpos_theme"); localStorage.removeItem("zanpos_language"); });
await p.reload({ waitUntil: "domcontentloaded" });
await p.waitForTimeout(1400);

// Back office -> POS mode.
await p.evaluate(() => [...document.querySelectorAll("button")]
  .find(x => /back to pos/i.test(x.textContent || ""))?.click());
await p.waitForTimeout(1400);

// Register handoff: pick the cashier, then enter the PIN the mock accepts.
await p.evaluate(() => [...document.querySelectorAll("button")]
  .find(x => /renihal/i.test(x.textContent || ""))?.click());
await p.waitForTimeout(900);
console.log("PIN SCREEN CONTROLS:", JSON.stringify(await p.evaluate(() => ({
  buttons: [...document.querySelectorAll("button")].map(b => b.textContent.trim().slice(0,12)).filter(Boolean).slice(0,24),
  inputs: [...document.querySelectorAll("input")].map(i => ({ type: i.type, ph: i.placeholder, id: i.id })),
}))));
for (const d of ["1", "2", "3", "4"]) {
  await p.evaluate(n => {
    const btn = [...document.querySelectorAll("button")].find(x => x.textContent.trim() === n);
    if (btn) btn.click();
  }, d);
  await p.waitForTimeout(140);
}
// Confirm the PIN.
await p.evaluate(() => [...document.querySelectorAll("button")]
  .find(x => /OK/.test(x.textContent || ""))?.click());
await p.waitForTimeout(2000);

console.log(slug, JSON.stringify(await p.evaluate(() => ({
  overflow: document.documentElement.scrollWidth > document.documentElement.clientWidth + 1,
  stage: /who.s on register|enter your pin/i.test(document.body.innerText) ? "HANDOFF"
       : /ready for the next sale|scan barcode/i.test(document.body.innerText) ? "REGISTER" : "OTHER",
  text: document.body.innerText.replace(/\s+/g, " ").slice(0, 110),
}))));
await p.screenshot({ path: `qa/parity/${slug}.png` });
console.log("console:", errs.length ? errs.slice(0, 3) : "none");
await b.close();
