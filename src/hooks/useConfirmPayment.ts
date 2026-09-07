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
import type { PaymentCompletionOptions } from "../components/PaymentModal";

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
  printSaleNow: (sale: SaleResult, isReprint: boolean, trigger: "auto" | "manual") => Promise<boolean>;
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
    completionOptions?: PaymentCompletionOptions,
  ) => {
    if (confirmingRef.current) return;
    confirmingRef.current = true;
    try {
      const allPayments = buildExchangePayments(exchangeCredit, payments, netTotal);
      const result = await finalizeSale(allPayments, customerId, deliveryInput, cart);
      onCommitted(result);
      if (payments.some(payment => payment.method === "cash")) {
        openCashDrawer(sessionUser.session_token)
          .catch((error: unknown) => console.warn("Cash drawer open failed:", error));
      }
      if (completionOptions?.printReceipt) {
        void printSaleNow(result, false, "manual");
      } else if (!completionOptions) {
        // Compatibility for callers that do not yet expose a per-sale choice.
        void printSaleNow(result, false, "auto");
      }
      onReady(result);
      dispatchPostSaleWhatsApp({
        result,
        deliveryInput,
        selectedCustomer,
        checkoutContactNumber: completionOptions?.whatsappNumber,
        rider: completionOptions?.rider,
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
