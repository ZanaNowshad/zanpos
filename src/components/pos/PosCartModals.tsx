import { useState, type Dispatch, type SetStateAction } from "react";
import { Trash2 } from "lucide-react";
import type { Cart } from "../../types";
import { DEVICE } from "../../types";
import { parseMoney } from "../../money";
import CustomItemModal from "../CustomItemModal";
import DiscountModal from "../DiscountModal";
import PriceInputModal from "../PriceInputModal";
import ManagerApprovalModal from "../ManagerApprovalModal";
import type { ActiveModal } from "./posModalState";

interface Props {
  activeModal: ActiveModal;
  setActiveModal: Dispatch<SetStateAction<ActiveModal>>;
  cart: Cart;
  lineCount: number;
  addCustomItem: (name: string, priceMajor: string, quantity: string) => Promise<void>;
  setLinePrice: (lineId: string, priceMinor: number, managerOverrideToken: string) => Promise<unknown>;
  applyBillDiscount: (discountMinor: number, reason: string, managerOverrideToken?: string) => Promise<unknown>;
  applyLineDiscount: (lineId: string, discountMinor: number, reason: string, managerOverrideToken?: string) => Promise<unknown>;
  clearCart: () => void;
  refreshSuggestions: () => void;
  resetNumpad: () => void;
  focusBarcode: () => void;
}

export default function PosCartModals({
  activeModal,
  setActiveModal,
  cart,
  lineCount,
  addCustomItem,
  setLinePrice,
  applyBillDiscount,
  applyLineDiscount,
  clearCart,
  refreshSuggestions,
  resetNumpad,
  focusBarcode,
}: Props) {
  const close = () => {
    setActiveModal({ kind: "none" });
    focusBarcode();
  };

  // Work held back because the till needs a manager's PIN before it can be
  // sent. A price override always needs one. A discount only needs one when
  // policy forbids cashiers from discounting, and the backend is the authority
  // on that — so a discount is attempted first and only escalates here if it
  // comes back asking for approval. That keeps the PIN prompt off the common
  // path instead of guessing the policy in the UI.
  const [pending, setPending] = useState<
    | { kind: "price"; lineId: string; priceMinor: number }
    | { kind: "bill"; discountMinor: number; reason: string }
    | { kind: "line"; lineId: string; discountMinor: number; reason: string }
    | null
  >(null);

  const needsApproval = (e: unknown) =>
    (typeof e === "string" ? e : (e as Error)?.message ?? "").includes("manager");

  const approvalLabel =
    pending?.kind === "price" ? "Override this line's price" : "Apply this discount";

  return (
    <>
      {activeModal.kind === "clearConfirm" && (
        <button className="modal-overlay" type="button" onClick={close}>
          <div role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}  className="modal clear-confirm-modal" onClick={event => event.stopPropagation()}>
            <div className="modal-header">
              <span className="modal-title">Clear Cart?</span>
            </div>
            <p className="clear-confirm-body">
              Remove all {lineCount} item{lineCount !== 1 ? "s" : ""} from the cart?
              This will be recorded as a pre-tender void.
            </p>
            <div className="modal-actions">
              <button className="btn-secondary" onClick={close}>Cancel</button>
              <button className="btn-danger" onClick={() => { clearCart(); close(); }}>
                <Trash2 size={15} strokeWidth={1.75} aria-hidden="true" /> Clear Cart
              </button>
            </div>
          </div>
        </button>
      )}

      {activeModal.kind === "customItem" && (
        <CustomItemModal
          onAdd={async (name, price, qty) => {
            await addCustomItem(name, price, qty);
            setActiveModal({ kind: "none" });
            refreshSuggestions();
            focusBarcode();
          }}
          onCancel={() => {
            setActiveModal({ kind: "none" });
            refreshSuggestions();
            focusBarcode();
          }}
        />
      )}

      {activeModal.kind === "priceInput" && (
        <PriceInputModal
          productName={activeModal.mode === "setExisting"
            ? activeModal.productName
            : activeModal.itemName}
          currentPriceMinor={activeModal.mode === "setExisting"
            ? activeModal.currentPriceMinor
            : undefined}
          onConfirm={async priceMajor => {
            if (activeModal.mode === "setExisting") {
              const priceMinor = parseMoney(priceMajor, DEVICE.currency_exponent);
              // Never applied on the cashier's say-so — a manager approves it.
              setPending({ kind: "price", lineId: activeModal.lineId, priceMinor });
              return;
            } else {
              await addCustomItem(activeModal.itemName, priceMajor, activeModal.quantity);
              resetNumpad();
            }
            close();
          }}
          onCancel={close}
        />
      )}

      {activeModal.kind === "discount" && (
        <DiscountModal
          grossMinor={cart.lines.filter(line => !line.voided).reduce((sum, line) => sum + line.line_total_minor, 0)}
          currentBillDiscountMinor={cart.bill_discount_minor}
          lines={cart.lines.filter(line => !line.voided)}
          initialLineId={activeModal.lineId}
          onApplyBill={async (discountMinor, reason) => {
            try {
              await applyBillDiscount(discountMinor, reason);
              close();
            } catch (e) {
              if (!needsApproval(e)) throw e;
              setPending({ kind: "bill", discountMinor, reason });
            }
          }}
          onApplyLine={async (lineId, discountMinor, reason) => {
            try {
              await applyLineDiscount(lineId, discountMinor, reason);
              close();
            } catch (e) {
              if (!needsApproval(e)) throw e;
              setPending({ kind: "line", lineId, discountMinor, reason });
            }
          }}
          onCancel={close}
        />
      )}

      {pending && (
        <ManagerApprovalModal
          action={approvalLabel}
          onApproved={async token => {
            if (pending.kind === "price") {
              await setLinePrice(pending.lineId, pending.priceMinor, token);
            } else if (pending.kind === "bill") {
              await applyBillDiscount(pending.discountMinor, pending.reason, token);
            } else {
              await applyLineDiscount(
                pending.lineId,
                pending.discountMinor,
                pending.reason,
                token,
              );
            }
            setPending(null);
            close();
          }}
          onCancel={() => setPending(null)}
        />
      )}
    </>
  );
}
