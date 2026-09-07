import { useEffect, useState } from "react";
import { Bike, TriangleAlert } from "lucide-react";
import type { RiderRow } from "../types";
import { riderList } from "../tauri/commands";
import type { SessionToken } from "../types";

interface Props {
  sessionToken?: SessionToken;
  selectedId: string | null;
  onSelect: (rider: RiderRow | null) => void;
}

/**
 * Rider selection at checkout.
 *
 * Choosing a rider is optional — a shop with one driver on shift does not want
 * a dropdown standing between them and a completed sale, and the drop can be
 * assigned afterwards from the deliveries list. What selecting one does is send
 * the job to that rider's WhatsApp at the same moment the customer gets their
 * receipt.
 */
export default function RiderPicker({ sessionToken, selectedId, onSelect }: Props) {
  const [riders, setRiders] = useState<RiderRow[]>([]);
  const [loadFailed, setLoadFailed] = useState(false);

  useEffect(() => {
    if (!sessionToken) return;
    let cancelled = false;
    riderList(sessionToken, true)
      .then(rows => { if (!cancelled) { setRiders(rows); setLoadFailed(false); } })
      // A roster that will not load must not block the sale; the drop is still
      // recorded and can be assigned from the deliveries list later.
      .catch(() => { if (!cancelled) setLoadFailed(true); });
    return () => { cancelled = true; };
  }, [sessionToken]);

  return (
    <div className="pm-rider">
      <label className="delivery-label" htmlFor="payment-rider">
        <Bike size={14} aria-hidden="true" /> Rider
      </label>
      <select
        id="payment-rider"
        className="delivery-input pm-rider-select"
        value={selectedId ?? ""}
        disabled={loadFailed || riders.length === 0}
        onChange={event => {
          const next = riders.find(rider => rider.rider_id === event.target.value) ?? null;
          onSelect(next);
        }}
      >
        <option value="">— No rider assigned —</option>
        {riders.map(rider => (
          <option key={rider.rider_id} value={rider.rider_id}>
            {rider.name} · {rider.phone}
          </option>
        ))}
      </select>
      {loadFailed ? (
        <small className="pm-rider-hint pm-rider-hint-warn">
          <TriangleAlert size={13} aria-hidden="true" />
          Rider list unavailable — the delivery still saves, assign a rider from Deliveries.
        </small>
      ) : riders.length === 0 ? (
        <small className="pm-rider-hint">
          No riders yet. Add them under Admin → Team → Riders.
        </small>
      ) : (
        <small className="pm-rider-hint">
          The address, customer number and total are sent to the rider on WhatsApp.
        </small>
      )}
    </div>
  );
}
