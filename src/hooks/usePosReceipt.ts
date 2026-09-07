import { useCallback } from "react";
import type { BusinessFlags, SaleResult } from "../types";
import {
  businessFlagsLoad,
  printReceiptRaw,
  settingsGetBranch,
  thermalGetConfig,
} from "../tauri/commands";
import { performReceiptPrint, type ReceiptPrintTrigger } from "../utils/receiptLines";
import type { SessionToken } from "../types";

export type ReceiptStatus = "ready" | "printing" | "printed" | "failed";

/**
 * Receipt printing for the till.
 *
 * Config is re-read on every print rather than cached: an operator who fixes a
 * disabled printer in Settings mid-shift expects the next receipt to work,
 * without restarting the app.
 *
 * A print failure never fails the sale. The sale is already committed by the
 * time this runs, so every path here reports and recovers — the historical
 * failure mode in this product was a receipt that silently did not print, so
 * the outcome is always surfaced rather than swallowed.
 */
export function usePosReceipt(options: {
  sessionToken: SessionToken;
  setBizFlags: (flags: BusinessFlags) => void;
  setReceiptStatus: (status: ReceiptStatus) => void;
  setError: (message: string) => void;
  focusBarcode: () => void;
}) {
  const { sessionToken, setBizFlags, setReceiptStatus, setError, focusBarcode } = options;

  const refreshPrintConfig = useCallback(async () => {
    const [flagsNow, settingsNow, thermalNow] = await Promise.all([
      businessFlagsLoad(),
      settingsGetBranch(sessionToken),
      thermalGetConfig(sessionToken),
    ]);
    setBizFlags(flagsNow);
    return { flags: flagsNow, settings: settingsNow, thermalEnabled: thermalNow.enabled };
  }, [sessionToken, setBizFlags]);

  const printSaleNow = useCallback(async (
    sale: SaleResult,
    isReprint: boolean,
    trigger: ReceiptPrintTrigger = "manual",
  ): Promise<boolean> => {
    try {
      const config = await refreshPrintConfig();
      const intendsToPrint = trigger === "manual"
        || (config.flags.auto_print_receipt && config.thermalEnabled);
      if (intendsToPrint) setReceiptStatus("printing");

      const outcome = await performReceiptPrint({
        sale,
        settings: config.settings,
        trigger,
        autoPrintEnabled: config.flags.auto_print_receipt,
        thermalEnabled: config.thermalEnabled,
        isReprint,
        print: (storeName, lines) => printReceiptRaw(sessionToken, storeName, lines),
      });

      if (outcome === "printed") {
        setReceiptStatus("printed");
        focusBarcode();
        return true;
      }
      setReceiptStatus("ready");
      if (outcome === "unavailable" && (trigger === "manual" || config.flags.auto_print_receipt)) {
        setError("Thermal printer is disabled. Open Settings → System Control → Printing.");
      }
      return false;
    } catch (e: unknown) {
      console.warn("Receipt print failed:", e);
      setReceiptStatus("failed");
      setError(typeof e === "string"
        ? `Sale saved, but printing failed: ${e}`
        : "Sale saved, but the printer did not respond — check power/cable, then press Print.");
      return false;
    }
  }, [focusBarcode, refreshPrintConfig, sessionToken, setError, setReceiptStatus]);

  return { refreshPrintConfig, printSaleNow } as const;
}
