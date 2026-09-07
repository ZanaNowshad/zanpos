import type { DeliveryInput, RiderRow } from "../types";
import type { PaymentContactState } from "../hooks/usePaymentContact";
import type { PaymentJourney } from "./PaymentModal";
import DeliveryForm from "./DeliveryForm";
import PaymentContactField from "./PaymentContactField";
import RiderPicker from "./RiderPicker";
import { DigitalReceiptGuide, PrintReceiptOption, SplitToggle } from "./PaymentExperience";
import type { SessionToken } from "../types";

interface Props {
  journey: PaymentJourney;
  contact: PaymentContactState;
  sessionToken?: SessionToken;
  deliveryData: Partial<DeliveryInput>;
  onDeliveryChange: (value: Partial<DeliveryInput>) => void;
  rider: RiderRow | null;
  onRiderChange: (rider: RiderRow | null) => void;
  printReceipt: boolean;
  onPrintReceiptChange: (next: boolean) => void;
  onContactFocus: () => void;
  showSplitToggle: boolean;
  splitLabel: string;
  onSplit: () => void;
}

/**
 * The middle column: everything about where this sale goes, as opposed to what
 * it costs.
 *
 * Split out of PaymentModal both for the 500-line limit and because the three
 * journeys disagree about this column and only this column — the money side is
 * identical whether the customer is carrying the bag out or a rider is. Keeping
 * the disagreement in one file makes it possible to see all three at once.
 *
 * Every block here is sized to fit the till's 1024×768 panel without scrolling.
 * The delivery journey is the tallest of the three and used to run 114px past
 * the bottom, which put the rider picker off-screen: the cashier could complete
 * a delivery without ever seeing that a rider could be assigned, and the drop
 * would sit unassigned in the queue.
 */
export default function PaymentContactPanel({
  journey, contact, sessionToken, deliveryData, onDeliveryChange,
  rider, onRiderChange, printReceipt, onPrintReceiptChange, onContactFocus,
  showSplitToggle, splitLabel, onSplit,
}: Props) {
  const isDelivery = journey === "delivery";
  const requiresContact = journey !== "receipt";

  return (
    <div className={`pm-mid pm-mid-${journey}`}>
      <div className="pm-step-label pm-step-muted">
        <span>3</span>
        {isDelivery ? "Where it goes, and who takes it"
          : journey === "digital" ? "Where the receipt is sent"
          : "Receipt options"}
      </div>

      {showSplitToggle && <SplitToggle label={splitLabel} onSplit={onSplit} />}

      {requiresContact && (
        <PaymentContactField
          contact={contact}
          sessionToken={sessionToken}
          onFocus={onContactFocus}
          isDelivery={isDelivery}
        />
      )}

      {journey === "digital" && <DigitalReceiptGuide />}

      {isDelivery && (
        <>
          <DeliveryForm value={deliveryData} onChange={onDeliveryChange} />
          <RiderPicker
            sessionToken={sessionToken}
            selectedId={rider?.rider_id ?? null}
            onSelect={onRiderChange}
          />
        </>
      )}

      <PrintReceiptOption checked={printReceipt} onChange={onPrintReceiptChange} />
    </div>
  );
}
