import playwright from "../storefront/node_modules/@playwright/test/index.js";
const { chromium } = playwright;
const [W, H] = [Number(process.argv[2] ?? 1536), Number(process.argv[3] ?? 1024)];
const b = await chromium.launch();

for (const lang of ["en", "ar"]) {
  const p = await b.newPage({ viewport: { width: W, height: H } });
  const errs = [];
  p.on("console", m => { if (m.type() === "error") errs.push(m.text().slice(0, 80)); });
  p.on("pageerror", e => errs.push("PAGEERROR " + e.message.slice(0, 80)));
  await p.goto("http://localhost:1420/?uimock=1", { waitUntil: "domcontentloaded" });
  await p.evaluate(() => { localStorage.removeItem("zanpos_theme"); localStorage.removeItem("zanpos_language"); });
  await p.reload({ waitUntil: "domcontentloaded" });
  await p.waitForSelector(".oa-nav-item");
  await p.waitForTimeout(800);
  if (lang === "ar") {
    await p.evaluate(() => [...document.querySelectorAll("button")].find(x => x.textContent.trim() === "ع")?.click());
    await p.waitForTimeout(700);
  }
  await p.evaluate(() => [...document.querySelectorAll("button")].find(x => /zanai|زان/i.test(x.textContent||""))?.click());
  await p.waitForTimeout(1000);
  const m = await p.evaluate(() => {
    const r = el => { const b = el.getBoundingClientRect(); return [Math.round(b.left), Math.round(b.right)]; };
    const sheet = document.querySelector(".oa-copilot-sheet");
    const rail = document.querySelector(".oa-primary-nav");
    if (!sheet) return { sheet: "ABSENT" };
    const s = r(sheet), ra = r(rail);
    return {
      dir: document.documentElement.getAttribute("dir"),
      sheet: s, rail: ra,
      overlapsRail: !(s[1] <= ra[0] || s[0] >= ra[1]),
      bg: getComputedStyle(sheet).backgroundColor,
      overflow: document.documentElement.scrollWidth > document.documentElement.clientWidth + 1,
    };
  });
  console.log(lang, W, JSON.stringify(m), "console:", errs.length ? errs.slice(0,2) : "none");
  await p.screenshot({ path: `qa/parity/zanai-dock-${lang}-${W}.png` });
  await p.close();
}
await b.close();
