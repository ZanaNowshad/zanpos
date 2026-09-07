import { useState } from "react";
import WaMessageEditor from "../WaMessageEditor";
import WhatsAppSection from "./WhatsAppSection";
import type { SessionToken } from "../../types";
import {
  loadWaFormat,
  saveWaFormat,
  makeDefaultEnLines,
  makeDefaultArLines,
  loadWaCustomerFormat,
  saveWaCustomerFormat,
  makeDefaultCustomerEnLines,
  makeDefaultCustomerArLines,
} from "../../utils/waMessageFormat";

interface WhatsAppTabProps {
  sessionToken: SessionToken;
  sessionRole: string;
  registerTimer: (id: ReturnType<typeof setTimeout>) => void;
}

type MsgKind = "delivery" | "customer";

export default function WhatsAppTab({ sessionToken, sessionRole, registerTimer }: WhatsAppTabProps) {
  const [kind, setKind] = useState<MsgKind>("delivery");

  return (
    <div className="settings-page">
      <section>
        <h3 className="settings-page-title">WhatsApp</h3>
        <WhatsAppSection sessionToken={sessionToken} sessionRole={sessionRole} registerTimer={registerTimer} />
      </section>

      <hr className="settings-page-divider" />

      <section>
        <h3 className="settings-page-title">WhatsApp Message Format</h3>
        <p className="settings-hint">
          Customise the WhatsApp message sent to customers. Pick which message to edit below,
          choose language, toggle lines, edit text, and reorder. Saved on this device only.
        </p>

        {/* Which message to edit */}
        <div
          className="wame-kind-tabs"
          role="tablist"
          aria-label="Message type"
          style={{ display: "flex", gap: "8px", flexWrap: "wrap" }}
        >
          <button
            role="tab"
            aria-selected={kind === "delivery"}
            className={kind === "delivery" ? "btn-primary btn-sm" : "btn-secondary btn-sm"}
            onClick={() => setKind("delivery")}
          >
            🛵 Delivery Order
          </button>
          <button
            role="tab"
            aria-selected={kind === "customer"}
            className={kind === "customer" ? "btn-primary btn-sm" : "btn-secondary btn-sm"}
            onClick={() => setKind("customer")}
          >
            🧾 Customer Receipt
          </button>
        </div>

        <p className="settings-hint" style={{ marginTop: "10px" }}>
          {kind === "delivery"
            ? "Sent automatically when a delivery order is confirmed."
            : "Sent automatically on a normal (non-delivery) sale when a customer with a saved phone number is selected at checkout."}
        </p>

        {/* key forces a fresh mount so each editor initialises from its own saved format */}
        {kind === "delivery" ? (
          <WaMessageEditor
            key="delivery"
            load={loadWaFormat}
            save={saveWaFormat}
            makeDefaultEn={makeDefaultEnLines}
            makeDefaultAr={makeDefaultArLines}
          />
        ) : (
          <WaMessageEditor
            key="customer"
            load={loadWaCustomerFormat}
            save={saveWaCustomerFormat}
            makeDefaultEn={makeDefaultCustomerEnLines}
            makeDefaultAr={makeDefaultCustomerArLines}
          />
        )}
      </section>
    </div>
  );
}
