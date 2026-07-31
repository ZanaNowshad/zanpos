import type { Dispatch, RefObject, SetStateAction } from "react";
import BarcodeInput, { type BarcodeInputHandle } from "../BarcodeInput";
import CartPanel from "../CartPanel";
import type { Cart } from "../../types";
import { DEVICE } from "../../types";
import type { useCart } from "../../hooks/useCart";
import type { ActiveModal } from "./posModalState";

interface Suggestion {
  id: string;
  name: string;
  price: string;
}

interface Props {
  barcodeRef: RefObject<BarcodeInputHandle | null>;
  actorUserId: string;
  cart: Cart;
  netTotal: number;
  taxTotal: number;
  numpadValue: string;
  suggestions: Suggestion[];
  loading: boolean;
  payFastLoading: boolean;
  paymentStarted: boolean;
  recentLineId: string | null;
  setNumpadValue: Dispatch<SetStateAction<string>>;
  setActiveModal: Dispatch<SetStateAction<ActiveModal>>;
  setError: ReturnType<typeof useCart>["setError"];
  addProduct: ReturnType<typeof useCart>["addProduct"];
  addCustomItem: ReturnType<typeof useCart>["addCustomItem"];
  updateQuantity: ReturnType<typeof useCart>["updateQuantity"];
  removeLine: ReturnType<typeof useCart>["removeLine"];
  applyLineDiscount: ReturnType<typeof useCart>["applyLineDiscount"];
  setLineNote: ReturnType<typeof useCart>["setLineNote"];
  bumpLine: ReturnType<typeof useCart>["bumpLine"];
  onBarcode: (barcode: string) => void;
  onPaySplit: () => void;
  onPayFast: () => void;
  onPayDirect: (method: "cash" | "card" | "wallet") => void;
  focusBarcode: () => void;
}

export default function PosCartColumn({
  barcodeRef,
  actorUserId,
  cart,
  netTotal,
  taxTotal,
  numpadValue,
  suggestions,
  loading,
  payFastLoading,
  paymentStarted,
  recentLineId,
  setNumpadValue,
  setActiveModal,
  setError,
  addProduct,
  addCustomItem,
  updateQuantity,
  removeLine,
  applyLineDiscount,
  setLineNote,
  bumpLine,
  onBarcode,
  onPaySplit,
  onPayFast,
  onPayDirect,
  focusBarcode,
}: Props) {
  const disabled = loading || payFastLoading;

  return (
    <div className="pos-cart-col">
      <BarcodeInput
        ref={barcodeRef}
        onBarcode={onBarcode}
        onSelectProduct={async product => {
          const qty = parseInt(numpadValue) > 1 ? numpadValue : undefined;
          try {
            await addProduct(product, qty);
            barcodeRef.current?.flashSuccess();
            setNumpadValue("1");
          } catch (error) {
            barcodeRef.current?.flashError();
            setError(error instanceof Error ? error.message : "Failed to add product — please try again");
          }
        }}
        onSearch={() => {}}
        onEscape={() => {}}
        actorUserId={actorUserId}
        disabled={disabled}
      />

      <div className="pos-quickadd-strip">
        <button
          className="pos-quickadd-custom"
          onClick={() => setActiveModal({ kind: "customItem" })}
          disabled={disabled}
          title="Add a custom item"
        >
          ✦ Custom
        </button>
        {suggestions.map(suggestion => {
          const hasPrice = suggestion.price && parseFloat(suggestion.price) > 0;
          return (
            <button
              key={suggestion.id}
              className="pos-quickadd-chip"
              disabled={disabled}
              onClick={async () => {
                if (!hasPrice) {
                  setActiveModal({
                    kind: "priceInput",
                    mode: "addNew",
                    itemName: suggestion.name,
                    quantity: numpadValue,
                  });
                  return;
                }
                await addCustomItem(suggestion.name, suggestion.price, numpadValue);
                setNumpadValue("1");
                focusBarcode();
              }}
              title={hasPrice
                ? `${suggestion.name} — ${DEVICE.currency} ${suggestion.price}`
                : `${suggestion.name} — price varies`}
            >
              <span className="pqc-name">{suggestion.name}</span>
              <span className="pqc-price">{hasPrice ? suggestion.price : "—"}</span>
            </button>
          );
        })}
      </div>

      <CartPanel
        cart={cart}
        netTotal={netTotal}
        taxTotal={taxTotal}
        onUpdateQty={updateQuantity}
        onRemove={removeLine}
        onApplyLineDiscount={applyLineDiscount}
        onSetLineNote={setLineNote}
        onPaySplit={onPaySplit}
        onPayFast={onPayFast}
        onPayDirect={onPayDirect}
        payFastLoading={payFastLoading}
        paymentStarted={paymentStarted}
        recentLineId={recentLineId}
        onBumpLine={bumpLine}
        compact
      />
    </div>
  );
}
