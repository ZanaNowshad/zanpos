import playwright from "../storefront/node_modules/@playwright/test/index.js";
const { chromium } = playwright;
const b = await chromium.launch();
const p = await b.newPage({ viewport: { width: 1440, height: 900 } });
const errs = [];
p.on("console", m => { if (m.type() === "error") errs.push(m.text().slice(0, 90)); });
p.on("pageerror", e => errs.push("PAGEERROR " + e.message.slice(0, 90)));

await p.goto("http://localhost:1420/?uimock=1", { waitUntil: "domcontentloaded" });
await p.waitForSelector(".oa-nav-item");
await p.evaluate(() => [...document.querySelectorAll(".oa-nav-item")]
  .find(x => /customer/i.test(x.textContent || ""))?.click());
await p.waitForTimeout(900);

/** Persistent shell chrome vs. page content — the two that used to disagree. */
const snap = () => p.evaluate(() => {
  const arabic = /[؀-ۿ]/;
  const sidebar = [...document.querySelectorAll(".oa-nav-item")]
    .map(n => n.textContent.trim()).join(" ");
  const page = document.querySelector(".oa-title")?.textContent?.trim() ?? "";
  return {
    dir: document.documentElement.getAttribute("dir"),
    lang: document.documentElement.getAttribute("lang"),
    sidebarArabic: arabic.test(sidebar),
    pageArabic: arabic.test(page),
    sidebarSample: sidebar.slice(0, 40),
    pageTitle: page,
  };
});

console.log("before:", JSON.stringify(await snap()));

// The language toggle in the persistent header.
await p.evaluate(() => {
  const btn = [...document.querySelectorAll("button")]
    .find(x => /^(ع|EN|AR|En)$/i.test(x.textContent.trim()));
  btn?.click();
});
await p.waitForTimeout(700);

const after = await snap();
console.log("after: ", JSON.stringify(after));
console.log("AGREE:", after.sidebarArabic === after.pageArabic
  ? "yes — shell and page switched together"
  : `NO — sidebarArabic=${after.sidebarArabic} pageArabic=${after.pageArabic}`);
await p.screenshot({ path: "qa-artifacts/language-ar.png" });

// Toggle back; both must return together too.
await p.evaluate(() => {
  const btn = [...document.querySelectorAll("button")]
    .find(x => /^(ع|EN|AR|En)$/i.test(x.textContent.trim()));
  btn?.click();
});
await p.waitForTimeout(700);
const back = await snap();
console.log("back:  ", JSON.stringify(back));
console.log("console:", errs.length ? errs.slice(0, 4) : "none");

await b.close();
