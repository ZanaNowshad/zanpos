import type { Dispatch, SetStateAction } from "react";
import { Trash2 } from "lucide-react";
import type { Cart, CartLine } from "../../types";
import { DEVICE } from "../../types";
import { parseMoney } from "../../money";
import CustomItemModal from "../CustomItemModal";
import DiscountModal from "../DiscountModal";
import LineDiscountModal from "../LineDiscountModal";
import PriceInputModal from "../PriceInputModal";
import type { ActiveModal } from "./posModalState";

interface Props {
  activeModal: ActiveModal;
  setActiveModal: Dispatch<SetStateAction<ActiveModal>>;
  cart: Cart;
  lineCount: number;
  discountLine?: CartLine;
  addCustomItem: (name: string, priceMajor: string, quantity: string) => Promise<void>;
  setLinePrice: (lineId: string, priceMinor: number) => Promise<unknown>;
  applyBillDiscount: (discountMinor: number, reason: string) => Promise<unknown>;
  applyLineDiscount: (lineId: string, discountMinor: number, reason: string) => Promise<unknown>;
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
  discountLine,
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

  return (
    <>
      {activeModal.kind === "clearConfirm" && (
        <button className="modal-overlay" type="button" onClick={close}>
          <div className="modal clear-confirm-modal" onClick={event => event.stopPropagation()}>
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
          onConfirm={async priceMajor => {
            if (activeModal.mode === "setExisting") {
              const priceMinor = parseMoney(priceMajor, DEVICE.currency_exponent);
              await setLinePrice(activeModal.lineId, priceMinor);
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
          currentDiscountMinor={cart.bill_discount_minor}
          onApply={async (discountMinor, reason) => {
            await applyBillDiscount(discountMinor, reason);
            close();
          }}
          onCancel={close}
        />
      )}

      {discountLine && (
        <LineDiscountModal
          line={discountLine}
          onApply={async (discountMinor, reason) => {
            await applyLineDiscount(discountLine.cart_line_id, discountMinor, reason);
            close();
          }}
          onCancel={close}
        />
      )}
    </>
  );
}
