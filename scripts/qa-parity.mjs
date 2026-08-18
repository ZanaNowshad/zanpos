import playwright from "../storefront/node_modules/@playwright/test/index.js";
const { chromium } = playwright;

/**
 * Parity capture. Clears stored theme/language so a run always starts from the
 * product defaults rather than whatever a previous session left behind.
 */
const [navRe, sectionRe, slug, wRaw, hRaw] = process.argv.slice(2);
const W = Number(wRaw ?? 1536), H = Number(hRaw ?? 1024);

const b = await chromium.launch();
const p = await b.newPage({ viewport: { width: W, height: H }, deviceScaleFactor: 1 });
const errs = [];
p.on("console", m => { if (m.type() === "error") errs.push(m.text().slice(0, 100)); });
p.on("pageerror", e => errs.push("PAGEERROR " + e.message.slice(0, 100)));

await p.goto("http://localhost:1420/?uimock=1", { waitUntil: "domcontentloaded" });
await p.evaluate(() => { localStorage.removeItem("zanpos_theme"); localStorage.removeItem("zanpos_language"); });
await p.reload({ waitUntil: "domcontentloaded" });
await p.waitForSelector(".oa-nav-item");

if (navRe && navRe !== "-") {
  await p.evaluate(re => [...document.querySelectorAll(".oa-nav-item")]
    .find(x => new RegExp(re, "i").test(x.textContent || ""))?.click(), navRe);
  await p.waitForTimeout(1000);
}
if (sectionRe && sectionRe !== "-") {
  await p.evaluate(re => [...document.querySelectorAll(".zp-section-nav-item")]
    .find(x => new RegExp(re, "i").test(x.textContent || ""))?.click(), sectionRe);
  await p.waitForTimeout(900);
}

console.log(slug, JSON.stringify(await p.evaluate(() => ({
  theme: document.documentElement.getAttribute("data-theme"),
  bg: getComputedStyle(document.body).backgroundColor,
  overflow: document.documentElement.scrollWidth > document.documentElement.clientWidth + 1,
  h1: document.querySelector("h1")?.textContent?.trim()?.slice(0, 40) ?? "none",
}))));
await p.screenshot({ path: `qa/parity/${slug}.png` });
console.log("console:", errs.length ? errs.slice(0, 3) : "none");
await b.close();
