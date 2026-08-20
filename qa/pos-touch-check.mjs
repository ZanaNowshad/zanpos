// POS reality check, run at the store's actual spec: 1024x768 physical, 100%
// Windows scaling, touch. That gives a ~728px-tall CSS viewport once the
// taskbar is out, and `pointer: coarse` for every control.
//
// Checks two things a mouse-and-laptop pass cannot see:
//   1. controls covered by something else (not merely scrolled away), and
//   2. tap targets below the 40px this codebase sets for coarse pointers.
//
//   node qa/pos-touch-check.mjs [port] [--all-scaling]
import { chromium } from "playwright-core";
import { mkdirSync } from "node:fs";

const EXE = "C:/Users/super/AppData/Local/ms-playwright/chromium-1234/chrome-win64/chrome.exe";
const port = process.argv[2] && !process.argv[2].startsWith("--") ? process.argv[2] : "1421";
const allScaling = process.argv.includes("--all-scaling");

/** The store's till. Extra rows only to prove DPI scaling stays safe. */
const CASES = allScaling
  ? [["100% (the till)", 1024, 728], ["125% scaling", 819, 582], ["150% scaling", 683, 486]]
  : [["100% (the till)", 1024, 728]];

/** Matches `@media (pointer: coarse) { .zp-row-action { height: 40px } }`. */
const MIN_TAP = 40;

const PAGES = [
  ["Today", []],
  ["Sell", []],
  ["Catalogue", ["Products", "Categories", "Inventory", "Quick POS"]],
  ["Purchasing", []],
  ["Customers", []],
  ["Team", ["Staff", "Riders"]],
  ["Insights", []],
  ["Review", []],
  ["System", []],
];

const AUDIT = (MIN_TAP) => {
  const vis = (el) => {
    const cs = getComputedStyle(el);
    return cs.display !== "none" && cs.visibility !== "hidden" && +cs.opacity > 0.01;
  };
  /* Outside a scrollable ancestor's box is "one scroll away", not "covered".
     Hit-testing alone cannot tell them apart — it reports whatever is painted
     at those coordinates, which is usually an unrelated sibling. */
  const scrolledAway = (el) => {
    for (let a = el.parentElement; a && a !== document.body; a = a.parentElement) {
      const cs = getComputedStyle(a);
      if (!/auto|scroll/.test(cs.overflowY + cs.overflowX)) continue;
      if (a.scrollHeight <= a.clientHeight + 2 && a.scrollWidth <= a.clientWidth + 2) continue;
      const ar = a.getBoundingClientRect();
      const r = el.getBoundingClientRect();
      if (r.bottom > ar.bottom + 1 || r.top < ar.top - 1) return true;
      if (r.right > ar.right + 1 || r.left < ar.left - 1) return true;
    }
    return false;
  };
  const name = (el) =>
    (el.getAttribute("aria-label") || el.textContent || el.getAttribute("placeholder") || el.tagName)
      .trim().slice(0, 30) || el.tagName;

  const covered = [];
  const small = [];
  for (const el of [...document.querySelectorAll("button, a[href], input, select, [role=tab], [role=button]")].filter(vis)) {
    const r = el.getBoundingClientRect();
    if (r.width < 2 || r.height < 2) continue;
    if (r.bottom < 0 || r.top > innerHeight) continue;
    if (scrolledAway(el)) continue;

    const t = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
    if (t && t !== el && !el.contains(t) && !t.contains(el)) {
      covered.push(`${name(el)} <- ${t.tagName.toLowerCase()}.${String(t.className).slice(0, 30)}`);
    }
    // Checkboxes and radios keep their native size; the label is the target.
    if (el.type === "checkbox" || el.type === "radio") continue;
    /* Minimise/maximise/close follow the OS title-bar convention at 46x32.
       Enlarging Close on a till invites the one misfire nobody wants mid-sale,
       so this is a deliberate exemption rather than an outstanding defect. */
    if (el.closest(".win-btn") || el.classList.contains("win-btn")) continue;
    if (r.height < MIN_TAP - 0.5 || r.width < 24) {
      small.push(`${name(el)} ${Math.round(r.width)}x${Math.round(r.height)}`);
    }
  }
  return { covered, small, coarse: matchMedia("(pointer: coarse)").matches };
};

const run = async (browser, label, w, h) => {
  mkdirSync("qa/layout", { recursive: true });
  const ctx = await browser.newContext({ viewport: { width: w, height: h }, hasTouch: true });
  const p = await ctx.newPage();
  await p.goto(`http://127.0.0.1:${port}/?uimock=1`, { waitUntil: "domcontentloaded" });
  await p.waitForTimeout(2400);

  let covered = 0, small = 0;
  console.log(`\n══ ${label} — ${w}x${h}, touch`);
  for (const [nav, tabs] of PAGES) {
    const btn = p.locator(".oa-nav-item", { hasText: new RegExp(`^${nav}$`) }).first();
    if (!(await btn.count())) continue;
    await btn.click().catch(() => {});
    await p.waitForTimeout(850);

    const visit = async (tag) => {
      const res = await p.evaluate(AUDIT, MIN_TAP);
      covered += res.covered.length;
      small += res.small.length;
      if (res.covered.length || res.small.length) {
        console.log(`  ${tag}: covered=${res.covered.length} undersized=${res.small.length}`);
        for (const c of res.covered.slice(0, 3)) console.log(`     COVERED  ${c}`);
        for (const s of [...new Set(res.small)]) console.log(`     SMALL    ${s}`);
      }
      await p.screenshot({ path: `qa/layout/pos-${tag.replace(/\W+/g, "-")}-${w}x${h}.png` });
    };

    if (!tabs.length) { await visit(nav); continue; }
    for (const tab of tabs) {
      const t = p.locator("button", { hasText: new RegExp(`^${tab}$`) }).first();
      if (await t.count()) { await t.click().catch(() => {}); await p.waitForTimeout(850); }
      await visit(`${nav}/${tab}`);
    }
  }
  console.log(`  ── total: ${covered} covered, ${small} undersized (min ${MIN_TAP}px)`);
  await ctx.close();
  return covered + small;
};

const main = async () => {
  const browser = await chromium.launch({ executablePath: EXE });
  let bad = 0;
  for (const [label, w, h] of CASES) bad += await run(browser, label, w, h);
  await browser.close();
  console.log(`\n${bad === 0 ? "PASS" : "FAIL"} — ${bad} issue(s)`);
  process.exit(bad === 0 ? 0 : 1);
};
main().catch((e) => { console.error(e); process.exit(1); });
