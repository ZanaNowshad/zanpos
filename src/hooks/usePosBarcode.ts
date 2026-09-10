import { useCallback, useEffect, useRef } from "react";
import type { BarcodeInputHandle } from "../components/BarcodeInput";
import type { useCart } from "./useCart";
import { ghostRecord } from "../tauri/commands";
import type { SessionToken } from "../types";

interface Options {
  sessionToken: SessionToken;
  numpadValue: string;
  addByBarcode: ReturnType<typeof useCart>["addByBarcode"];
  setError: ReturnType<typeof useCart>["setError"];
  resetNumpad: () => void;
  refreshNotifications: () => void;
}

export function usePosBarcode({
  sessionToken,
  numpadValue,
  addByBarcode,
  setError,
  resetNumpad,
  refreshNotifications,
}: Options) {
  const barcodeRef = useRef<BarcodeInputHandle>(null);
  const sessionTokenRef = useRef(sessionToken);
  const numpadRef = useRef(numpadValue);
  const scanBufferRef = useRef<Array<{ barcode: string; qty: number }>>([]);
  const scanDrainingRef = useRef(false);

  useEffect(() => { sessionTokenRef.current = sessionToken; }, [sessionToken]);
  useEffect(() => { numpadRef.current = numpadValue; }, [numpadValue]);

  const drainScanBuffer = useCallback(async () => {
    if (scanDrainingRef.current) return;
    scanDrainingRef.current = true;
    while (scanBufferRef.current.length > 0) {
      const item = scanBufferRef.current.shift()!;
      try {
        await addByBarcode(item.barcode, item.qty);
        resetNumpad();
      } catch {
        barcodeRef.current?.flashError();
        void ghostRecord(sessionTokenRef.current, item.barcode).then(refreshNotifications);
        setError(`Barcode "${item.barcode}" not found — flagged for your manager. Keep selling.`);
      }
    }
    scanDrainingRef.current = false;
    barcodeRef.current?.focus();
  }, [addByBarcode, refreshNotifications, resetNumpad, setError]);

  const handleBarcode = useCallback((barcode: string, qty?: number) => {
    const parsedNumpad = parseInt(numpadRef.current);
    const effectiveQty = qty
      ?? (Number.isFinite(parsedNumpad) && parsedNumpad > 0 ? parsedNumpad : 1);
    barcodeRef.current?.flashSuccess();
    scanBufferRef.current.push({ barcode, qty: effectiveQty });
    void drainScanBuffer();
  }, [drainScanBuffer]);

  const focusBarcode = useCallback(() => barcodeRef.current?.focus(), []);

  return { barcodeRef, focusBarcode, handleBarcode } as const;
}
