// Layout audit — drives the real UI in Chromium across viewport sizes, every
// primary nav domain and every section within it, and reports anything that is
// clipped away, pushed off the window, or unreachable.
//
//   node qa/layout-audit.mjs            # full sweep
//   node qa/layout-audit.mjs --shots    # also write a PNG per page
import { chromium } from "playwright-core";
import { mkdirSync, writeFileSync } from "node:fs";

const EXE = "C:/Users/super/AppData/Local/ms-playwright/chromium-1234/chrome-win64/chrome.exe";
const BASE = "http://127.0.0.1:1420/?uimock=1";
const OUT = "qa/layout";
const SHOTS = process.argv.includes("--shots");

/* Brackets the range the app can actually be shown at: tauri.conf.json pins the
   window to minWidth 1024 / minHeight 700 and opens maximized, so 1024x700 is
   the true floor and the large sizes cover the shop's own displays. 1280x600 is
   below the floor and is kept as a margin case. */
const VIEWPORTS = [
  { name: "2560x1440", width: 2560, height: 1440 },
  { name: "1920x1080", width: 1920, height: 1080 },
  { name: "1600x900", width: 1600, height: 900 },
  { name: "1440x900", width: 1440, height: 900 },
  { name: "1366x768", width: 1366, height: 768 },
  { name: "1280x800", width: 1280, height: 800 },
  { name: "1152x864", width: 1152, height: 864 },
  { name: "1024x768", width: 1024, height: 768 },
  { name: "1024x700", width: 1024, height: 700 },
  { name: "1280x600", width: 1280, height: 600 },
];

/** Intentionally out of flow — an audit hit on these is noise, not a defect.
 *  The sign-in geometry is decorative, pointer-events:none, and is *supposed*
 *  to bleed off the leading edge behind `overflow-x: hidden`; a closed drawer
 *  is *supposed* to be a zero-width box with its contents clipped away. */
const IGNORE = /oa-skip-link|zp-visually-hidden|login-geo-|pos-sidebar-hidden/;

const AUDIT = () => {
  const label = (el) => {
    const cls = typeof el.className === "string" ? el.className.trim() : "";
    return el.tagName.toLowerCase() + (el.id ? `#${el.id}` : "") +
      (cls ? "." + cls.split(/\s+/).slice(0, 3).join(".") : "");
  };
  const vw = document.documentElement.clientWidth;
  const vh = document.documentElement.clientHeight;
  const findings = [];
  // A closed drawer is a zero-width clipping box by design, and everything in
  // it is correctly offscreen. Skipping the subtree, not just the box itself.
  const inClosedDrawer = (el) => Boolean(el.closest(".pos-sidebar-hidden"));

  for (const el of document.querySelectorAll("*")) {
    if (inClosedDrawer(el)) continue;
    const cs = getComputedStyle(el);
    if (cs.display === "none" || cs.visibility === "hidden") continue;
    const r = el.getBoundingClientRect();
    if (r.width === 0 && r.height === 0) continue;
    const sel = label(el);

    // Content destroyed by an overflow:hidden box — invisible and unscrollable.
    // `text-overflow: ellipsis` is the exception: it is an explicit statement
    // that the text is meant to be shortened, and it leaves a visible marker.
    const truncatesOnPurpose = cs.textOverflow === "ellipsis";
    if (!truncatesOnPurpose && (cs.overflowX === "hidden" || cs.overflowX === "clip") && el.scrollWidth - el.clientWidth > 1) {
      findings.push({ kind: "clipped-x", sel, by: el.scrollWidth - el.clientWidth });
    }
    if ((cs.overflowY === "hidden" || cs.overflowY === "clip") && el.scrollHeight - el.clientHeight > 1) {
      findings.push({ kind: "clipped-y", sel, by: el.scrollHeight - el.clientHeight });
    }
    // Anything protruding past the window edge.
    if (r.right > vw + 1) findings.push({ kind: "past-right", sel, by: Math.round(r.right - vw) });
    if (r.left < -1) findings.push({ kind: "past-left", sel, by: Math.round(-r.left) });
  }

  // A data table wider than its wrapper is a defect even though the wrapper
  // scrolls: the row-action buttons sit in the last column, so they end up off
  // the right-hand edge and read as missing rather than as scrolled away. This
  // is the one case where "reachable by scrolling" is not good enough.
  for (const wrap of document.querySelectorAll(".zp-table-wrap")) {
    const over = wrap.scrollWidth - wrap.clientWidth;
    if (over > 1) findings.push({ kind: "table-wider-than-pane", sel: label(wrap), by: over });
  }

  // The window buttons are OS chrome painted over everything at the physical
  // top-right corner, in both writing directions. Anything that prints ink
  // under them is unreadable, and no `covered` check below will see it because
  // text is not a control.
  const ctrls = document.querySelector(".win-ctrls");
  if (ctrls) {
    const c = ctrls.getBoundingClientRect();
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_ELEMENT);
    for (let el = walker.nextNode(); el; el = walker.nextNode()) {
      if (ctrls.contains(el)) continue;
      const paints = [...el.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim());
      if (!paints && !el.matches("svg, img")) continue;
      const cs = getComputedStyle(el);
      if (cs.visibility === "hidden" || cs.opacity === "0") continue;
      const r = el.getBoundingClientRect();
      if (r.width < 2 || r.height < 2) continue;
      if (r.right > c.left && r.left < c.right && r.bottom > c.top && r.top < c.bottom) {
        findings.push({ kind: "under-window-buttons", sel: label(el), by: Math.round(Math.min(r.right, c.right) - Math.max(r.left, c.left)) });
      }
    }
  }

  // Controls that cannot be reached: off the window with no scroll container
  // able to bring them back, or covered by another element at their centre.
  // While an overlay is open the page behind it is *meant* to be unreachable,
  // so the reachability check follows the topmost overlay instead.
  const overlays = document.querySelectorAll("[role='dialog'], .modal-overlay, .zp-drawer");
  const root = overlays.length ? overlays[overlays.length - 1] : document;
  const stranded = [];
  for (const el of root.querySelectorAll("button, a[href], input, select, textarea, [role='button'], [role='tab']")) {
    if (inClosedDrawer(el)) continue;
    const r = el.getBoundingClientRect();
    if (r.width < 4 || r.height < 4) continue;
    const sel = label(el);
    const text = (el.textContent || el.getAttribute("placeholder") || el.getAttribute("aria-label") || "").trim().slice(0, 28);
    const off = r.top < 0 || r.bottom > vh || r.left < 0 || r.right > vw;
    if (off) {
      let n = el.parentElement, rescuer = null;
      while (n && n !== document.body) {
        const cs = getComputedStyle(n);
        if (/(auto|scroll|overlay)/.test(cs.overflowY) && n.scrollHeight - n.clientHeight > 1) { rescuer = "scroll-y"; break; }
        if (/(auto|scroll|overlay)/.test(cs.overflowX) && n.scrollWidth - n.clientWidth > 1) { rescuer = "scroll-x"; break; }
        n = n.parentElement;
      }
      if (!rescuer) stranded.push({ sel, text, why: "offscreen-unreachable", rect: [Math.round(r.left), Math.round(r.top), Math.round(r.right), Math.round(r.bottom)] });
      continue;
    }
    const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
    if (hit && hit !== el && !el.contains(hit) && !hit.contains(el)) {
      // A control sitting past the edge of its own scroll container is simply
      // below the fold; scrolling brings it back. Only report it when nothing
      // can scroll it into view.
      let n = el.parentElement, scrollable = false;
      while (n && n !== document.body) {
        const cs = getComputedStyle(n);
        if ((/(auto|scroll|overlay)/.test(cs.overflowY) && n.scrollHeight - n.clientHeight > 1) ||
            (/(auto|scroll|overlay)/.test(cs.overflowX) && n.scrollWidth - n.clientWidth > 1)) {
          const nr = n.getBoundingClientRect();
          if (r.top < nr.top || r.bottom > nr.bottom || r.left < nr.left || r.right > nr.right) scrollable = true;
          break;
        }
        n = n.parentElement;
      }
      if (!scrollable) stranded.push({ sel, text, why: "covered", by: label(hit) });
    }
  }

  const doc = document.documentElement;
  return {
    pageScrollX: doc.scrollWidth - doc.clientWidth,
    findings,
    stranded,
  };
};

const sweep = async (page, vpName, lang, report) => {
  const domains = await page.locator(".oa-primary-nav button[data-nav-id], .oa-primary-nav nav button").all();
  const domainNames = [];
  for (const d of domains) {
    const name = (await d.getAttribute("data-nav-id")) || (await d.innerText()).trim().split("\n")[0];
    if (name) domainNames.push(name);
  }

  for (let i = 0; i < domainNames.length; i++) {
    const btns = await page.locator(".oa-primary-nav nav button").all();
    if (!btns[i]) continue;
    try {
      await btns[i].click({ timeout: 3000 });
    } catch { continue; }
    await page.waitForTimeout(500);

    // Every section within the domain, plus the domain's own landing view.
    // Labels are captured up front: clicking a section re-renders the nav, so
    // holding element handles across iterations goes stale.
    const sectionLabels = await page.locator(".zp-section-nav-item").evaluateAll(
      (els) => els.map((e) => e.textContent.trim()).filter(Boolean),
    );
    const stops = sectionLabels.length ? sectionLabels : ["main"];
    for (const sectionLabel of stops) {
      if (sectionLabels.length) {
        const item = page.locator(".zp-section-nav-item", { hasText: sectionLabel }).first();
        try {
          await item.click({ timeout: 3000 });
        } catch { continue; }
        await page.waitForTimeout(450);
      }
      const result = await page.evaluate(AUDIT);
      const route = `${domainNames[i]}/${sectionLabel.replace(/\s+/g, "-")}`.slice(0, 48);
      report.push({ vp: vpName, lang, route, ...result });
      if (SHOTS) {
        await page.screenshot({ path: `${OUT}/${vpName}-${lang}-${route.replace(/[^\w.-]/g, "_")}.png` });
      }
    }
  }
};

/** Surfaces outside the domain/section grid: overlays, the till, sign-in.
 *  Each starts from a fresh load — Escape inside the shell exits to the till,
 *  so reusing one page across surfaces silently audits the wrong screen. */
const sweepExtras = async (page, vpName, lang, report) => {
  const record = async (route) => {
    const result = await page.evaluate(AUDIT);
    report.push({ vp: vpName, lang, route, ...result });
    if (SHOTS) await page.screenshot({ path: `${OUT}/${vpName}-${lang}-${route.replace(/[^\w.-]/g, "_")}.png` });
  };
  const reload = async () => {
    await page.goto(BASE, { waitUntil: "domcontentloaded" });
    await page.waitForSelector(".oa-page", { timeout: 20000 });
    await page.waitForTimeout(900);
  };

  // The product form is the densest form in the app and the one most likely to
  // outgrow a short window.
  try {
    await reload();
    await page.locator(".oa-primary-nav nav button").nth(2).click({ timeout: 3000 });
    await page.waitForTimeout(600);
    await page.locator(".oa-topbar-actions button").last().click({ timeout: 3000 });
    await page.waitForSelector(".bo-form-modal", { timeout: 4000 });
    await page.waitForTimeout(400);
    await record("overlay/new-product");
  } catch { /* form unavailable at this size */ }

  try {
    await reload();
    await page.keyboard.press("Control+k");
    await page.waitForTimeout(600);
    const open = await page.evaluate(() => Boolean(document.querySelector("[role='dialog'], .modal-overlay")));
    if (open) await record("overlay/command-palette");
  } catch { /* palette unavailable */ }

  // Leaving the shell lands on the register-handoff screen. Audit it, then sign
  // in (the mock accepts PIN 1234) and audit the till behind it — the till is
  // the other half of the app and has its own fixed-width panels.
  try {
    await reload();
    await page.locator(".oa-back-btn").first().click({ timeout: 3000 });
    await page.waitForSelector(".login-screen", { timeout: 5000 });
    await page.waitForTimeout(700);
    await record("view/sign-in");

    await page.locator(".user-card").first().click({ timeout: 3000 });
    await page.waitForTimeout(500);
    await page.keyboard.type("1234", { delay: 60 });
    await page.keyboard.press("Enter");
    await page.waitForSelector(".pos-layout", { timeout: 8000 });
    await page.waitForTimeout(1200);
    await record("view/till");

    // The payment modal is the densest screen in the app — three panels, the
    // tender keypad, and on a delivery sale a whole address form. It needs a
    // cart line before any tender route will open it.
    await page.locator("input[placeholder*='Scan'], input[placeholder*='امسح']").first().fill("6280123456781");
    await page.keyboard.press("Enter");
    await page.waitForTimeout(900);
    for (const [route, index] of [["receipt", 0], ["delivery", 1], ["digital", 2]]) {
      try {
        await page.locator(".np-journey-btn").nth(index).click({ timeout: 3000 });
        await page.waitForSelector(".pm-shell", { timeout: 5000 });
        await page.waitForTimeout(600);
        await record(`payment/${route}`);
        await page.locator(".pm-close-btn").click({ timeout: 3000 });
        await page.waitForTimeout(400);
      } catch { /* route unavailable at this size */ }
    }
  } catch { /* could not reach the till from this state */ }
};

const main = async () => {
  mkdirSync(OUT, { recursive: true });
  const browser = await chromium.launch({ executablePath: EXE });
  const report = [];

  for (const vp of VIEWPORTS) {
    for (const lang of ["en", "ar"]) {
      const ctx = await browser.newContext({ viewport: { width: vp.width, height: vp.height } });
      await ctx.addInitScript((l) => { localStorage.setItem("zanpos_language", l); }, lang);
      const page = await ctx.newPage();
      await page.goto(BASE, { waitUntil: "domcontentloaded" });
      await page.waitForSelector(".oa-page", { timeout: 20000 });
      await page.waitForTimeout(900);
      await sweep(page, vp.name, lang, report);
      await sweepExtras(page, vp.name, lang, report);
      await ctx.close();
    }
  }
  await browser.close();
  writeFileSync(`${OUT}/report.json`, JSON.stringify(report, null, 1));

  const rows = [];
  for (const r of report) {
    for (const f of r.findings) if (!IGNORE.test(f.sel)) rows.push({ ...f, vp: r.vp, lang: r.lang, route: r.route });
  }
  const strand = [];
  for (const r of report) {
    for (const s of r.stranded) if (!IGNORE.test(s.sel)) strand.push({ ...s, vp: r.vp, lang: r.lang, route: r.route });
  }

  const tally = (list, keyFn) => {
    const m = new Map();
    for (const x of list) {
      const k = keyFn(x);
      const e = m.get(k) || { k, n: 0, worst: 0, where: new Set() };
      e.n++; e.worst = Math.max(e.worst, x.by || 0); e.where.add(`${x.vp}/${x.lang}`);
      m.set(k, e);
    }
    return [...m.values()].sort((a, b) => b.n - a.n);
  };

  console.log(`\naudited ${report.length} page states`);
  const hscroll = report.filter((r) => r.pageScrollX > 0);
  console.log(`window-level horizontal scroll: ${hscroll.length === 0 ? "none (good)" : hscroll.map((r) => r.vp + " " + r.route).join(", ")}`);

  console.log(`\n── clipped / overflowing (${rows.length} hits) ──`);
  for (const t of tally(rows, (x) => `${x.kind.padEnd(11)} ${x.sel}`).slice(0, 40)) {
    console.log(` ${String(t.n).padStart(4)}x  worst ${String(t.worst).padStart(4)}px  ${t.k}`);
  }

  console.log(`\n── unreachable controls (${strand.length} hits) ──`);
  for (const t of tally(strand, (x) => `${x.why.padEnd(21)} ${x.sel} ${x.by ? "<- " + x.by : ""} "${x.text}"`).slice(0, 40)) {
    console.log(` ${String(t.n).padStart(4)}x  ${t.k}`);
  }

  console.log(`\n── worst routes ──`);
  const byRoute = new Map();
  for (const x of [...rows, ...strand]) {
    const k = `${x.vp} ${x.lang} ${x.route}`;
    byRoute.set(k, (byRoute.get(k) || 0) + 1);
  }
  for (const [k, n] of [...byRoute.entries()].sort((a, b) => b[1] - a[1]).slice(0, 20)) {
    console.log(` ${String(n).padStart(4)}  ${k}`);
  }
};

main().catch((e) => { console.error(e); process.exit(1); });
