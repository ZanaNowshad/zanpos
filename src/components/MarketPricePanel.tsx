import { useCallback, useEffect, useState } from "react";
import {
  marketPriceCached, marketPriceConfirmMatch, marketPriceRejectMatch,
  marketPriceSearch, marketWatchlistSet,
  type MarketCandidate, type MarketPriceReport,
} from "../tauri/commands";
import { formatMoney } from "../money";
import { DEVICE } from "../types";
import type { SessionToken } from "../types";

/**
 * What other shops charge for this product, next to the box where its price is
 * typed.
 *
 * The panel never sets a price. `onUsePrice` fills the form's price field and
 * the operator still saves through the ordinary path, which carries the RBAC,
 * the confirmation and the audit trail. A competitor's number reaching the shelf
 * without a person deciding is the failure this is arranged to prevent, so there
 * is no button here that writes one.
 *
 * Cached on open, network only when asked. A product form should not stall
 * behind somebody else's website.
 */

const exp = DEVICE.currency_exponent;
const cur = DEVICE.currency;

function money(minor: number | null): string {
  return minor === null ? "—" : `${cur} ${formatMoney(minor, exp)}`;
}

/** Old enough to say so. A price from last month is not today's market. */
function age(iso: string | null): string | null {
  if (!iso) return null;
  const days = Math.floor((Date.now() - new Date(iso).getTime()) / 86_400_000);
  if (Number.isNaN(days)) return null;
  if (days <= 0) return "checked today";
  if (days === 1) return "checked yesterday";
  if (days < 30) return `checked ${days} days ago`;
  return `checked ${Math.round(days / 30)} month(s) ago — worth refreshing`;
}

export default function MarketPricePanel({
  productId, sessionToken, onUsePrice,
}: {
  productId: string;
  sessionToken: SessionToken;
  onUsePrice: (minor: number) => void;
}) {
  const [report, setReport] = useState<MarketPriceReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<"idle" | "loading" | "searching" | "saving">("loading");
  const [tracked, setTracked] = useState(false);

  const fail = (cause: unknown) =>
    setError(typeof cause === "string" ? cause : "Could not read market prices.");

  const receive = (next: MarketPriceReport) => {
    setReport(next);
    setTracked(next.tracked);
  };

  const load = useCallback(() => {
    setBusy("loading");
    marketPriceCached(sessionToken, productId)
      .then(receive)
      .catch(fail)
      .finally(() => setBusy("idle"));
  }, [sessionToken, productId]);

  useEffect(load, [load]);

  const search = () => {
    setBusy("searching");
    setError(null);
    marketPriceSearch(sessionToken, productId)
      .then(receive)
      .catch(fail)
      .finally(() => setBusy("idle"));
  };

  const confirm = (candidate: MarketCandidate) => {
    setBusy("saving");
    marketPriceConfirmMatch(sessionToken, productId, candidate)
      .then(receive)
      .catch(fail)
      .finally(() => setBusy("idle"));
  };

  const reject = (matchId: string) => {
    setBusy("saving");
    marketPriceRejectMatch(sessionToken, matchId)
      .then(load)
      .catch(fail)
      .finally(() => setBusy("idle"));
  };

  const toggleWatch = (next: boolean) => {
    setTracked(next);
    marketWatchlistSet(sessionToken, productId, next).catch(cause => {
      setTracked(!next);
      fail(cause);
    });
  };

  if (busy === "loading" && !report) {
    return <p className="settings-hint">Reading what is already known…</p>;
  }
  if (!report) {
    return <div className="modal-error">{error ?? "No market data."}</div>;
  }

  const { summary, trusted, candidates, unavailable } = report;
  const checked = age(summary.observed_at);
  const working = busy !== "idle";

  return (
    <div className="market-panel">
      {error && <div className="modal-error">{error}</div>}

      <div className="market-actions">
        <button type="button" className="btn-secondary" onClick={search} disabled={working}>
          {busy === "searching" ? "Checking other shops…" : "Check other shops"}
        </button>
        <label className="market-watch">
          <input
            type="checkbox"
            checked={tracked}
            disabled={working}
            onChange={event => toggleWatch(event.target.checked)}
          />
          Keep this one up to date
        </label>
      </div>

      {summary.retailer_count === 0 ? (
        <p className="settings-hint">
          No confirmed match yet, so there is nothing to compare against. Check other
          shops, then confirm the listing that is actually this product.
        </p>
      ) : (
        <>
          <dl className="market-summary">
            <div><dt>Lowest</dt><dd>{money(summary.low_minor)}</dd></div>
            <div><dt>Middle</dt><dd>{money(summary.median_minor)}</dd></div>
            <div><dt>Highest</dt><dd>{money(summary.high_minor)}</dd></div>
          </dl>
          <p className="settings-hint">
            {/* One retailer is a price, not a market. Saying so stops a single
                shop's number being read as what everyone charges. */}
            {summary.retailer_count === 1
              ? "From one retailer only — that is a price, not a market."
              : `From ${summary.retailer_count} retailers.`}
            {checked ? ` ${checked}.` : ""}
          </p>

          <ul className="market-list">
            {trusted.map(row => (
              <li key={`${row.match_id}-${row.retailer_name}`}>
                <span className="market-retailer">{row.retailer_name}</span>
                <span className="market-price">{money(row.price_minor)}</span>
                <button type="button" className="market-use" disabled={working}
                        onClick={() => onUsePrice(row.price_minor)}>
                  Use
                </button>
                {/* Withdrawing a confirmation, not deleting a price. The
                    observations stay; they simply stop being trusted. */}
                <button type="button" className="market-reject" disabled={working}
                        onClick={() => reject(row.match_id)}>
                  Not this product
                </button>
              </li>
            ))}
          </ul>

          {summary.median_minor !== null && (
            <button type="button" className="btn-secondary" disabled={working}
                    onClick={() => onUsePrice(summary.median_minor as number)}>
              Use the middle price ({money(summary.median_minor)})
            </button>
          )}
        </>
      )}

      {candidates.length > 0 && (
        <div className="market-candidates">
          <h5>Possible matches</h5>
          <p className="settings-hint">
            {/* Kept out of the figures above until somebody says so. A listing
                that looks right and is not would move the median silently. */}
            These look like this product but nobody has confirmed them, so their
            prices are not counted above.
          </p>
          <ul className="market-list">
            {candidates.map(candidate => (
              <li key={`${candidate.source_id}-${candidate.source_product_key}`}>
                <span className="market-retailer">
                  {candidate.name}
                  {candidate.pack_text ? ` · ${candidate.pack_text}` : ""}
                </span>
                <span className="market-price">
                  {candidate.offers.length > 0
                    ? money(Math.min(...candidate.offers.map(o => o.price_minor)))
                    : "—"}
                </span>
                <span className="market-confidence">{candidate.confidence}% sure</span>
                <button type="button" className="market-use" disabled={working}
                        onClick={() => confirm(candidate)}>
                  This is it
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}

      {unavailable.length > 0 && (
        <div className="market-unavailable">
          {/* Named rather than omitted. A source that quietly returned nothing
              would read as "nobody else sells this", which is the opposite of
              what it means. */}
          <h5>Could not check</h5>
          <ul>
            {unavailable.map(source => (
              <li key={source.source_id}>
                {source.name}
                {source.reason ? ` — ${source.reason}` : ""}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
