// Walks every domain and section in the back office and reports how much
// vertical space each page spends before content begins, plus whether it is
// still printing a heading the section tab already said.
import { chromium } from "playwright-core";
const EXE = "C:/Users/super/AppData/Local/ms-playwright/chromium-1234/chrome-win64/chrome.exe";
const port = process.argv[2] || "1421";
const b = await chromium.launch({ executablePath: EXE });
const p = await (await b.newContext({ viewport: { width: 1024, height: 728 }, hasTouch: true })).newPage();
p.on("pageerror", e => console.log("  [pageerror]", String(e).slice(0, 150)));
await p.goto(`http://127.0.0.1:${port}/?uimock=1`, { waitUntil: "domcontentloaded" });
await p.waitForTimeout(2400);

const domains = await p.locator(".oa-primary-nav-items .oa-nav-item").allTextContents();
const rows = [];

for (const domain of domains.map(d => d.trim()).filter(Boolean)) {
  await p.locator(".oa-nav-item", { hasText: new RegExp(`^${domain}$`) }).first().click();
  await p.waitForTimeout(650);
  // Section tabs for this domain, if it has any.
  const tabs = await p.locator(".zp-section-nav button").allTextContents();
  const list = tabs.length ? tabs.map(t => t.trim()).filter(Boolean) : [null];
  for (const tab of list) {
    if (tab) {
      const btn = p.locator(".zp-section-nav button", { hasText: new RegExp(`^${tab}$`) }).first();
      if (!(await btn.count())) continue;
      await btn.click();
      await p.waitForTimeout(650);
    }
    const m = await p.evaluate(() => {
      const content = document.querySelector(".zp-workspace-content, .oa-embedded-tab, .zp-domain-content section");
      const h1 = document.querySelector(".oa-title");
      const band = document.querySelector(".zp-workspace > .oa-topbar");
      const sub = document.querySelector(".oa-subtitle");
      return {
        top: content ? Math.round(content.getBoundingClientRect().top) : -1,
        h1: h1?.textContent?.trim() ?? "",
        h1Visible: h1 ? !h1.className.includes("zp-visually-hidden") : false,
        band: band ? Math.round(band.getBoundingClientRect().height) : 0,
        sub: sub?.textContent?.trim().slice(0, 46) ?? "",
      };
    });
    rows.push({ page: `${domain}${tab ? "/" + tab : ""}`, ...m });
  }
}

rows.sort((a, b) => b.top - a.top);
console.log("page".padEnd(30), "content-y", "band", "visible-h1");
for (const r of rows) {
  const flag = r.top > 220 ? "  ← still heavy" : "";
  console.log(
    r.page.padEnd(30),
    String(r.top).padStart(6),
    String(r.band).padStart(5),
    " ",
    (r.h1Visible ? `"${r.h1}"` : "-").padEnd(22),
    r.sub ? `sub:"${r.sub}"` : "",
    flag,
  );
}
await b.close();
