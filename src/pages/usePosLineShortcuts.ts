import { useMemo } from "react";
import type { CartLine } from "../types";
import type { ActiveModal } from "../components/pos/posModalState";

interface Options {
  selectedLine: CartLine | null;
  lineCount: number;
  setActiveModal: (modal: ActiveModal) => void;
  setQtyPadLineId: (id: string | null) => void;
  setShowMore: (open: boolean) => void;
  removeLine: (cartLineId: string) => void | Promise<void>;
  focusBarcode: () => void;
  openPaymentJourney: (journey: "receipt" | "delivery" | "digital") => void;
}

/**
 * What a keyboard shortcut does to the line the cashier has selected.
 *
 * Grouped because they share one precondition — there has to be a live
 * selected line — and because every one of them is a no-op rather than an
 * error when there is not. A shortcut that throws mid-sale is worse than a
 * shortcut that does nothing.
 */
export function usePosLineShortcuts({
  selectedLine, lineCount, setActiveModal, setQtyPadLineId, setShowMore,
  removeLine, focusBarcode, openPaymentJourney,
}: Options) {
  return useMemo(() => ({
    onLineQty: () => { if (selectedLine) setQtyPadLineId(selectedLine.cart_line_id); },
    onLinePrice: () => {
      if (!selectedLine) return;
      setActiveModal({
        kind: "priceInput",
        mode: "setExisting",
        lineId: selectedLine.cart_line_id,
        productName: selectedLine.product_name,
        currentPriceMinor: selectedLine.unit_price_minor,
      });
    },
    onVoidLine: () => {
      if (!selectedLine) return;
      void removeLine(selectedLine.cart_line_id);
      focusBarcode();
    },
    onMoreOptions: () => setShowMore(true),
    // Nothing to pay for on an empty cart, so the key is inert rather than
    // opening a payment sheet for zero.
    onJourney: (journey: "receipt" | "delivery" | "digital") => {
      if (lineCount > 0) openPaymentJourney(journey);
    },
  }), [
    selectedLine, lineCount, setActiveModal, setQtyPadLineId, setShowMore,
    removeLine, focusBarcode, openPaymentJourney,
  ]);
}
