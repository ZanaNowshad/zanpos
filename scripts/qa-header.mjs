import playwright from "../storefront/node_modules/@playwright/test/index.js";
const { chromium } = playwright;
const b = await chromium.launch();
const p = await b.newPage({ viewport: { width: 768, height: 1024 } });
await p.goto("http://localhost:1420/?uimock=1", { waitUntil: "networkidle" });
await p.waitForSelector(".zp-topbar");
console.log(JSON.stringify(await p.evaluate(() => {
  const h = document.querySelector(".zp-topbar");
  const hr = h.getBoundingClientRect();
  const kids = [...h.children].map(k => {
    const r = k.getBoundingClientRect(); const c = getComputedStyle(k);
    return { cls: k.className.slice(0,40), y: Math.round(r.y), h: Math.round(r.height), w: Math.round(r.width), disp: c.display, wrap: c.flexWrap };
  });
  return { header: { y: Math.round(hr.y), h: Math.round(hr.height), scrollH: h.scrollHeight, scrollW: h.scrollWidth, clientW: h.clientWidth }, kids };
}), null, 1));
await b.close();
