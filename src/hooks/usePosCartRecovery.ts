import { useCallback } from "react";
import type { Cart } from "../types";
import type { useCart } from "./useCart";
import { useCartRecovery } from "./useCartRecovery";
import { DEVICE } from "../types";

interface Options {
  cart: Cart;
  trainingMode: boolean;
  shiftId: string;
  userId: string;
  replaceCart: ReturnType<typeof useCart>["replaceCart"];
  focusBarcode: () => void;
}

export function usePosCartRecovery({
  cart,
  trainingMode,
  shiftId,
  userId,
  replaceCart,
  focusBarcode,
}: Options) {
  const { recoverable, dismiss } = useCartRecovery(cart, {
    enabled: !trainingMode,
  });

  const recoverCart = useCallback(() => {
    if (!recoverable) return;
    replaceCart({
      ...recoverable.cart,
      shift_id: shiftId,
      cashier_user_id: userId,
      device_id: DEVICE.device_id,
    });
    dismiss();
    focusBarcode();
  }, [dismiss, focusBarcode, recoverable, replaceCart, shiftId, userId]);

  return { recoverable, dismissRecovery: dismiss, recoverCart } as const;
}
