import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";

/**
 * The till was rearranged; nothing was allowed to fall out of it.
 *
 * A layout change is exactly the kind of edit that silently drops a button
 * nobody uses daily — a cash drop, a reprint, an exchange cancel — and the loss
 * only surfaces weeks later when a cashier needs it mid-queue. This pins the
 * full inventory taken from the previous layout so a missing action fails the
 * build instead of the shift.
 *
 * It reads source rather than rendering, because these actions live across a
 * dozen components behind role and feature flags; asserting on the wiring is
 * both cheaper and harder to fool than driving every branch of the UI.
 */

const POS_DIR = path.resolve(__dirname, "../components/pos");
const SOURCES = [
  /* Every file the till page is made of, not just PosPage.tsx.
     PosPage was split into a state hook and a view, and this test immediately
     reported removeLine, bumpLine and openPaymentJourney as "dropped from the
     till" when they had only moved one file across. A capability that is gone
     is a real defect; a capability that relocated is not, and reading the
     directory keeps the two from looking alike. */
  ...readdirSync(path.resolve(__dirname, "../pages"))
    .filter(f => /^(PosPage|usePosPage|usePos|usePractice)/.test(f) && /\.tsx?$/.test(f))
    .map(f => path.resolve(__dirname, "../pages", f)),
  path.resolve(__dirname, "../hooks/usePosShortcuts.ts"),
  path.resolve(__dirname, "../hooks/usePosShortcutBindings.ts"),
  path.resolve(__dirname, "../hooks/usePosOverlays.ts"),
  ...readdirSync(POS_DIR)
    .filter(f => /\.(tsx?|ts)$/.test(f))
    .map(f => path.join(POS_DIR, f)),
].map(p => readFileSync(p, "utf8")).join("\n");

/** Every operator-reachable action the till had before the rearrangement. */
const ACTIONS: Record<string, string[]> = {
  "sale loop": [
    "onBarcode",          // scan / manual entry
    "addProduct",
    "addCustomItem",      // custom item
    "removeLine",         // void a line
    "bumpLine",           // qty +/-
    "setLinePrice",       // price override
  ],
  tender: [
    "onPayFast",          // Fast Cash
    "openPaymentJourney", // Receipt / Delivery / Digital
    "onPaySplit",         // split across methods
    /* Not listed: `onPayDirect`. Its cash/card/wallet buttons sat inside
       CartPanel's `!compact` branch and the till has always rendered that panel
       compact, so they were unreachable before this rearrangement too. Method
       choice happens inside the payment modal. */
  ],
  exchange: [
    "exchangeCredit",
    "onCancelExchange",
    "onCompleteCoveredExchange",
  ],
  "sale operations": [
    "onClearCart",
    /* Hold and Void traded places: Hold came out of the More drawer onto the
       rail under the cart, Void went the other way. The prop names moved with
       them, so this pins the handler the till actually calls rather than the
       drawer prop it used to be passed as — `handleOpenHold` is the one thing
       both the rail and the F1 shortcut go through. */
    "handleOpenHold",     // hold / resume
    "onVoidLine",         // void, now behind More Options
    "onOpenDiscount",     // bill discount
    "applyLineDiscount",  // line discount
    "onOpenRefund",
    "onOpenCashEvent",    // cash in / out / safe drop
    "onOpenDeliveries",
    "onOpenRecent",
    "onReprintLast",
    "onNoSale",
  ],
  "shift + session": [
    "onCloseShift",
    "onLogout",
    "onOpenReport",       // X report
  ],
  "back office": [
    "onOpenOfficeAI",     // Admin
    "onOpenNotes",
    "onOpenOrders",       // WhatsApp orders
    "onOpenNotifications",// alerts
  ],
  chrome: [
    "onToggleSidebar",
    "onToggleTheme",
    "onToggleLanguage",
    "onOpenSyncDetails",
    "QuranToggle",
    "WhatsAppPill",
    "SyncChip",
  ],
  overlays: [
    "showWaQR",
    "showSyncDetails",
    "showNotes",
    "showNotifications",
    "showOrders",
    "showDeliveries",
  ],
  modals: [
    '"payment"', '"shiftClose"', '"hold"', '"refund"', '"report"',
    '"discount"', '"customItem"', '"cashEvent"', '"clearConfirm"',
    '"help"', '"recent"', '"priceInput"',
  ],
};

describe("till inventory survives the rearrangement", () => {
  for (const [group, names] of Object.entries(ACTIONS)) {
    it(`keeps every ${group} action reachable`, () => {
      const missing = names.filter(name => !SOURCES.includes(name));
      expect(missing, `dropped from the till: ${missing.join(", ")}`).toEqual([]);
    });
  }

  /* Shortcuts are muscle memory. Remapping one is a retraining cost, so each
     binding is pinned to the key it fires on — a change has to be deliberate. */
  it("keeps every keyboard shortcut bound", () => {
    const shortcuts = readFileSync(
      path.resolve(__dirname, "../hooks/usePosShortcuts.ts"), "utf8",
    );
    for (const key of ["F2", "F6", "F7", "F8", "F9", "F10", "F11", "F12"]) {
      expect(shortcuts, `${key} lost its binding`).toContain(`"${key}"`);
    }
    for (const combo of ["d", "r", "h", "l", "n", "p", "x"]) {
      expect(shortcuts, `Ctrl+${combo} lost its binding`).toContain(`case "${combo}"`);
    }
  });
});
