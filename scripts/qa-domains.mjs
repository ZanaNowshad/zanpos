import playwright from "../storefront/node_modules/@playwright/test/index.js";
const { chromium } = playwright;

/**
 * Cross-domain responsive + coherence QA.
 *
 * Usage: node scripts/qa-domains.mjs <domainLabelRegex> <sectionRegex|-> <slug> [widths]
 * Reuses the Customers harness rather than adding a second browser setup.
 */
const [domainRe, sectionRe, slug, widthArg] = process.argv.slice(2);
const WIDTHS = (widthArg ?? "1920,1440,1280,1024,768").split(",").map(Number);
const HEIGHTS = { 1920: 1080, 1440: 900, 1280: 820, 1024: 800, 768: 1024 };

const b = await chromium.launch();

const audit = p => p.evaluate(() => ({
  overflow: document.documentElement.scrollWidth > document.documentElement.clientWidth + 1,
  navColumns: document.querySelectorAll(".zp-section-nav").length,
  // Blue was retired as a business-action colour.
  bluePrimaries: [...document.querySelectorAll(".oa-primary-mini, .btn-primary")]
    .filter(el => {
      const bg = getComputedStyle(el).backgroundColor.match(/\d+/g)?.map(Number) ?? [];
      return bg.length >= 3 && bg[2] > bg[0] + 30 && bg[2] > bg[1] + 30;
    }).map(el => el.textContent.trim().slice(0, 24)),
  headings: [...document.querySelectorAll(".oa-title")].map(h => h.textContent.trim()),
  // Any user-visible NaN/Infinity is a correctness defect, not a cosmetic one.
  badNumbers: (document.body.innerText.match(/NaN|Infinity|undefined/g) ?? []).length,
  errorBoundary: /something went wrong|error boundary/i.test(document.body.innerText),
}));

const errors = [];
for (const w of WIDTHS) {
  const p = await b.newPage({ viewport: { width: w, height: HEIGHTS[w] ?? 900 } });
  p.on("console", m => { if (m.type() === "error") errors.push(`${w}: ${m.text().slice(0, 120)}`); });
  p.on("pageerror", e => errors.push(`${w}: PAGEERROR ${e.message.slice(0, 120)}`));

  await p.goto("http://localhost:1420/?uimock=1", { waitUntil: "domcontentloaded" });
  await p.waitForSelector(".oa-nav-item");
  await p.evaluate(re => [...document.querySelectorAll(".oa-nav-item")]
    .find(x => new RegExp(re, "i").test(x.textContent || ""))?.click(), domainRe);
  await p.waitForTimeout(900);

  if (sectionRe && sectionRe !== "-") {
    await p.evaluate(re => [...document.querySelectorAll(".zp-section-nav-item")]
      .find(x => new RegExp(re, "i").test(x.textContent || ""))?.click(), sectionRe);
    await p.waitForTimeout(900);
  }

  console.log(w, JSON.stringify(await audit(p)));
  await p.screenshot({ path: `qa-artifacts/${slug}-${w}.png` });
  await p.close();
}

console.log("console errors:", errors.length ? errors.slice(0, 6) : "none");
await b.close();
