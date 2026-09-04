import { useCallback, useMemo } from "react";
import type { AiHandoff, Cart, SaleResult } from "../types";
import type { PosRecoveryAction } from "../components/PosRecoveryBanner";
import type { ReceiptConfidenceStatus } from "../utils/posConfidence";
import type { useSyncStatus } from "./useSyncStatus";
import { posRepriceCart, syncTriggerNow } from "../tauri/commands";

/** The checkout refusal that a reprice can actually clear. Matched on the
 *  message because that is what the till has — the backend returns a validation
 *  error, not a typed code. Kept next to the string it must track. */
export function isPriceChangeRefusal(error: string | null): boolean {
  return (error ?? "").toLowerCase().includes("price changed");
}

interface Options {
  error: string | null;
  bannerResult: SaleResult | null;
  receiptStatus: ReceiptConfidenceStatus;
  syncStatus: ReturnType<typeof useSyncStatus>;
  lineCount: number;
  canOpenBackOffice: boolean;
  sessionToken: string;
  cart: Cart | null;
  setCart: (cart: Cart) => void;
  printSaleNow: (result: SaleResult, reprint: boolean) => Promise<unknown>;
  setReceiptStatus: (status: ReceiptConfidenceStatus) => void;
  setError: (error: string) => void;
  clearError: () => void;
  focusBarcode: () => void;
  openHold: () => void;
  openSyncDetails: () => void;
  openWhatsAppQr: () => void;
  onAskOfficeAI?: (handoff: AiHandoff) => void;
  onOpenOfficeAI?: () => void;
}

export function usePosRecoveryActions({
  error, bannerResult, receiptStatus, syncStatus, lineCount, canOpenBackOffice,
  sessionToken, cart, setCart, printSaleNow, setReceiptStatus, setError, clearError,
  focusBarcode, openHold, openSyncDetails, openWhatsAppQr, onAskOfficeAI,
  onOpenOfficeAI,
}: Options) {
  const actions = useMemo<PosRecoveryAction[]>(() => {
    if (!error) return [];
    const message = error.toLowerCase();
    const next: PosRecoveryAction[] = [];
    /* First, because it is the only one that resolves this refusal. The
       alternative the message used to offer was to remove the line and scan it
       again — fine for one item, miserable for a full basket, and busywork
       either way since the till already knows both prices. */
    if (isPriceChangeRefusal(error) && cart) {
      next.push({ key: "reprice-cart", label: "Update prices" });
    }
    if (bannerResult
      && (receiptStatus === "failed" || message.includes("print") || message.includes("printer"))) {
      next.push({ key: "retry-print", label: "Retry print" });
    }
    if (message.includes("printer") || message.includes("print")) {
      next.push({ key: "printer-settings", label: "Open printer settings" });
    }
    if ((syncStatus?.pending_events ?? 0) > 0
      || syncStatus?.last_error
      || message.includes("sync")
      || message.includes("offline")) {
      next.push({ key: "retry-sync", label: "Retry sync" });
    }
    if (lineCount > 0) next.push({ key: "hold-cart", label: "Hold cart" });
    if (canOpenBackOffice
      && (message.includes("whatsapp") || message.includes("sidecar") || message.includes("qr"))) {
      next.push({ key: "open-whatsapp-qr", label: "Open WhatsApp QR" });
    }
    next.push({ key: "focus-scan", label: "Focus scan" });
    return next;
  }, [bannerResult, canOpenBackOffice, cart, error, lineCount, receiptStatus, syncStatus]);

  const handleAction = useCallback((key: string) => {
    if (key === "reprice-cart" && cart) {
      void posRepriceCart(cart)
        .then(result => {
          setCart(result.cart);
          if (result.changed.length === 0) {
            // Nothing moved, so the refusal was not a price change after all.
            // Saying so beats clearing the banner and leaving the cashier to
            // press Pay again into the same wall.
            setError("Prices are already current. Try taking payment again.");
            return;
          }
          clearError();
        })
        .catch((cause: unknown) => {
          setError(typeof cause === "string" ? cause : "Could not update prices.");
        });
      return;
    }
    if (key === "retry-print" && bannerResult) {
      void printSaleNow(bannerResult, false).catch((cause: unknown) => {
        setReceiptStatus("failed");
        setError(typeof cause === "string" ? cause : "Print failed.");
      });
      return;
    }
    if (key === "printer-settings") {
      clearError();
      if (onAskOfficeAI) {
        onAskOfficeAI({ text: "Open printer settings and check the thermal printer configuration." });
      } else {
        onOpenOfficeAI?.();
      }
      return;
    }
    if (key === "retry-sync") {
      void syncTriggerNow(sessionToken).catch((cause: unknown) => {
        setError(typeof cause === "string" ? cause : "Retry sync failed.");
      });
      openSyncDetails();
      return;
    }
    if (key === "hold-cart") {
      openHold();
      return;
    }
    if (key === "open-whatsapp-qr") {
      openWhatsAppQr();
      return;
    }
    clearError();
    focusBarcode();
  }, [
    bannerResult, cart, clearError, focusBarcode, onAskOfficeAI, onOpenOfficeAI,
    openHold, openSyncDetails, openWhatsAppQr, printSaleNow, setCart, setError,
    setReceiptStatus, sessionToken,
  ]);

  return { recoveryActions: actions, handleRecoveryAction: handleAction } as const;
}
