// Every way a key can reach the payment modal, checked on the till's real panel.
//   node qa/payment-input-check.mjs [port]
//
// Exists because all three input paths were broken at once and none of them
// showed up in the unit suite: the fields kept focus and rendered correctly,
// they just silently discarded what was typed. Only a real browser catches
// that, so this drives the modal the way a cashier does.
import { chromium } from "playwright-core";

const EXE = "C:/Users/super/AppData/Local/ms-playwright/chromium-1234/chrome-win64/chrome.exe";
const port = process.argv[2] || "1421";
const failures = [];
const check = (ok, label, detail = "") => {
  console.log(`${ok ? "PASS" : "FAIL"}  ${label}${detail ? `  ${detail}` : ""}`);
  if (!ok) failures.push(label);
};

const b = await chromium.launch({ executablePath: EXE });
const ctx = await b.newContext({ viewport: { width: 1024, height: 728 }, hasTouch: true });
const p = await ctx.newPage();
const pageErrors = [];
p.on("pageerror", e => pageErrors.push(String(e).slice(0, 160)));

await p.goto(`http://127.0.0.1:${port}/?uimock=1`, { waitUntil: "domcontentloaded" });
await p.waitForTimeout(2400);
await p.locator(".oa-back-btn").first().click();
await p.waitForSelector(".login-screen", { timeout: 8000 });
await p.waitForTimeout(600);
await p.locator(".user-card").first().click();
await p.waitForTimeout(500);
await p.keyboard.type("1234", { delay: 60 });
await p.keyboard.press("Enter");
await p.waitForSelector(".pos-layout", { timeout: 10000 });
await p.waitForTimeout(1400);
await p.keyboard.type("6280123456781", { delay: 5 });
await p.keyboard.press("Enter");
await p.waitForTimeout(500);

const openDelivery = async () => {
  if (!(await p.locator(".pm-shell").count())) {
    await p.keyboard.press("F6");
    await p.waitForSelector(".pm-shell", { timeout: 8000 });
    await p.waitForTimeout(400);
  }
  const opt = p.locator(".pm-journey-opt", { hasText: /^Delivery$/ }).first();
  if ((await opt.getAttribute("aria-pressed")) !== "true") {
    await opt.click();
    await p.waitForTimeout(600);
  }
};
const blur = () => p.evaluate(() => document.activeElement?.blur());
/* Select-all then type, the way a cashier corrects a field. An earlier version
   wrote `el.value = ""` first, which desynchronises React's value tracker and
   made the fill() that followed a silent no-op — the harness was fighting the
   same class of bug it exists to catch. */
const retype = async (selector, text) => {
  /* No settling pause between the tap and the first keystroke. A cashier taps
     a field and starts typing in the same motion, and every earlier version of
     this helper that paused first — even by one round trip — hid a race that
     swallowed the opening characters. */
  await p.locator(selector).click();
  await p.keyboard.press("Control+a");
  await p.keyboard.press("Backspace");
  if (text) await p.keyboard.type(text, { delay: 25 });
  await p.waitForTimeout(250);
};
await openDelivery();

// ── 1. The whole journey has to be on screen at once ────────────────────────
const layout = await p.evaluate(() => {
  const out = { columns: {}, rider: null, viewportH: window.innerHeight };
  for (const sel of [".pm-left", ".pm-mid", ".pm-right"]) {
    const el = document.querySelector(sel);
    out.columns[sel] = el ? el.scrollHeight - el.clientHeight : -1;
  }
  const rider = document.querySelector(".pm-rider");
  if (rider) {
    const r = rider.getBoundingClientRect();
    out.rider = { top: Math.round(r.top), bottom: Math.round(r.bottom) };
  }
  return out;
});
for (const [sel, overflow] of Object.entries(layout.columns)) {
  check(overflow <= 0, `${sel} needs no scrolling`, `overflow=${overflow}px`);
}
/* The right column is taller with the keypad up than at rest, and the keypad
   is its normal state while an address is being typed. Measuring only the
   resting surface let Cancel sit below the fold. */
await p.locator("#payment-customer-phone").click();
await p.waitForTimeout(500);
const rightWithPad = await p.evaluate(() => {
  const el = document.querySelector(".pm-right");
  const cancel = document.querySelector(".pm-cancel-btn")?.getBoundingClientRect();
  return {
    overflow: el.scrollHeight - el.clientHeight,
    cancelBottom: cancel ? Math.round(cancel.bottom) : null,
    viewportH: window.innerHeight,
  };
});
check(rightWithPad.overflow <= 0, ".pm-right needs no scrolling with the keypad up",
  `overflow=${rightWithPad.overflow}px`);
check(rightWithPad.cancelBottom !== null && rightWithPad.cancelBottom <= rightWithPad.viewportH,
  "Cancel is fully on screen with the keypad up",
  `bottom=${rightWithPad.cancelBottom} viewport=${rightWithPad.viewportH}`);
await blur();
await p.waitForTimeout(300);
check(
  layout.rider && layout.rider.bottom <= layout.viewportH && layout.rider.top >= 0,
  "rider picker is on screen without scrolling",
  layout.rider ? `bottom=${layout.rider.bottom} viewport=${layout.viewportH}` : "missing",
);

// ── 2. The overlay must not be an activatable element ───────────────────────
const overlayTag = await p.evaluate(() =>
  document.querySelector(".modal-overlay")?.tagName.toLowerCase() ?? "missing");
check(overlayTag === "div", "overlay is not a button", `<${overlayTag}>`);

// ── 3. Physical keyboard reaches every field ────────────────────────────────
for (const [selector, text] of [
  ["#payment-customer-phone", "Fat"],
  ["#payment-house", "12"],
  ["#payment-flat", "3B"],
  ["#payment-road", "Road 2814"],
]) {
  await p.locator(selector).click();
  await p.waitForTimeout(120);
  await p.keyboard.type(text, { delay: 45 });
  await p.waitForTimeout(220);
  const value = await p.locator(selector).inputValue();
  check(value === text, `typing reaches ${selector}`, `value="${value}"`);
}

// ── 4. Space types a space; it must not dismiss the sale ────────────────────
await p.locator("#payment-road").click();
await p.keyboard.press("End");
await p.keyboard.press("Space");
await p.keyboard.type("B", { delay: 40 });
await p.waitForTimeout(220);
check(await p.locator(".pm-shell").count() === 1, "Space does not close the modal");
check((await p.locator("#payment-road").inputValue()) === "Road 2814 B", "Space types a space",
  `value="${await p.locator("#payment-road").inputValue()}"`);

// ── 5. Enter moves to the next field instead of saving a half-filled order ──
await p.locator("#payment-house").click();
await p.waitForTimeout(120);
await p.keyboard.press("Enter");
await p.waitForTimeout(220);
const afterEnter = await p.evaluate(() => document.activeElement?.id ?? "none");
check(afterEnter === "payment-flat", "Enter advances to the next field", `focus=${afterEnter}`);
check(await p.locator(".pm-shell").count() === 1, "Enter in a field does not complete the sale");

// ── 6. The on-screen keys and the dialpad both reach the focused field ──────
await retype("#payment-road", "");
await p.waitForTimeout(300);
const toKeys = p.locator(".pm-mode-btn");
if ((await toKeys.count()) && (await toKeys.textContent()) === "ABC") await toKeys.click();
await p.waitForTimeout(300);
const keyA = p.locator(".tkb-key", { hasText: /^a$/ }).first();
check(await keyA.count() > 0, "on-screen letters are available");
if (await keyA.count()) {
  await keyA.click();
  await p.locator(".tkb-space").click();
  await p.waitForTimeout(220);
  check((await p.locator("#payment-road").inputValue()) === "a ",
    "touch keyboard types into the focused field",
    `value="${await p.locator("#payment-road").inputValue()}"`);
}
// Arabic is a layout switch on the same keyboard, not a second one.
const script = p.locator(".tkb-script");
if (await script.count()) {
  await script.click();
  await p.waitForTimeout(200);
  const arabicKey = p.locator(".tkb-key", { hasText: /^ض$/ }).first();
  check(await arabicKey.count() > 0, "Arabic letters are one tap away");
  if (await arabicKey.count()) {
    await arabicKey.click();
    await p.waitForTimeout(200);
    check((await p.locator("#payment-road").inputValue()).includes("ض"),
      "Arabic letters type into the focused field");
  }
  await script.click();
  await p.waitForTimeout(150);
}

await retype("#payment-house", "");
await p.waitForTimeout(300);
const toPad = p.locator(".pm-mode-btn");
if ((await toPad.count()) && (await toPad.textContent()) === "123") await toPad.click();
await p.waitForTimeout(300);
const key7 = p.locator(".dialpad-key", { hasText: /^7$/ }).first();
check(await key7.count() > 0, "dialpad is available for the address fields");
if (await key7.count()) {
  await key7.click();
  await p.waitForTimeout(220);
  check((await p.locator("#payment-house").inputValue()) === "7",
    "dialpad types into the focused field",
    `value="${await p.locator("#payment-house").inputValue()}"`);
}

// ── 7. The numeric keypad on a real keyboard ────────────────────────────────
/* Playwright's own `press("Numpad4")` emulates NumLock *off*, which sends
   ArrowLeft — the till runs with NumLock on, so that would be testing a key
   the shop never presses. Dispatched through CDP instead, with the location
   and text a real numpad sends. */
const cdp = await ctx.newCDPSession(p);
const numpad = async (key, code, text) => {
  for (const type of ["keyDown", "keyUp"]) {
    await cdp.send("Input.dispatchKeyEvent", {
      type: type === "keyDown" ? "keyDown" : "keyUp",
      key, code, location: 3,
      ...(type === "keyDown" && text ? { text } : {}),
    });
  }
};
await retype("#payment-house", "");
await numpad("4", "Numpad4", "4");
await numpad("2", "Numpad2", "2");
await p.waitForTimeout(220);
check((await p.locator("#payment-house").inputValue()) === "42", "numeric keypad reaches the field",
  `value="${await p.locator("#payment-house").inputValue()}"`);
await numpad("Enter", "NumpadEnter");
await p.waitForTimeout(250);
const afterNumpadEnter = await p.evaluate(() => document.activeElement?.id ?? "none");
check(afterNumpadEnter === "payment-flat", "the keypad's Enter advances like the main one",
  `focus=${afterNumpadEnter}`);

// ── 8. Contact field: names as well as numbers, from all three directories ──
await retype("#payment-customer-phone", "Mariam");
check((await p.locator("#payment-customer-phone").inputValue()) === "Mariam",
  "contact field accepts letters");
/* Waited for rather than slept on. The lookup is debounced and hits two
   commands, so a fixed sleep measures the machine as much as the code — and a
   cashier does not care whether the list took 300ms or 800ms, only that it
   arrives. A bounded wait still fails if it never does. */
const started = Date.now();
const appeared = await p.waitForSelector(".pm-contact-suggestions button", { timeout: 4000 })
  .then(() => true).catch(() => false);
const sugg = p.locator(".pm-contact-suggestions button");
check(appeared, "a name produces suggestions", `${await sugg.count()} rows in ${Date.now() - started}ms`);
const suggText = (await p.locator(".pm-contact-suggestions").textContent().catch(() => "")) ?? "";
check(/Chatted|WhatsApp/.test(suggText),
  "suggestions reach beyond the customer table", suggText.replace(/\s+/g, " ").slice(0, 80));
if (await sugg.count()) {
  await sugg.first().click();
  await p.waitForTimeout(400);
  check((await p.locator(".pm-contact-match").count()) > 0, "picking a suggestion resolves a number",
    ((await p.locator(".pm-contact-match").textContent().catch(() => "")) ?? "").trim().slice(0, 60));
}

await retype("#payment-customer-phone", "36001122");
await p.waitForSelector(".pm-contact-suggestions button", { timeout: 4000 }).catch(() => {});
check((await p.locator("#payment-customer-phone").inputValue()) === "36001122",
  "contact field still accepts a plain number",
  await p.evaluate(() => {
    const el = document.getElementById("payment-customer-phone");
    const key = Object.keys(el).find(k => k.startsWith("__reactProps$"));
    return `dom="${el.value}" reactProp="${key ? el[key].value : "?"}" focus=${document.activeElement?.id || document.activeElement?.tagName}`;
  }));
const numText = (await p.locator(".pm-contact-suggestions").textContent().catch(() => "")) ?? "";
check(/Fatima/.test(numText), "a number finds the saved customer behind it",
  `dropdown="${numText.replace(/\s+/g, " ").slice(0, 90)}" status="${((await p.locator(".pm-contact-match, .pm-contact-error, .pm-contact-help").first().textContent().catch(() => "")) ?? "").trim().slice(0, 50)}"`);

// ── 9. The directory: typeable, and Escape closes only it ───────────────────
await blur();
await p.waitForTimeout(300);
await p.locator(".pm-directory-btn").click();
await p.waitForSelector(".pm-directory-dialog", { timeout: 4000 });
await p.waitForTimeout(400);
/* Typed fast and repeatedly: the directory box had the same fault as the
   contact field — results arriving mid-word re-rendered the input and ate a
   character, so "Ali" landed as "Al". */
let dirLost = 0;
for (let attempt = 0; attempt < 4; attempt++) {
  await p.locator(".pm-directory-search input").click();
  await p.keyboard.press("Control+a");
  await p.keyboard.press("Backspace");
  await p.waitForTimeout(120);
  await p.keyboard.type("Ali", { delay: 30 });
  await p.waitForTimeout(320);
  if ((await p.locator(".pm-directory-search input").inputValue()) !== "Ali") dirLost++;
}
check(dirLost === 0, "directory search accepts typing", `${dirLost}/4 bursts lost characters`);
await p.keyboard.press("Space");
await p.waitForTimeout(300);
check(await p.locator(".pm-directory-dialog").count() === 1, "Space does not close the directory");
check((await p.locator(".pm-directory-search input").inputValue()) === "Ali ",
  "Space types a space in the directory",
  `value="${await p.locator(".pm-directory-search input").inputValue()}"`);
const dirKeys = p.locator(".pm-directory-keys-btn");
if (await dirKeys.count()) {
  await dirKeys.click();
  await p.waitForTimeout(300);
  check(await p.locator(".pm-directory-keys .tkb-key").count() > 0,
    "directory carries its own on-screen keys");
}
await p.keyboard.press("Escape");
await p.waitForTimeout(400);
check(await p.locator(".pm-directory-dialog").count() === 0, "Escape closes the directory");
check(await p.locator(".pm-shell").count() === 1, "Escape does not also close the sale");

// ── 10. Correcting the field right after picking someone ───────────────────
/* The nastiest sequence in this modal, and the one that lost characters:
   choose a suggestion — which sets state, closes the list and moves focus
   programmatically, all inside one mousedown — then immediately select-all and
   retype at speed while the debounced lookup for the cleared field is still in
   flight. "36001122" came back as "3600112", and sometimes as nothing at all.
   Repeated, because an intermittent fault checked once is a coin toss. */
let lost = 0;
const losses = [];
for (let attempt = 0; attempt < 5; attempt++) {
  await p.locator("#payment-customer-phone").click();
  await p.keyboard.press("Control+a");
  await p.keyboard.press("Backspace");
  await p.keyboard.type("Mariam", { delay: 30 });
  const listed = await p.waitForSelector(".pm-contact-suggestions button", { timeout: 4000 })
    .then(() => true).catch(() => false);
  if (!listed) { losses.push("no suggestions"); lost++; continue; }
  await p.locator(".pm-contact-suggestions button").first().click();
  await p.waitForTimeout(150);

  await p.locator("#payment-customer-phone").click();
  await p.keyboard.press("Control+a");
  await p.keyboard.press("Backspace");
  await p.waitForTimeout(120);
  await p.keyboard.type("36001122", { delay: 35 });
  await p.waitForTimeout(320);
  const typed = await p.locator("#payment-customer-phone").inputValue();
  if (typed !== "36001122") { losses.push(`"${typed}"`); lost++; }
}
check(lost === 0, "no characters lost when correcting a picked contact",
  `${lost}/5 bursts lost characters${losses.length ? ` -> ${losses.join(", ")}` : ""}`);

// ── 11. The whole point: a delivery still completes ─────────────────────────
/* The contact field now feeds the delivery record through a different path, so
   this walks the journey end to end. A modal that takes input beautifully and
   then cannot save is not an improvement. */
await retype("#payment-customer-phone", "36001122");
await p.waitForTimeout(700);
await p.locator("#payment-house").click();
await p.keyboard.type("12", { delay: 30 });
await p.waitForTimeout(300);
await blur();
await p.waitForTimeout(400);
const confirmText = ((await p.locator(".pm-confirm-btn").textContent()) ?? "").trim();
const blocked = await p.locator(".pm-confirm-btn-blocked").count();
check(blocked === 0, "a named contact and a house number unblock the sale",
  `cta="${confirmText}" status="${((await p.locator(".pm-confirm-status").textContent()) ?? "").trim().slice(0, 50)}"`);
await p.screenshot({ path: "qa/layout/pay-delivery-fit.png" });
if (blocked === 0) {
  await p.locator(".pm-confirm-btn").click();
  await p.waitForTimeout(2500);
  check(await p.locator(".pm-shell").count() === 0, "confirming closes the modal and records the sale");
}

check(pageErrors.length === 0, "no page errors", pageErrors.join(" | "));
await b.close();

console.log(failures.length ? `\n${failures.length} FAILED` : "\nall checks passed");
process.exit(failures.length ? 1 : 0);
