import { useCallback, useEffect, type Dispatch, type RefObject, type SetStateAction } from "react";
import { formatMoney } from "../money";
import { applyDialpadKey } from "./Dialpad";
import { focusNextField } from "./paymentFieldNav";
import { typeIntoFocusedField } from "./paymentFieldTyping";
import type { ActiveField, PaymentLine } from "./paymentLines";
import type { PaymentInput } from "../types";

interface Options {
  activeField: ActiveField;
  lines: PaymentLine[];
  updateLine: (id: number, patch: Partial<PaymentLine>) => void;
  setLines: Dispatch<SetStateAction<PaymentLine[]>>;
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
  activeField, lines, updateLine, setLines, freshTenderedEntryRef,
  selectMethod, onCancel, netTotal, exp, canConfirm, loading, handleConfirmRef,
}: Options): (key: string) => void {
  const handleDialpadKey = useCallback((key: string) => {
    /* A focused text field wins over the virtual display fields below. The
       contact box is a real input, so the phone number is typed straight into
       it here rather than through a mirrored string — which is what lets the
       same keypad fill the address and the customer search too. */
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
    }
  }, [activeField, lines, updateLine, freshTenderedEntryRef]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      /* A field with the caret owns every key. The single-letter method
         shortcuts below are only safe because of this line: without SELECT and
         contenteditable in it, choosing a rider with the keyboard typed "c"
         and silently switched the sale to cash. */
      const target = e.target as HTMLElement;
      const tag = target.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target.isContentEditable) return;
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
      if (e.key !== "Enter") return;
      const active = document.activeElement as HTMLElement | null;
      // A suggestion list owns Enter while it is open — the cashier is picking
      // a customer, not finishing the sale.
      if (active?.closest(".bo-select-dropdown, [role='listbox'], .customer-drop")) return;
      if (active?.getAttribute("aria-expanded") === "true") return;

      /*
       * Enter inside a field means "I have finished this one". It used to mean
       * "complete the sale", which on a delivery saved an order with the
       * address still empty because the first field is the phone number.
       * Ctrl+Enter still completes from anywhere, for a cashier who knows the
       * rest is already filled.
       */
      const inField = active instanceof HTMLInputElement
        || active instanceof HTMLTextAreaElement
        || active instanceof HTMLSelectElement;
      if (inField && !e.ctrlKey && focusNextField(active.closest(".pm-shell"))) {
        e.preventDefault();
        return;
      }
      e.preventDefault();
      if (canConfirm && !loading) handleConfirmRef.current();
    };
    document.addEventListener("keydown", onEnterKey);
    return () => document.removeEventListener("keydown", onEnterKey);
  }, [canConfirm, loading, handleConfirmRef]);

  return handleDialpadKey;
}
