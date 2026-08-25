// Hold and Void traded places. Both must still work from where they now are.
//   node qa/till-hold-void.mjs [port]
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
p.on("pageerror", e => console.log("[pageerror]", String(e).slice(0, 160)));
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
for (const code of ["6280123456781", "6280987654321"]) {
  await p.keyboard.type(code, { delay: 5 });
  await p.keyboard.press("Enter");
  await p.waitForTimeout(450);
}

const rowCount = () => p.locator(".till-cart-body .till-row").count();
const rail = p.locator(".till-line-actions");

// ── The rail under the cart ─────────────────────────────────────────────────
const labels = (await rail.locator("button span:not(.till-key)").allTextContents())
  .map(s => s.trim()).filter(Boolean);
check(labels.join("|") === "Qty|Price|Hold|Discount",
  "the rail reads Qty, Price, Hold, Discount", labels.join(" | "));
check(!labels.includes("Void"), "Void is no longer on the rail");

/* Hold must not disable with the line actions: resuming a parked bill is done
   with an empty cart and nothing selected, which is exactly when the per-line
   buttons are dead. */
const holdBtn = rail.locator("button", { hasText: "Hold" });
check(await holdBtn.isEnabled(), "Hold is enabled with items in the cart");

await holdBtn.click();
await p.waitForTimeout(700);
check(await p.locator(".modal-overlay, [role='dialog']").count() > 0,
  "Hold opens the hold/resume dialog");
await p.keyboard.press("Escape");
await p.waitForTimeout(500);

// ── Void, now behind More Options ───────────────────────────────────────────
const before = await rowCount();
await p.locator(".till-more").click();
await p.waitForSelector(".till-more-drawer", { timeout: 4000 });
await p.waitForTimeout(500);
const drawerText = (await p.locator(".till-more-drawer").textContent()) ?? "";
check(/Void Line/.test(drawerText), "Void Line is in More Options");
check(!/Hold \/ Resume/.test(drawerText), "Hold is no longer duplicated in More Options");
/* The drawer names the line it would remove. A destructive action reached
   through a menu has lost the "last scanned" strip that guarded it on the rail,
   so it has to carry that context itself. */
check(/Nadec|Lipton|Almarai|Barbican/.test(drawerText),
  "the Void entry names the line it would remove",
  (drawerText.match(/Void Line([^A-Z]*[A-Za-z0-9 ]+)/) ?? ["?"])[0].slice(0, 48));
await p.screenshot({ path: "qa/layout/till-more-drawer.png" });

await p.locator(".till-more-item", { hasText: "Void Line" }).click();
await p.waitForTimeout(1200);
check(await rowCount() === before - 1, "Void removes the selected line",
  `${before} -> ${await rowCount()}`);

await p.screenshot({ path: "qa/layout/till-rail.png" });
await b.close();
console.log(failures.length ? `\n${failures.length} FAILED` : "\nall checks passed");
process.exit(failures.length ? 1 : 0);
