import { useCallback, useState } from "react";
import type { Cart, PaymentInput, ProductWithPrice, SaleResult } from "../types";
import * as cmd from "../tauri/commands";

export interface CartSession {
  branch_id: string;
  device_id: string;
  shift_id: string;
  cashier_user_id: string;
}

function makeEmptyCart(session: CartSession): Cart {
  return {
    cart_id: crypto.randomUUID(),
    branch_id: session.branch_id,
    device_id: session.device_id,
    shift_id: session.shift_id,
    cashier_user_id: session.cashier_user_id,
    lines: [],
    bill_discount_minor: 0,
  };
}

export function useCart(session: CartSession) {
  const [cart, setCart] = useState<Cart>(() => makeEmptyCart(session));
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const clearError = () => setError(null);

  const addByBarcode = useCallback(async (barcode: string) => {
    setLoading(true);
    setError(null);
    try {
      const updated = await cmd.posAddItemByBarcode(cart, barcode);
      setCart(updated);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Barcode not found");
    } finally {
      setLoading(false);
    }
  }, [cart]);

  const addProduct = useCallback(async (product: ProductWithPrice) => {
    setLoading(true);
    setError(null);
    try {
      const updated = await cmd.posAddItem(cart, product.product_id);
      setCart(updated);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to add item");
    } finally {
      setLoading(false);
    }
  }, [cart]);

  const updateQuantity = useCallback(async (cart_line_id: string, quantity: string) => {
    try {
      const updated = await cmd.posUpdateQuantity(cart, cart_line_id, quantity);
      setCart(updated);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Invalid quantity");
    }
  }, [cart]);

  const removeLine = useCallback(async (cart_line_id: string) => {
    try {
      const updated = await cmd.posRemoveLine(cart, cart_line_id);
      setCart(updated);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to remove item");
    }
  }, [cart]);

  const applyBillDiscount = useCallback(async (discount_minor: number) => {
    try {
      const updated = await cmd.posApplyBillDiscount(cart, discount_minor);
      setCart(updated);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to apply discount");
    }
  }, [cart]);

  const applyLineDiscount = useCallback(async (cart_line_id: string, discount_minor: number) => {
    try {
      const updated = await cmd.posApplyLineDiscount(cart, cart_line_id, discount_minor);
      setCart(updated);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to apply line discount");
    }
  }, [cart]);

  const addCustomItem = useCallback(async (name: string, priceMajor: string, quantity: string) => {
    setLoading(true);
    setError(null);
    try {
      const priceMinor = Math.round(parseFloat(priceMajor) * 1000); // BHD has 3 decimal places
      const updated = await cmd.posAddCustomItem(cart, name, priceMinor, quantity);
      setCart(updated);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to add custom item");
    } finally {
      setLoading(false);
    }
  }, [cart]);

  const setLineNote = useCallback(async (cart_line_id: string, note: string | null) => {
    try {
      const updated = await cmd.posSetLineNote(cart, cart_line_id, note);
      setCart(updated);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to set note");
    }
  }, [cart]);

  const finalizeSale = useCallback(async (payments: PaymentInput[]): Promise<SaleResult> => {
    setLoading(true);
    setError(null);
    try {
      const result = await cmd.posFinalizeSale(cart, payments);
      setCart(makeEmptyCart(session));
      return result;
    } catch (e: unknown) {
      const msg = typeof e === "string" ? e : "Sale failed";
      setError(msg);
      throw new Error(msg);
    } finally {
      setLoading(false);
    }
  }, [cart, session]);

  const clearCart = useCallback(() => setCart(makeEmptyCart(session)), [session]);

  const replaceCart = useCallback((newCart: Cart) => setCart(newCart), []);

  const netTotal = Math.max(
    0,
    cart.lines.filter(l => !l.voided).reduce((s, l) => s + l.line_total_minor, 0) - cart.bill_discount_minor
  );
  const taxTotal = cart.lines.filter(l => !l.voided).reduce((s, l) => s + l.tax_amount_minor, 0);
  const lineCount = cart.lines.filter(l => !l.voided).length;

  return {
    cart,
    loading,
    error,
    clearError,
    addByBarcode,
    addProduct,
    addCustomItem,
    updateQuantity,
    removeLine,
    applyBillDiscount,
    applyLineDiscount,
    setLineNote,
    finalizeSale,
    clearCart,
    replaceCart,
    netTotal,
    taxTotal,
    lineCount,
  };
}
