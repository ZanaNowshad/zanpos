import { useCallback, useMemo } from "react";
import type { AiHandoff, SaleResult } from "../types";
import type { PosRecoveryAction } from "../components/PosRecoveryBanner";
import type { ReceiptConfidenceStatus } from "../utils/posConfidence";
import type { useSyncStatus } from "./useSyncStatus";
import { syncTriggerNow } from "../tauri/commands";

interface Options {
  error: string | null;
  bannerResult: SaleResult | null;
  receiptStatus: ReceiptConfidenceStatus;
  syncStatus: ReturnType<typeof useSyncStatus>;
  lineCount: number;
  canOpenBackOffice: boolean;
  userId: string;
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
  userId, printSaleNow, setReceiptStatus, setError, clearError, focusBarcode,
  openHold, openSyncDetails, openWhatsAppQr, onAskOfficeAI, onOpenOfficeAI,
}: Options) {
  const actions = useMemo<PosRecoveryAction[]>(() => {
    if (!error) return [];
    const message = error.toLowerCase();
    const next: PosRecoveryAction[] = [];
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
  }, [bannerResult, canOpenBackOffice, error, lineCount, receiptStatus, syncStatus]);

  const handleAction = useCallback((key: string) => {
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
      void syncTriggerNow(userId).catch((cause: unknown) => {
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
    bannerResult, clearError, focusBarcode, onAskOfficeAI, onOpenOfficeAI,
    openHold, openSyncDetails, openWhatsAppQr, printSaleNow, setError,
    setReceiptStatus, userId,
  ]);

  return { recoveryActions: actions, handleRecoveryAction: handleAction } as const;
}
