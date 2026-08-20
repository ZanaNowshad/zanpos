// Guards the product table's column algorithm at every width the app ships at.
//
// Twice now the lead column has lost its width to a column with bounded
// content: first collapsing to the width of its 42px thumbnail, so every row
// showed a barcode and no product name, then truncating names while the
// barcode column held ~600px for 13 characters. Both were invisible against a
// short fixture and only appeared on the real 28,000-product catalogue, which
// is why this measures rendered geometry rather than asserting on CSS.
import { chromium } from "playwright-core";
const EXE = "C:/Users/super/AppData/Local/ms-playwright/chromium-1234/chrome-win64/chrome.exe";
const port = process.argv[2] || "1421";
const WIDTHS = [1024, 1280, 1366, 1920];

const b = await chromium.launch({ executablePath: EXE });
let failures = 0;

for (const width of WIDTHS) {
  const p = await (await b.newContext({ viewport: { width, height: 900 } })).newPage();
  p.on("pageerror", e => console.log("  [pageerror]", String(e).slice(0, 140)));
  await p.goto(`http://127.0.0.1:${port}/?uimock=1`, { waitUntil: "domcontentloaded" });
  await p.waitForTimeout(2400);
  await p.locator(".oa-nav-item", { hasText: /^Catalogue$/ }).first().click();
  await p.waitForTimeout(600);
  await p.locator(".zp-section-nav button", { hasText: /^Products$/ }).first().click();
  await p.waitForTimeout(1500);

  const m = await p.evaluate(() => {
    const ths = [...document.querySelectorAll("table thead th")];
    const cols = Object.fromEntries(ths.map(th => [th.textContent.trim() || "actions", Math.round(th.getBoundingClientRect().width)]));
    const tbl = document.querySelector("table");
    const wrap = document.querySelector(".zp-table-wrap");
    const names = [...document.querySelectorAll(".product-catalogue-identity .zp-cell-primary")];
    return {
      cols,
      hOverflow: wrap ? tbl.scrollWidth > wrap.clientWidth + 1 : false,
      // A name cell showing nothing at all is the original bug.
      emptyNameCells: names.filter(e => e.getBoundingClientRect().width < 40).length,
      tallestRow: Math.max(...[...document.querySelectorAll("table tbody tr")].slice(0, 20)
        .map(tr => Math.round(tr.getBoundingClientRect().height))),
    };
  });

  const problems = [];
  const name = m.cols["Product name"] ?? 0;
  // The identifying column must stay the widest thing on the row and stay
  // readable — 150px is the narrowest floor the stylesheet sets.
  if (name < 150) problems.push(`name column ${name}px is below the 150px floor`);
  if (m.cols["Barcode"] > 200) problems.push(`barcode column ${m.cols["Barcode"]}px — bounded content should not absorb slack`);
  if (m.cols["Category"] > 200) problems.push(`category column ${m.cols["Category"]}px`);
  if (name < (m.cols["Barcode"] ?? 0)) problems.push("barcode column is wider than the product name");
  if (m.hOverflow) problems.push("table overflows its wrapper — trailing columns unreachable");
  if (m.emptyNameCells) problems.push(`${m.emptyNameCells} name cells rendered with no room for text`);
  if (m.tallestRow > 80) problems.push(`tallest row ${m.tallestRow}px — a cell is wrapping past two lines`);

  const cols = Object.entries(m.cols).map(([k, v]) => `${k}:${v}`).join("  ");
  console.log(`${String(width).padEnd(5)} ${cols}`);
  for (const problem of problems) { console.log(`      ✗ ${problem}`); failures += 1; }
  await p.context().close();
}

await b.close();
console.log(failures ? `\nFAIL — ${failures} issue(s)` : "\nPASS — product table columns hold at every width");
process.exit(failures ? 1 : 0);
