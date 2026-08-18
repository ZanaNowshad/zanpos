import playwright from "../storefront/node_modules/@playwright/test/index.js";
const { chromium } = playwright;
const b = await chromium.launch();
const p = await b.newPage({ viewport: { width: 1440, height: 900 }, acceptDownloads: true });

await p.goto("http://localhost:1420/?uimock=1", { waitUntil: "domcontentloaded" });
await p.waitForSelector(".oa-nav-item");
await p.evaluate(() => [...document.querySelectorAll(".oa-nav-item")]
  .find(x => /catalogue/i.test(x.textContent || ""))?.click());
await p.waitForTimeout(1000);

console.log("header actions:", await p.evaluate(
  () => [...document.querySelectorAll(".oa-topbar-actions button")].map(x => x.textContent.trim())));

const [download] = await Promise.all([
  p.waitForEvent("download"),
  p.getByRole("button", { name: /^export$/i }).first().click(),
]);
console.log("filename:", download.suggestedFilename());

const stream = await download.createReadStream();
let csv = "";
for await (const chunk of stream) csv += chunk;
csv = csv.replace(/^﻿/, "");

console.log("--- exported csv ---");
console.log(csv);
console.log("--- checks ---");
const lines = csv.split("\n");
console.log("header ok:", lines[0] === "name,category_name,price,sku,barcodes,track_inventory,tax_rule_name");
console.log("row count:", lines.length - 1);
console.log("no null/undefined:", !/null|undefined/.test(csv));
console.log("price format:", lines[1]?.split(",")[2]);

await p.screenshot({ path: "qa-artifacts/products-export.png" });
await b.close();
