import type { Dispatch, RefObject, SetStateAction } from "react";
import { ShoppingBasket } from "lucide-react";
import BarcodeInput, { type BarcodeInputHandle } from "../BarcodeInput";
import PosCartTable from "./PosCartTable";
import PosQuickAddRail from "./PosQuickAddRail";
import type { QuickPosSlot } from "../../tauri/commands";
import type { Cart, CartLine } from "../../types";
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
  numpadValue: string;
  suggestions: Suggestion[];
  loading: boolean;
  payFastLoading: boolean;
  selectedLineId: string | null;
  quickSlots: QuickPosSlot[];
  setNumpadValue: Dispatch<SetStateAction<string>>;
  setActiveModal: Dispatch<SetStateAction<ActiveModal>>;
  setError: ReturnType<typeof useCart>["setError"];
  addProduct: ReturnType<typeof useCart>["addProduct"];
  addCustomItem: ReturnType<typeof useCart>["addCustomItem"];
  onBarcode: (barcode: string) => void;
  onQuickAdd: (productId: string) => void;
  onSelectLine: (lineId: string) => void;
  onBumpQty: (lineId: string, delta: number) => void;
  onEditPrice: (line: CartLine) => void;
  focusBarcode: () => void;
}

export default function PosCartColumn({
  barcodeRef, actorUserId, cart, numpadValue, suggestions, loading,
  payFastLoading, selectedLineId, quickSlots, setNumpadValue, setActiveModal,
  setError, addProduct, addCustomItem, onBarcode, onQuickAdd, onSelectLine,
  onBumpQty, onEditPrice, focusBarcode,
}: Props) {
  const disabled = loading || payFastLoading;
  const basketCount = cart.lines
    .filter(line => !line.voided)
    .reduce((sum, line) => sum + (Number.parseFloat(line.quantity) || 0), 0);

  return (
    <div className="till-main">
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

      {/* Frequently-sold items that are not in the catalogue. Rendered only when
          the shop actually has some, so the strip costs nothing on a till that
          never uses custom items. "Custom" itself lives in More Options. */}
      {suggestions.length > 0 && (
        <div className="till-quickadd">
          {suggestions.map(suggestion => {
            const hasPrice = suggestion.price && parseFloat(suggestion.price) > 0;
            return (
              <button
                key={suggestion.id}
                type="button"
                className="till-quickadd-chip"
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
                <span className="till-quickadd-name">{suggestion.name}</span>
                <span className="till-quickadd-price">{hasPrice ? suggestion.price : "—"}</span>
              </button>
            );
          })}
        </div>
      )}

      {/* The last-scanned strip under the cart already reports the item just
          added, so this row carries the shop's one-tap products instead. */}
      <div className="till-strips">
        <PosQuickAddRail slots={quickSlots} disabled={disabled} onAdd={onQuickAdd} />
        <div className="till-basket">
          <ShoppingBasket size={20} aria-hidden="true" />
          <span className="till-basket-text">
            <strong>{Number.isInteger(basketCount) ? basketCount : basketCount.toFixed(3)} items</strong>
            <small>in basket</small>
          </span>
        </div>
      </div>

      <PosCartTable
        cart={cart}
        selectedLineId={selectedLineId}
        disabled={disabled}
        onSelectLine={onSelectLine}
        onBumpQty={onBumpQty}
        onEditPrice={onEditPrice}
      />
    </div>
  );
}
