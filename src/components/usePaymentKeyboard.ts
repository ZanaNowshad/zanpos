import { useCallback, useEffect, type Dispatch, type RefObject, type SetStateAction } from "react";
import { formatMoney } from "../money";
import { applyDialpadKey } from "./Dialpad";
import { typeIntoFocusedField } from "./paymentFieldTyping";
import type { ActiveField, PaymentLine } from "./paymentLines";
import type { PaymentInput } from "../types";

interface Options {
  activeField: ActiveField;
  lines: PaymentLine[];
  updateLine: (id: number, patch: Partial<PaymentLine>) => void;
  setLines: Dispatch<SetStateAction<PaymentLine[]>>;
  setPhoneRaw: Dispatch<SetStateAction<string>>;
  /** True until the cashier types their first tendered digit. */
  freshTenderedEntryRef: RefObject<boolean>;
  selectMethod: (method: PaymentLine["method"]) => void;
  onCancel: () => void;
  netTotal: number;
  exp: number;
  canConfirm: boolean;
  loading?: boolean;
  /** Read through a ref so the listener never closes over a stale confirm. */
  handleConfirmRef: RefObject<() => void>;
}

/**
 * Every way a key reaches the payment modal: the on-screen dialpad, the
 * physical keyboard's shortcuts, and Enter to confirm.
 *
 * Lifted out of PaymentModal, which had grown past the 500-line limit the ship
 * gate enforces. Keeping the three together is not just line count — they
 * share one rule that is easy to break separately: a keystroke belongs to a
 * focused text field first, and only falls through to the virtual display
 * fields when nothing real has the caret.
 */
export function usePaymentKeyboard({
  activeField, lines, updateLine, setLines, setPhoneRaw, freshTenderedEntryRef,
  selectMethod, onCancel, netTotal, exp, canConfirm, loading, handleConfirmRef,
}: Options): (key: string) => void {
  const handleDialpadKey = useCallback((key: string) => {
    // A focused text field wins over the virtual display fields below.
    if (typeIntoFocusedField(key)) return;
    if (!activeField) return;
    if (activeField.kind === "amount" || activeField.kind === "tendered") {
      const line = lines.find(l => l.id === activeField.lineId);
      if (!line) return;
      const cur = activeField.kind === "amount" ? line.amountStr : line.tenderedStr;
      const startsFreshTendered = activeField.kind === "tendered" && freshTenderedEntryRef.current;
      const next = applyDialpadKey(startsFreshTendered && key !== "⌫" ? "" : cur, key);
      if (activeField.kind === "amount") updateLine(activeField.lineId, { amountStr: next });
      else {
        freshTenderedEntryRef.current = false;
        updateLine(activeField.lineId, { tenderedStr: next });
      }
    } else if (activeField.kind === "phone") {
      setPhoneRaw(prev => {
        if (key === "⌫") return prev.slice(0, -1);
        if (key === "C")  return "";
        if (key === "." || key === "00") return prev;
        if (prev.length >= 8) return prev;
        return prev + key;
      });
    }
  }, [activeField, lines, updateLine, setPhoneRaw, freshTenderedEntryRef]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement).tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      const k = e.key.toLowerCase();
      if (e.key >= "0" && e.key <= "9") { e.preventDefault(); handleDialpadKey(e.key); }
      else if (e.key === "Backspace")   { e.preventDefault(); handleDialpadKey("⌫"); }
      else if (e.key === "Delete")      { e.preventDefault(); handleDialpadKey("C"); }
      else if (e.key === ".")           { e.preventDefault(); handleDialpadKey("."); }
      else if (k === "c" || e.key === "F1") { e.preventDefault(); selectMethod("cash" as PaymentInput["method"]); }
      else if (k === "a" || e.key === "F2") { e.preventDefault(); selectMethod("card" as PaymentInput["method"]); }
      else if (k === "w" || e.key === "F3") { e.preventDefault(); selectMethod("wallet" as PaymentInput["method"]); }
      else if (k === "e") {
        e.preventDefault();
        const due = formatMoney(netTotal, exp);
        setLines(p => p.map((l, i) => i === 0 ? { ...l, amountStr: due, tenderedStr: due } : l));
      }
      else if (e.key === "Escape")      { e.preventDefault(); onCancel(); }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [handleDialpadKey, onCancel, netTotal, exp, selectMethod, setLines]);

  useEffect(() => {
    const onEnterKey = (e: KeyboardEvent) => {
      // Only skip if a dropdown/autocomplete is visible (avoid accidental confirm)
      const active = document.activeElement as HTMLElement | null;
      if (active && active.closest(".bo-select-dropdown, [role='listbox'], .customer-drop")) return;
      if (e.key === "Enter" || e.key === "NumpadEnter") {
        e.preventDefault();
        if (canConfirm && !loading) handleConfirmRef.current();
      }
    };
    document.addEventListener("keydown", onEnterKey);
    return () => document.removeEventListener("keydown", onEnterKey);
  }, [canConfirm, loading, handleConfirmRef]);

  return handleDialpadKey;
}
