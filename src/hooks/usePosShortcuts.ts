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
  onRefund: () => void;
  onClearCart: () => void;
  onReprintLast: () => void;
  onNoSale: () => void;
  onIncrementRecent: () => void;
  onDecrementRecent: () => void;
  onRemoveRecent: () => void;
  onLock: () => void;
  onReport: () => void;
  onCustomItem: () => void;
}

/**
 * Global POS keyboard shortcut manager.
 *
 * F2/F3       — focus barcode (handled in BarcodeInput; duplicated here for robustness)
 * F6          — hold cart
 * F7          — resume held cart
 * F8/Ctrl+D  — discount
 * F9          — pay (full modal)
 * F10/Ctrl+P — reprint last receipt
 * F11         — no-sale / cash drawer
 * F12         — Pay Fast (cash exact, no receipt)
 * Escape      — focus barcode / clear search
 * +/=         — increment recent item qty (when barcode field is empty)
 * -           — decrement recent item qty (when barcode field is empty)
 * Delete      — remove recent item (when not in a text field)
 * Backspace   — remove recent item (only when barcode field is focused and empty)
 * Ctrl+Backspace / Ctrl+Delete — clear entire cart (with confirm)
 * Ctrl+H      — hold cart
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
          case "F10": e.preventDefault(); if (h.lastReceiptNumber) h.onReprintLast(); return;
          case "F11": e.preventDefault(); h.onNoSale(); return;
          case "F12": e.preventDefault(); if (h.lineCount > 0) h.onPayFast(); return;
          case "Escape": e.preventDefault(); h.onFocusBarcode(); return;
        }
      }

      // ── Skip remaining shortcuts when typing in non-barcode fields ───────────
      if (inFreeText) return;

      if (!h.noModalOpen) return;

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
          if (e.key === "Backspace") {
            e.preventDefault();
            if (h.hasRecentLine) h.onRemoveRecent();
            return;
          }
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
        if (e.ctrlKey) {
          e.preventDefault();
          if (h.lineCount > 0) h.onClearCart();
        } else {
          e.preventDefault();
          if (h.hasRecentLine) h.onRemoveRecent();
        }
        return;
      }
      if (e.key === "Backspace" && e.ctrlKey) {
        e.preventDefault();
        if (h.lineCount > 0) h.onClearCart();
        return;
      }

      // ── Ctrl combos ──────────────────────────────────────────────────────────
      if (e.ctrlKey) {
        switch (e.key.toLowerCase()) {
          case "d": e.preventDefault(); if (h.lineCount > 0) h.onDiscount(); return;
          case "r": e.preventDefault(); h.onRefund(); return;
          case "h": e.preventDefault(); h.onHold(); return;
          case "l": e.preventDefault(); h.onLock(); return;
          case "p": e.preventDefault(); if (h.lastReceiptNumber) h.onReprintLast(); return;
        }
      }
    };

    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [h]);
}
