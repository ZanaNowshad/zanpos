import { useCallback } from "react";
import type {
  Cart,
  CustomerRow,
  DeliveryInput,
  PaymentInput,
  SaleResult,
  SessionUser,
} from "../types";
import { openCashDrawer } from "../tauri/commands";
import type { ExchangeCredit } from "../components/pos/posModalState";
import { buildExchangePayments } from "../utils/posExchange";
import { dispatchPostSaleWhatsApp } from "../utils/postSaleWhatsApp";

interface Options {
  confirmingRef: { current: boolean };
  cart: Cart;
  exchangeCredit: ExchangeCredit | null;
  netTotal: number;
  sessionUser: SessionUser;
  finalizeSale: (
    payments: PaymentInput[],
    customerId?: string,
    deliveryInput?: DeliveryInput,
    cartOverride?: Cart,
  ) => Promise<SaleResult>;
  printSaleNow: (sale: SaleResult, isReprint: boolean, trigger: "auto") => Promise<boolean>;
  onCommitted: (result: SaleResult) => void;
  onReady: (result: SaleResult) => void;
  onPairingRequired: () => void;
}

export function useConfirmPayment({
  confirmingRef,
  cart,
  exchangeCredit,
  netTotal,
  sessionUser,
  finalizeSale,
  printSaleNow,
  onCommitted,
  onReady,
  onPairingRequired,
}: Options) {
  return useCallback(async (
    payments: PaymentInput[],
    customerId?: string,
    deliveryInput?: DeliveryInput,
    selectedCustomer?: CustomerRow,
  ) => {
    if (confirmingRef.current) return;
    confirmingRef.current = true;
    try {
      const allPayments = buildExchangePayments(exchangeCredit, payments, netTotal);
      const result = await finalizeSale(allPayments, customerId, deliveryInput, cart);
      onCommitted(result);
      if (payments.some(payment => payment.method === "cash")) {
        openCashDrawer(sessionUser.user_id)
          .catch((error: unknown) => console.warn("Cash drawer open failed:", error));
      }
      void printSaleNow(result, false, "auto");
      onReady(result);
      dispatchPostSaleWhatsApp({
        result,
        deliveryInput,
        selectedCustomer,
        sessionUser,
        onPairingRequired,
      });
    } catch {
      // useCart owns the error; the payment modal and cart remain intact.
    } finally {
      confirmingRef.current = false;
    }
  }, [
    cart,
    confirmingRef,
    exchangeCredit,
    finalizeSale,
    netTotal,
    onCommitted,
    onPairingRequired,
    onReady,
    printSaleNow,
    sessionUser,
  ]);
}
