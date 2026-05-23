import { useCallback, useState } from "react";
import type { Cart, CartLine, PaymentInput, ProductWithPrice, SaleResult } from "../types";
import { DEVICE } from "../types";
import * as cmd from "../tauri/commands";
import { posRecordVoid } from "../tauri/commands";

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
    bill_discount_reason: null,
  };
}

/** Find which line changed (new or qty-bumped) between two cart snapshots. */
function findChangedLineId(before: Cart, after: Cart): string | null {
  const beforeIds = new Set(before.lines.map(l => l.cart_line_id));
  // Prefer a brand-new line
  for (const line of after.lines) {
    if (!line.voided && !beforeIds.has(line.cart_line_id)) return line.cart_line_id;
  }
  // Fall back to a line whose quantity increased
  const beforeQty = new Map(before.lines.map(l => [l.cart_line_id, l.quantity]));
  for (const line of after.lines) {
    if (!line.voided && beforeQty.get(line.cart_line_id) !== line.quantity) {
      return line.cart_line_id;
    }
  }
  return after.lines.filter(l => !l.voided).at(-1)?.cart_line_id ?? null;
}

export function useCart(session: CartSession) {
  const [cart, setCart] = useState<Cart>(() => makeEmptyCart(session));
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [recentLineId, setRecentLineId] = useState<string | null>(null);

  const clearError = () => setError(null);

  const addByBarcode = useCallback(async (barcode: string, qty?: number) => {
    setLoading(true);
    setError(null);
    try {
      let updated = await cmd.posAddItemByBarcode(cart, barcode);
      const changedId = findChangedLineId(cart, updated);

      // Apply quantity prefix: posAddItemByBarcode already added 1;
      // if prefix qty > 1, set the final quantity to (base + qty) for existing lines
      // or just qty for new lines.
      if (qty && qty > 1 && changedId) {
        const existingLine = cart.lines.find(l => l.cart_line_id === changedId);
        const baseQty = existingLine ? parseFloat(existingLine.quantity) : 0;
        const finalQty = existingLine ? String(baseQty + qty) : String(qty);
        updated = await cmd.posUpdateQuantity(updated, changedId, finalQty);
      }

      setRecentLineId(findChangedLineId(cart, updated));
      setCart(updated);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Barcode not found");
    } finally {
      setLoading(false);
    }
  }, [cart]);

  const addProduct = useCallback(async (product: ProductWithPrice, qty?: string) => {
    setLoading(true);
    setError(null);
    try {
      const updated = await cmd.posAddItem(cart, product.product_id, qty);
      setRecentLineId(findChangedLineId(cart, updated));
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
      // Point recent at the new last active line
      const active = updated.lines.filter(l => !l.voided);
      setRecentLineId(active.at(-1)?.cart_line_id ?? null);
      setCart(updated);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to remove item");
    }
  }, [cart]);

  const applyBillDiscount = useCallback(async (discount_minor: number, reason: string) => {
    try {
      const updated = await cmd.posApplyBillDiscount(cart, discount_minor, reason, session.cashier_user_id);
      setCart(updated);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to apply discount");
    }
  }, [cart, session.cashier_user_id]);

  const applyLineDiscount = useCallback(async (cart_line_id: string, discount_minor: number, reason: string) => {
    try {
      const updated = await cmd.posApplyLineDiscount(cart, cart_line_id, discount_minor, reason, session.cashier_user_id);
      setCart(updated);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to apply line discount");
    }
  }, [cart, session.cashier_user_id]);

  const addCustomItem = useCallback(async (name: string, priceMajor: string, quantity: string) => {
    setLoading(true);
    setError(null);
    try {
      const priceMinor = Math.round(parseFloat(priceMajor) * Math.pow(10, DEVICE.currency_exponent));
      const updated = await cmd.posAddCustomItem(cart, name, priceMinor, quantity);
      setRecentLineId(findChangedLineId(cart, updated));
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

  const finalizeSale = useCallback(async (
    payments: PaymentInput[],
    customerId?: string,
    deliveryInput?: import("../types").DeliveryInput,
  ): Promise<SaleResult> => {
    setLoading(true);
    setError(null);
    try {
      const result = await cmd.posFinalizeSale(cart, payments, undefined, customerId, deliveryInput);
      setCart(makeEmptyCart(session));
      setRecentLineId(null);
      return result;
    } catch (e: unknown) {
      const msg = typeof e === "string" ? e : "Sale failed";
      setError(msg);
      throw new Error(msg, { cause: e });
    } finally {
      setLoading(false);
    }
  }, [cart, session]);

  const clearCart = useCallback(() => {
    const activeLines = cart.lines.filter(l => !l.voided);
    if (activeLines.length > 0) {
      const total = Math.max(0,
        activeLines.reduce((s, l) => s + l.line_total_minor, 0) - cart.bill_discount_minor
      );
      posRecordVoid(cart.cart_id, session.device_id, session.cashier_user_id, activeLines.length, total)
        .catch(() => {});
    }
    setCart(makeEmptyCart(session));
    setRecentLineId(null);
  }, [cart, session]);

  const replaceCart = useCallback((newCart: Cart) => {
    setCart(newCart);
    const active = newCart.lines.filter(l => !l.voided);
    setRecentLineId(active.at(-1)?.cart_line_id ?? null);
  }, []);

  /** Bump quantity of any specific line (used by inline +/− controls). */
  const bumpLine = useCallback(async (cart_line_id: string, delta: number) => {
    const line: CartLine | undefined = cart.lines.find(l => l.cart_line_id === cart_line_id && !l.voided);
    if (!line) return;
    const current = parseFloat(line.quantity);
    const next = current + delta;
    if (next <= 0) {
      await cmd.posRemoveLine(cart, cart_line_id).then(updated => {
        const active = updated.lines.filter(l => !l.voided);
        setRecentLineId(active.at(-1)?.cart_line_id ?? null);
        setCart(updated);
      }).catch((e: unknown) => setError(typeof e === "string" ? e : "Failed to remove item"));
    } else {
      await cmd.posUpdateQuantity(cart, cart_line_id, String(next)).then(updated => {
        setCart(updated);
      }).catch((e: unknown) => setError(typeof e === "string" ? e : "Invalid quantity"));
    }
  }, [cart]);

  /** Increment quantity of the most recently touched line (keyboard shortcut). */
  const bumpRecentQty = useCallback(async (delta: number) => {
    if (!recentLineId) return;
    await bumpLine(recentLineId, delta);
  }, [recentLineId, bumpLine]);

  /** Remove the most recently touched line. */
  const removeRecentLine = useCallback(async () => {
    if (!recentLineId) return;
    await removeLine(recentLineId);
  }, [recentLineId, removeLine]);

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
    recentLineId,
    addByBarcode,
    addProduct,
    addCustomItem,
    updateQuantity,
    removeLine,
    removeRecentLine,
    bumpLine,
    bumpRecentQty,
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
