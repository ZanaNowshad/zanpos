import { useEffect } from "react";

export interface PosShortcutHandlers {
  noModalOpen: boolean;
  lineCount: number;
  hasRecentLine: boolean;
  lastReceiptNumber: string | null;
  onFocusBarcode: () => void;
  onHold: () => void;
  onResumeHeld: () => void;
  onPay: () => void;
  onPayFast: () => void;
  onDiscount: () => void;
  onLineDiscount: () => void;
  onRefund: () => void;
  onClearCart: () => void;
  onReprintLast: () => void;
  onNoSale: () => void;
  onXReport?: () => void;
  onIncrementRecent: () => void;
  onDecrementRecent: () => void;
  onRemoveRecent: () => void;
  onLock: () => void;
  onReport: () => void;
  onCustomItem: () => void;
  onHelp?: () => void;
}

/**
 * Global POS keyboard shortcut manager.
 *
 * F2/F3       — focus barcode (handled in BarcodeInput; duplicated here for robustness)
 * F6          — hold cart
 * F7          — resume held cart
 * F8/Ctrl+D  — discount
 * F9          — pay (full modal)
 * F10/Ctrl+R — refund
 * Ctrl+P     — reprint last receipt
 * F11         — no-sale / cash drawer
 * F12         — Pay Fast (cash exact, no receipt)
 * /           — open Custom Item modal (when barcode field is empty)
 * Escape      — focus barcode / clear search
 * +/=         — increment recent item qty (when barcode field is empty)
 * -           — decrement recent item qty (when barcode field is empty)
 * Delete      — remove recent item (when not in a text field)
 * Ctrl+Backspace / Ctrl+Delete — clear entire cart (with confirm)
 * Ctrl+H      — help popup
 * Ctrl+R      — refund
 * Ctrl+L      — lock/logout
 */
export function usePosShortcuts(h: PosShortcutHandlers) {
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      const tag = target.tagName;
      const inBarcode = target.classList.contains("barcode-input");
      const inFreeText = (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") && !inBarcode;

      // ── F-keys always intercept (any field, any modal) ──────────────────────
      if (e.key === "F2" || e.key === "F3") {
        e.preventDefault();
        h.onFocusBarcode();
        return;
      }

      // ── F-keys that require no modal ─────────────────────────────────────────
      if (h.noModalOpen) {
        switch (e.key) {
          case "F6": e.preventDefault(); h.onHold(); return;
          case "F7": e.preventDefault(); h.onResumeHeld(); return;
          case "F8": e.preventDefault(); if (h.lineCount > 0) h.onDiscount(); return;
          case "F9": e.preventDefault(); if (h.lineCount > 0) h.onPay(); return;
          case "F10": e.preventDefault(); h.onRefund(); return;
          case "F11": e.preventDefault(); h.onNoSale(); return;
          case "F12": e.preventDefault(); if (h.lineCount > 0) h.onPayFast(); return;
          case "Escape": e.preventDefault(); h.onFocusBarcode(); return;
        }
      }

      // "/" opens the Custom Item modal — works from the barcode field (when empty)
      // since that's where the cursor normally rests, or anywhere outside a text input.
      if (h.noModalOpen && e.key === "/" && !inFreeText) {
        const barcodeHasText = inBarcode && (target as HTMLInputElement).value !== "";
        if (!barcodeHasText) {
          e.preventDefault();
          h.onCustomItem();
          return;
        }
      }

      // ── Skip remaining shortcuts when typing in non-barcode fields ───────────
      if (inFreeText) return;

      if (!h.noModalOpen) return;

      // ── Ctrl combos — checked BEFORE barcode early-return so they always fire ──
      if (e.ctrlKey) {
        switch (e.key.toLowerCase()) {
          case "d": e.preventDefault(); if (h.lineCount > 0 && h.hasRecentLine) h.onLineDiscount(); return;
          case "r": e.preventDefault(); h.onRefund(); return;
          case "h": e.preventDefault(); h.onHelp?.(); return;
          case "l": e.preventDefault(); h.onLock(); return;
          case "n": e.preventDefault(); h.onNoSale(); return;
          case "p": e.preventDefault(); if (h.lastReceiptNumber) h.onReprintLast(); return;
          case "x": e.preventDefault(); h.onXReport?.(); return;
          case "backspace":
          case "delete": e.preventDefault(); if (h.lineCount > 0) h.onClearCart(); return;
        }
      }

      // ── Recent-item quantity shortcuts (barcode field must be empty) ─────────
      if (inBarcode) {
        const barcodeEmpty = (target as HTMLInputElement).value === "";
        if (barcodeEmpty) {
          if (e.key === "+" || e.key === "=") {
            e.preventDefault();
            if (h.hasRecentLine) h.onIncrementRecent();
            return;
          }
          if (e.key === "-") {
            e.preventDefault();
            if (h.hasRecentLine) h.onDecrementRecent();
            return;
          }
          // NOTE: Backspace intentionally NOT bound here — it is too easy to
          // accidentally trigger on a tablet and remove the last scanned item.
        }
        return; // don't let barcode field trigger other shortcuts
      }

      // ── Shortcuts outside of any text field ─────────────────────────────────
      if (e.key === "+" || e.key === "=") {
        e.preventDefault();
        if (h.hasRecentLine) h.onIncrementRecent();
        return;
      }
      if (e.key === "-") {
        e.preventDefault();
        if (h.hasRecentLine) h.onDecrementRecent();
        return;
      }
      if (e.key === "Delete") {
        e.preventDefault();
        if (h.hasRecentLine) h.onRemoveRecent();
        return;
      }
      // NOTE: Backspace NOT bound — accidental removal risk on tablets.
    };

    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
    // L9: `h` is a plain object rebuilt on every PosPage render.
    // Listing it as the dep causes 20+ listeners to be re-registered on every
    // keystroke. Use a stable ref and read from it inside the handler instead.
  // The rationale is the L9 note above: `h` is rebuilt every render, so
  // depending on the object re-registers 20+ listeners per keystroke. The
  // individual fields actually read are listed instead.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    h.noModalOpen, h.lineCount, h.hasRecentLine, h.lastReceiptNumber,
    h.onFocusBarcode, h.onHold, h.onResumeHeld, h.onPay, h.onPayFast,
    h.onDiscount, h.onLineDiscount, h.onRefund, h.onClearCart, h.onReprintLast, h.onNoSale,
    h.onXReport, h.onIncrementRecent, h.onDecrementRecent, h.onRemoveRecent,
    h.onLock, h.onReport, h.onCustomItem, h.onHelp,
  ]);
}
