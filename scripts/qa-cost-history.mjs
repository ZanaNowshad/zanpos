import playwright from "../storefront/node_modules/@playwright/test/index.js";
const { chromium } = playwright;
const b = await chromium.launch();

for (const [w, h] of [[1440, 900], [768, 1024]]) {
  const p = await b.newPage({ viewport: { width: w, height: h } });
  const errs = [];
  p.on("console", m => { if (m.type() === "error") errs.push(m.text().slice(0, 100)); });
  p.on("pageerror", e => errs.push("PAGEERROR " + e.message.slice(0, 100)));

  await p.goto("http://localhost:1420/?uimock=1", { waitUntil: "domcontentloaded" });
  await p.waitForSelector(".oa-nav-item");
  await p.evaluate(() => [...document.querySelectorAll(".oa-nav-item")]
    .find(x => /purchasing/i.test(x.textContent || ""))?.click());
  await p.waitForTimeout(1000);

  await p.getByRole("button", { name: /cost history/i }).first().click();
  await p.waitForSelector(".zp-drawer");
  await p.waitForTimeout(600);

  console.log(w, JSON.stringify(await p.evaluate(() => {
    const d = document.querySelector(".zp-drawer");
    return {
      title: d?.querySelector(".zp-drawer-title")?.textContent?.trim(),
      rows: d?.querySelectorAll("tbody tr").length ?? 0,
      // The copy must not promise linkage the schema lacks.
      forbidden: /reconcil|invoice|variance/i.test(d?.textContent || ""),
      overflow: document.documentElement.scrollWidth > document.documentElement.clientWidth + 1,
      footerReachable: (() => {
        const r = d?.getBoundingClientRect();
        return r ? r.bottom <= window.innerHeight + 1 : false;
      })(),
      body: d?.textContent?.replace(/\s+/g, " ").slice(0, 150),
    };
  })));
  await p.screenshot({ path: `qa-artifacts/cost-history-${w}.png` });
  console.log(w, "console:", errs.length ? errs.slice(0, 3) : "none");
  await p.close();
}
await b.close();
