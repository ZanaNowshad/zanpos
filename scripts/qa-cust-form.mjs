import playwright from "../storefront/node_modules/@playwright/test/index.js";
const { chromium } = playwright;
const b = await chromium.launch();
const p = await b.newPage({ viewport: { width: 1440, height: 900 } });

await p.goto("http://localhost:1420/?uimock=1", { waitUntil: "domcontentloaded" });
await p.waitForSelector(".oa-nav-item");
await p.evaluate(() => [...document.querySelectorAll(".oa-nav-item")]
  .find(x => /customer/i.test(x.textContent || ""))?.click());
await p.waitForTimeout(900);
await p.getByRole("button", { name: /new customer/i }).first().click();
await p.waitForSelector("#cust-email");

const state = () => p.evaluate(() => {
  const save = [...document.querySelectorAll(".zp-drawer button")]
    .find(x => /^save$/i.test(x.textContent.trim()));
  const email = document.querySelector("#cust-email");
  return {
    saveDisabled: save?.disabled ?? null,
    emailAriaInvalid: email?.getAttribute("aria-invalid"),
    emailBorder: getComputedStyle(email).borderColor,
    nameBorder: getComputedStyle(document.querySelector("#cust-name")).borderColor,
  };
});

console.log("empty form:      ", JSON.stringify(await state()));
await p.fill("#cust-email", "not-an-address");
await p.waitForTimeout(300);
console.log("bad email:       ", JSON.stringify(await state()));
await p.fill("#cust-name", "Ahmed Al Sayed");
await p.waitForTimeout(300);
console.log("name + bad email:", JSON.stringify(await state()));
await p.fill("#cust-email", "ahmed@example.test");
await p.waitForTimeout(300);
console.log("all valid:       ", JSON.stringify(await state()));

await b.close();
