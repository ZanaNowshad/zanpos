// Measure the toolbar/panel overlap reported on the Command workspace pages.
//   node qa/toolbar-overlap.mjs [w] [h] [port]
//
// The layout audit passed these pages, so this probes the specific geometry the
// screenshots show: header/toolbar controls covered by the content panel that
// follows them. Detection is by hit-testing the control's own pixels, which is
// what "the user cannot click this" actually means.
import { chromium } from "playwright-core";
import { mkdirSync } from "node:fs";

const EXE = "C:/Users/super/AppData/Local/ms-playwright/chromium-1234/chrome-win64/chrome.exe";
const [, , w = "1024", h = "768", port = "1421"] = process.argv;

/** Sidebar label → tabs to visit within it. */
const PAGES = [
  ["Catalogue", ["Products", "Categories", "Inventory"]],
  ["Team", ["Staff", "Riders"]],
  ["Customers", []],
  ["Purchasing", []],
  ["Insights", []],
  ["Review", []],
  ["System", []],
  ["Today", []],
];

/**
 * `root` scopes the probe to one container. Needed for overlays: when the dock
 * is open its backdrop covers the whole page on purpose, so probing everything
 * reports seventeen "covered" nav items that are supposed to be covered. The
 * only question worth asking about an overlay is whether its *own* controls
 * are reachable.
 */
const probe = (page, label, root = null) =>
  page.evaluate(({ label, root }) => {
    const out = [];
    const vis = (el) => {
      const cs = getComputedStyle(el);
      return cs.display !== "none" && cs.visibility !== "hidden" && +cs.opacity > 0.01;
    };
    const scope = root ? document.querySelector(root) : document;
    if (!scope) return out;
    const controls = [...scope.querySelectorAll("button, a[href], input, select")].filter(vis);

    for (const el of controls) {
      const r = el.getBoundingClientRect();
      if (r.width < 2 || r.height < 2) continue;
      if (r.bottom < 0 || r.top > innerHeight) continue; // off-screen is a different defect

      const hit = (x, y) => {
        const t = document.elementFromPoint(x, y);
        return t && t !== el && !el.contains(t) && !t.contains(el) ? t : null;
      };
      const cx = r.left + r.width / 2;
      const centre = hit(cx, r.top + r.height / 2);
      const topEdge = hit(cx, r.top + 2);
      if (!centre && !topEdge) continue;

      const blocker = centre || topEdge;
      out.push({
        page: label,
        control: (el.textContent || el.getAttribute("placeholder") || el.tagName).trim().slice(0, 28),
        y: Math.round(r.top),
        centreBlocked: !!centre,
        blocker: `${blocker.tagName.toLowerCase()}.${String(blocker.className).slice(0, 45)}`,
      });
    }
    return out;
  }, { label, root });

const main = async () => {
  mkdirSync("qa/layout", { recursive: true });
  const browser = await chromium.launch({ executablePath: EXE });
  const ctx = await browser.newContext({ viewport: { width: +w, height: +h } });
  const page = await ctx.newPage();
  await page.goto(`http://127.0.0.1:${port}/?uimock=1`, { waitUntil: "domcontentloaded" });
  await page.waitForTimeout(2500);

  const findings = [];
  for (const [nav, tabs] of PAGES) {
    const navBtn = page.locator(".oa-nav-item", { hasText: new RegExp(`^${nav}$`) }).first();
    if (!(await navBtn.count())) continue;
    await navBtn.click();
    await page.waitForTimeout(900);

    if (tabs.length === 0) {
      findings.push(...(await probe(page, nav)));
      await page.screenshot({ path: `qa/layout/ov-${nav}-${w}x${h}.png` });
      continue;
    }
    for (const tab of tabs) {
      const t = page.locator("button", { hasText: new RegExp(`^${tab}$`) }).first();
      if (await t.count()) {
        await t.click().catch(() => {});
        await page.waitForTimeout(900);
      }
      findings.push(...(await probe(page, `${nav}/${tab}`)));
      await page.screenshot({ path: `qa/layout/ov-${nav}-${tab}-${w}x${h}.png` });
    }
  }

  /* Overlay surfaces, probed separately.
     Walking the nav pages only ever tests the layout at rest, and this harness
     reported a clean sweep while both controls in the ZanAI dock header sat
     underneath the fixed window buttons — "Collapse ZanAI" was under Close, so
     dismissing the assistant quit the application. Anything that opens over the
     page has to be opened before it can be measured. */
  const overlays = [
    { label: "ZanAI dock", root: ".oa-dock", open: async () => {
      const btn = page.locator("button", { hasText: /ZanAI/ }).first();
      if (!(await btn.count())) return false;
      await btn.click();
      await page.waitForTimeout(1200);
      return (await page.locator(".oa-dock").count()) > 0;
    } },
  ];
  for (const overlay of overlays) {
    if (!(await overlay.open())) {
      console.log(`! could not open "${overlay.label}" — not measured`);
      continue;
    }
    findings.push(...(await probe(page, overlay.label, overlay.root)));
    await page.screenshot({ path: `qa/layout/ov-dock-${w}x${h}.png` });
  }

  const byPage = {};
  for (const f of findings) (byPage[f.page] ||= []).push(f);
  for (const [p, list] of Object.entries(byPage)) {
    console.log(`\n── ${p} — ${list.length} covered control(s)`);
    for (const f of list.slice(0, 8)) {
      console.log(`   y=${String(f.y).padStart(4)} ${f.centreBlocked ? "CENTRE" : "top   "}  "${f.control}"  under ${f.blocker}`);
    }
  }
  console.log(`\nTOTAL ${findings.length} covered control(s) at ${w}x${h}`);
  await browser.close();
};

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
