import playwright from "../storefront/node_modules/@playwright/test/index.js";
const { chromium } = playwright;

/** Usage: node scripts/qa-rtl.mjs <domainRegex> <sectionRegex|-> <slug> */
const [domainRe, sectionRe, slug] = process.argv.slice(2);
const b = await chromium.launch();

for (const [w, h] of [[1440, 900], [768, 1024]]) {
  const p = await b.newPage({ viewport: { width: w, height: h } });
  const errs = [];
  p.on("console", m => { if (m.type() === "error") errs.push(m.text().slice(0, 90)); });
  p.on("pageerror", e => errs.push("PAGEERROR " + e.message.slice(0, 90)));

  await p.goto("http://localhost:1420/?uimock=1", { waitUntil: "domcontentloaded" });
  await p.waitForSelector(".oa-nav-item");

  // Switch to Arabic via the header language toggle.
  await p.evaluate(() => [...document.querySelectorAll("button")]
    .find(x => (x.textContent || "").trim() === "ع")?.click());
  await p.waitForTimeout(600);

  await p.evaluate(re => [...document.querySelectorAll(".oa-nav-item")]
    .find(x => new RegExp(re, "i").test(x.textContent || ""))?.click(), domainRe);
  await p.waitForTimeout(900);
  if (sectionRe && sectionRe !== "-") {
    await p.evaluate(re => [...document.querySelectorAll(".zp-section-nav-item")]
      .find(x => new RegExp(re, "i").test(x.textContent || ""))?.click(), sectionRe);
    await p.waitForTimeout(900);
  }

  console.log(w, JSON.stringify(await p.evaluate(() => {
    const main = document.querySelector(".oa-workspace, .dlv-tab, .rpt-layout, main") ?? document.body;
    const text = main.innerText;
    // Latin words that are NOT identifiers/currency/numbers indicate an
    // untranslated string leaking into the Arabic UI.
    const latin = (text.match(/\b[A-Za-z]{4,}\b/g) ?? [])
      .filter(w => !["BHD", "ZANPOS", "ZanAI", "PO", "AMW"].includes(w));
    return {
      dir: document.documentElement.getAttribute("dir"),
      overflow: document.documentElement.scrollWidth > document.documentElement.clientWidth + 1,
      leakedLatin: [...new Set(latin)].slice(0, 8),
      sample: text.replace(/\s+/g, " ").slice(0, 110),
    };
  })));
  await p.screenshot({ path: `qa-artifacts/rtl-${slug}-${w}.png` });
  console.log(w, "console:", errs.length ? errs.slice(0, 3) : "none");
  await p.close();
}
await b.close();
