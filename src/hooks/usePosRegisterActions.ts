import { useCallback } from "react";
import type { Dispatch, SetStateAction } from "react";
import type { SaleListRow, Shift } from "../types";
import type { ActiveModal } from "../components/pos/posModalState";
import type { ReceiptConfidenceStatus } from "../utils/posConfidence";
import type { useCart } from "./useCart";
import type { usePosReceipt } from "./usePosReceipt";
import {
  cashNoSale,
  openCashDrawer,
  posLoadSaleForEdit,
  receiptReprint,
} from "../tauri/commands";
import { DEVICE } from "../types";

interface Options {
  shift: Shift;
  userId: string;
  lineCount: number;
  lastReceiptNumber: string | null;
  printSaleNow: ReturnType<typeof usePosReceipt>["printSaleNow"];
  clearCart: ReturnType<typeof useCart>["clearCart"];
  replaceCart: ReturnType<typeof useCart>["replaceCart"];
  setError: ReturnType<typeof useCart>["setError"];
  setReceiptStatus: Dispatch<SetStateAction<ReceiptConfidenceStatus>>;
  setLastReceiptNumber: Dispatch<SetStateAction<string | null>>;
  setActiveModal: Dispatch<SetStateAction<ActiveModal>>;
  focusBarcode: () => void;
}

export function usePosRegisterActions({
  shift, userId, lineCount, lastReceiptNumber, printSaleNow, clearCart,
  replaceCart, setError, setReceiptStatus, setLastReceiptNumber,
  setActiveModal, focusBarcode,
}: Options) {
  const reprintReceiptNow = useCallback(async (receiptNumber: string) => {
    try {
      const reprinted = await receiptReprint(receiptNumber, userId);
      await printSaleNow(reprinted, true);
      setLastReceiptNumber(receiptNumber);
    } catch (cause: unknown) {
      setReceiptStatus("failed");
      setError(typeof cause === "string" ? cause : "Reprint failed — check receipt number");
    }
  }, [printSaleNow, setError, setLastReceiptNumber, setReceiptStatus, userId]);

  const handleReprintLast = useCallback(async () => {
    if (!lastReceiptNumber) return;
    await reprintReceiptNow(lastReceiptNumber);
  }, [lastReceiptNumber, reprintReceiptNow]);

  const handleNoSale = useCallback(async () => {
    try {
      await cashNoSale(shift.shift_id, userId);
    } catch (cause) {
      setError(typeof cause === "string" ? cause : "No-sale open failed");
      // Without this return the drawer opened anyway. A no-sale is the one
      // drawer-open with no sale behind it, so its `no_sale_events` row and
      // audit entry are the only record of who opened the till and why —
      // popping it after that write failed leaves cash accessible with nothing
      // recording it.
      return;
    }
    openCashDrawer(userId)
      .catch((cause: unknown) => console.warn("Cash drawer open failed (no-sale):", cause));
  }, [setError, shift.shift_id, userId]);

  const handleEditSale = useCallback(async (sale: SaleListRow) => {
    try {
      const restored = await posLoadSaleForEdit(
        sale.receipt_number,
        DEVICE.branch_id,
        shift.device_id,
        shift.shift_id,
        userId,
      );
      clearCart();
      replaceCart(restored);
      setActiveModal({ kind: "none" });
      focusBarcode();
    } catch (cause) {
      setError(typeof cause === "string" ? cause : "Failed to load sale for edit");
    }
  }, [
    clearCart, focusBarcode, replaceCart, setActiveModal, setError,
    shift.device_id, shift.shift_id, userId,
  ]);

  const handleClearCartRequest = useCallback(() => {
    if (lineCount === 0) return;
    setActiveModal({ kind: "clearConfirm" });
  }, [lineCount, setActiveModal]);

  return {
    reprintReceiptNow,
    handleReprintLast,
    handleNoSale,
    handleEditSale,
    handleClearCartRequest,
  } as const;
}
