import playwright from "../storefront/node_modules/@playwright/test/index.js";
const { chromium } = playwright;
const b = await chromium.launch();

/** Open the Customers domain (rail index 5) and its Directory section. */
async function openDirectory(p) {
  await p.goto("http://localhost:1420/?uimock=1", { waitUntil: "domcontentloaded" });
  await p.waitForSelector(".oa-nav-item");
  await p.evaluate(() => [...document.querySelectorAll(".oa-nav-item")]
    .find(x => /customer/i.test(x.textContent || ""))?.click());
  await p.waitForTimeout(900);
}

async function openSection(p, re) {
  await p.evaluate(pattern => [...document.querySelectorAll(".zp-section-nav-item")]
    .find(x => new RegExp(pattern, "i").test(x.textContent || ""))?.click(), re);
  await p.waitForTimeout(900);
}

const audit = p => p.evaluate(() => ({
  overflow: document.documentElement.scrollWidth > document.documentElement.clientWidth + 1,
  navColumns: document.querySelectorAll(".zp-section-nav").length,
  rows: document.querySelectorAll(".zp-table tbody tr").length,
  // Blue was retired as an action colour; any blue primary is a regression.
  bluePrimaries: [...document.querySelectorAll(".oa-primary-mini, .btn-primary")]
    .filter(el => {
      const bg = getComputedStyle(el).backgroundColor.match(/\d+/g)?.map(Number) ?? [];
      return bg.length >= 3 && bg[2] > bg[0] + 30 && bg[2] > bg[1] + 30;
    }).length,
  emptyState: document.querySelector(".zp-empty")?.className.match(/zp-empty-[a-z-]+/)?.[0] ?? "none",
}));

for (const [label, w, h] of [
  ["1920", 1920, 1080], ["1440", 1440, 900], ["1280", 1280, 820],
  ["1024", 1024, 800], ["768", 768, 1024],
]) {
  const p = await b.newPage({ viewport: { width: w, height: h } });
  await openDirectory(p);
  console.log(label, "directory", JSON.stringify(await audit(p)));
  await p.screenshot({ path: `qa-artifacts/cust-directory-${label}.png` });

  // Detail pane: click the first row's activator.
  await p.evaluate(() => document.querySelector(".zp-row-activator")?.click());
  await p.waitForTimeout(500);
  console.log(label, "detail   ", JSON.stringify(await audit(p)));
  await p.screenshot({ path: `qa-artifacts/cust-detail-${label}.png` });

  await openSection(p, "loyalty");
  console.log(label, "loyalty  ", JSON.stringify(await audit(p)));
  await p.screenshot({ path: `qa-artifacts/cust-loyalty-${label}.png` });
  await p.close();
}

// State coverage at one width: no-results, loyalty drawer, form drawer.
const p = await b.newPage({ viewport: { width: 1440, height: 900 } });
await openDirectory(p);

await p.fill(".zp-search input", "zzzzz");
await p.waitForTimeout(700);
console.log("no-results", JSON.stringify(await audit(p)));
await p.screenshot({ path: "qa-artifacts/cust-no-results.png" });
await p.fill(".zp-search input", "");
await p.waitForTimeout(700);

await p.evaluate(() => document.querySelector(".zp-row-activator")?.click());
await p.waitForTimeout(400);
await p.evaluate(() => [...document.querySelectorAll(".zp-cust-actions button")]
  .find(x => /point/i.test(x.textContent || ""))?.click());
await p.waitForTimeout(600);
await p.screenshot({ path: "qa-artifacts/cust-loyalty-drawer.png" });

// Over-redemption must be refused in the client, before it is ever sent.
await p.evaluate(() => [...document.querySelectorAll(".zp-cust-seg-btn")]
  .find(x => /redeem/i.test(x.textContent || ""))?.click());
await p.fill("#loyalty-points", "99999");
await p.waitForTimeout(400);
console.log("over-redeem:", JSON.stringify(await p.evaluate(() => ({
  error: document.querySelector("#loyalty-points-err")?.textContent ?? "none",
  submitDisabled: [...document.querySelectorAll(".zp-drawer-foot button, .zp-drawer button")]
    .some(b => /redeem/i.test(b.textContent || "") && b.disabled),
}))));
await p.screenshot({ path: "qa-artifacts/cust-loyalty-invalid.png" });

// A valid award must project the resulting balance before confirming.
await p.evaluate(() => [...document.querySelectorAll(".zp-cust-seg-btn")]
  .find(x => /award/i.test(x.textContent || ""))?.click());
await p.fill("#loyalty-points", "25");
await p.waitForTimeout(400);
console.log("projection:", await p.evaluate(
  () => document.querySelector(".zp-field-hint")?.textContent ?? "none"));
await p.screenshot({ path: "qa-artifacts/cust-loyalty-valid.png" });
// Close via the drawer's own Cancel — Radix leaves outside content aria-hidden
// until the dialog actually unmounts, so Escape from a focused field is not
// enough to make the page interactive again.
await p.locator(".zp-drawer").getByRole("button", { name: /^cancel$/i }).click();
await p.waitForSelector(".zp-drawer", { state: "detached" });

await p.getByRole("button", { name: /new customer/i }).first().click();
await p.waitForSelector("#cust-email");
await p.fill("#cust-email", "not-an-address");
await p.fill("#cust-phone", "ext. 12");
await p.waitForTimeout(400);
console.log("field errors shown:", await p.evaluate(
  () => document.querySelectorAll(".zp-cust-field-error").length));
await p.screenshot({ path: "qa-artifacts/cust-form-invalid.png" });

console.log("unstubbed commands:", await p.evaluate(
  () => (window.__uimockCalls ?? []).filter(c => c.startsWith("MISS")).join(", ") || "none"));
await p.close();

console.log("done");
await b.close();
