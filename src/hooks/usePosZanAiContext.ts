import { useCallback } from "react";
import type { Cart, SessionUser, Shift, SyncStatus } from "../types";
import { DEVICE } from "../types";
import { buildPosAiContext } from "../zanai/posContext";

/**
 * The till snapshot handed to ZanAI, and whether the widget should be showing
 * at all.
 *
 * `suppressed` is the safety half: the assistant is hidden outright while a
 * modal is open, while Fast Cash is settling, and while a completed sale is on
 * screen. Those are the moments a cashier is committing money, and a floating
 * panel over them is how a wrong button gets pressed.
 */
export function usePosZanAiContext(input: {
  cart: Cart;
  shift: Shift;
  sessionUser: SessionUser;
  netTotal: number;
  taxTotal: number;
  syncStatus: SyncStatus | null;
  modalOpen: boolean;
  payFastLoading: boolean;
  showSaleDetails: boolean;
}) {
  const { cart, shift, sessionUser, netTotal, taxTotal, syncStatus } = input;

  const buildZanAiContext = useCallback((capturedAt: string) => buildPosAiContext({
    cart,
    shift,
    user: sessionUser,
    branchName: DEVICE.branch_name,
    deviceId: DEVICE.device_id,
    netTotalMinor: netTotal,
    taxTotalMinor: taxTotal,
    syncStatus,
    capturedAt,
  }), [cart, netTotal, sessionUser, shift, syncStatus, taxTotal]);

  return {
    buildZanAiContext,
    zanAiSuppressed: input.modalOpen || input.payFastLoading || input.showSaleDetails,
  };
}
