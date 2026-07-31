import { useCallback, useEffect, useRef, useState } from "react";
import type { Dispatch, SetStateAction } from "react";
import type { Cart, PaymentInput, SaleResult, SessionUser } from "../types";
import type { ActiveModal, ExchangeCredit } from "../components/pos/posModalState";
import type { ReceiptConfidenceStatus } from "../utils/posConfidence";
import type { useCart } from "./useCart";
import type { usePosReceipt } from "./usePosReceipt";
import { useConfirmPayment } from "./useConfirmPayment";
import { buildExchangePayments } from "../utils/posExchange";
import { openCashDrawer } from "../tauri/commands";

interface ExchangeBalance {
  amountDueMinor: number;
  refundDueMinor: number;
}

interface Options {
  cart: Cart;
  exchangeCredit: ExchangeCredit | null;
  exchangeBalance: ExchangeBalance | null;
  netTotal: number;
  payableTotal: number;
  lineCount: number;
  noModalOpen: boolean;
  sessionUser: SessionUser;
  finalizeSale: ReturnType<typeof useCart>["finalizeSale"];
  printSaleNow: ReturnType<typeof usePosReceipt>["printSaleNow"];
  setActiveModal: Dispatch<SetStateAction<ActiveModal>>;
  setBannerResult: Dispatch<SetStateAction<SaleResult | null>>;
  setLastReceiptNumber: Dispatch<SetStateAction<string | null>>;
  setReceiptStatus: Dispatch<SetStateAction<ReceiptConfidenceStatus>>;
  setExchangeCredit: Dispatch<SetStateAction<ExchangeCredit | null>>;
  focusBarcode: () => void;
  openWhatsAppQr: () => void;
}

export function usePosPaymentActions({
  cart, exchangeCredit, exchangeBalance, netTotal, payableTotal, lineCount,
  noModalOpen, sessionUser, finalizeSale, printSaleNow, setActiveModal,
  setBannerResult, setLastReceiptNumber, setReceiptStatus, setExchangeCredit,
  focusBarcode, openWhatsAppQr,
}: Options) {
  const [payFastLoading, setPayFastLoading] = useState(false);
  const [restockAlerts, setRestockAlerts] = useState<SaleResult["low_stock_alerts"]>([]);
  const restockTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const confirmingRef = useRef(false);

  const showRestockAlerts = useCallback((result: SaleResult) => {
    if (result.low_stock_alerts.length === 0) return;
    if (restockTimerRef.current) clearTimeout(restockTimerRef.current);
    setRestockAlerts(result.low_stock_alerts);
    restockTimerRef.current = setTimeout(() => setRestockAlerts([]), 6000);
  }, []);

  const dismissRestockAlerts = useCallback(() => {
    if (restockTimerRef.current) clearTimeout(restockTimerRef.current);
    setRestockAlerts([]);
  }, []);

  useEffect(() => () => {
    if (restockTimerRef.current) clearTimeout(restockTimerRef.current);
  }, []);

  const handlePayFast = useCallback(async () => {
    if (lineCount === 0 || payFastLoading || !noModalOpen) return;
    setPayFastLoading(true);
    try {
      const payment: PaymentInput = {
        method: "cash",
        amount_minor: payableTotal,
        tendered_minor: payableTotal,
      };
      const payments = buildExchangePayments(
        exchangeCredit,
        payableTotal > 0 ? [payment] : [],
        netTotal,
      );
      const result = await finalizeSale(payments, undefined, undefined, cart);
      setBannerResult(result);
      setLastReceiptNumber(result.receipt_number);
      setReceiptStatus("ready");
      setExchangeCredit(null);
      openCashDrawer(sessionUser.user_id)
        .catch((cause: unknown) => console.warn("Cash drawer open failed:", cause));
      void printSaleNow(result, false, "auto");
      focusBarcode();
      showRestockAlerts(result);
    } catch {
      // useCart surfaces the failure and keeps the cart intact.
    } finally {
      setPayFastLoading(false);
    }
  }, [
    cart, exchangeCredit, finalizeSale, focusBarcode, lineCount, netTotal,
    noModalOpen, payFastLoading, payableTotal, printSaleNow, sessionUser.user_id,
    setBannerResult, setExchangeCredit, setLastReceiptNumber, setReceiptStatus,
    showRestockAlerts,
  ]);

  const openPayDirect = useCallback((method: PaymentInput["method"]) => {
    if (lineCount === 0 || payableTotal <= 0) return;
    setActiveModal({ kind: "payment", method, split: false });
  }, [lineCount, payableTotal, setActiveModal]);

  const openPaySplit = useCallback(() => {
    if (lineCount === 0 || payableTotal <= 0) return;
    setActiveModal({ kind: "payment", method: undefined, split: true });
  }, [lineCount, payableTotal, setActiveModal]);

  const openPay = useCallback(() => {
    if (lineCount === 0 || payableTotal <= 0) return;
    setActiveModal({ kind: "payment", method: undefined, split: false });
  }, [lineCount, payableTotal, setActiveModal]);

  const handleConfirmPayment = useConfirmPayment({
    confirmingRef,
    cart,
    exchangeCredit,
    netTotal,
    sessionUser,
    finalizeSale,
    printSaleNow,
    onCommitted: result => {
      setActiveModal({ kind: "none" });
      setBannerResult(result);
      setLastReceiptNumber(result.receipt_number);
      setReceiptStatus("ready");
      setExchangeCredit(null);
    },
    onReady: result => {
      focusBarcode();
      showRestockAlerts(result);
    },
    onPairingRequired: openWhatsAppQr,
  });

  const handleCompleteCoveredExchange = useCallback(async () => {
    if (!exchangeBalance
      || exchangeBalance.amountDueMinor > 0
      || lineCount === 0
      || confirmingRef.current) return;
    confirmingRef.current = true;
    try {
      const payments = buildExchangePayments(exchangeCredit, [], netTotal);
      const result = await finalizeSale(payments, undefined, undefined, cart);
      setBannerResult(result);
      setLastReceiptNumber(result.receipt_number);
      setReceiptStatus("ready");
      if (exchangeBalance.refundDueMinor > 0) {
        openCashDrawer(sessionUser.user_id)
          .catch((cause: unknown) => console.warn("Cash drawer open failed:", cause));
      }
      void printSaleNow(result, false, "auto");
      setExchangeCredit(null);
      focusBarcode();
    } catch {
      // useCart surfaces the failure and keeps the cart intact.
    } finally {
      confirmingRef.current = false;
    }
  }, [
    cart, exchangeBalance, exchangeCredit, finalizeSale, focusBarcode, lineCount,
    netTotal, printSaleNow, sessionUser.user_id, setBannerResult,
    setExchangeCredit, setLastReceiptNumber, setReceiptStatus,
  ]);

  return {
    payFastLoading,
    restockAlerts,
    dismissRestockAlerts,
    handlePayFast,
    openPayDirect,
    openPaySplit,
    openPay,
    handleConfirmPayment,
    handleCompleteCoveredExchange,
  } as const;
}
