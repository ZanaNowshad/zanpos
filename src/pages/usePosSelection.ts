import { useEffect } from "react";
import type { Cart, CartLine } from "../types";
import { usePosNumpad } from "../hooks/usePosNumpad";

/**
 * Which cart line the till is currently acting on.
 *
 * Two lines compete for that role: the one the cashier tapped, and the one the
 * scanner just added. The scan wins — attention is on the item that landed,
 * not a row inspected three items ago — which is why the tapped line is
 * cleared whenever a new scan arrives.
 *
 * The quantity pad is keyed to the same selection rather than to the scanned
 * line, so it also works on a row the cashier reached for further up the cart.
 */
export function usePosSelection(
  cart: Cart,
  pickedLineId: string | null,
  setPickedLineId: (id: string | null) => void,
  recentLineId: string | null,
  qtyPadLineId: string | null,
  updateQuantity: (cartLineId: string, quantity: string) => Promise<void>,
) {
  const selectedLineId = pickedLineId ?? recentLineId;

  const liveLine = (id: string | null): CartLine | null =>
    cart.lines.find(line => line.cart_line_id === id && !line.voided) ?? null;

  const selectedLine = liveLine(selectedLineId);
  const qtyPadLine = liveLine(qtyPadLineId);

  const { numpadValue, setNumpadValue, handleNumpadKey } =
    usePosNumpad(selectedLineId, updateQuantity);

  useEffect(() => { setPickedLineId(null); }, [recentLineId, setPickedLineId]);

  return { selectedLineId, selectedLine, qtyPadLine, numpadValue, setNumpadValue, handleNumpadKey };
}
