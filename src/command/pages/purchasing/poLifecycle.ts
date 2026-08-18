/**
 * Purchase-order lifecycle, derived from the real backend.
 *
 * Source of truth: migrations/0009_suppliers_and_purchases.sql
 *   status TEXT NOT NULL DEFAULT 'draft'  -- draft|ordered|partial|received|cancelled
 *
 * NOTE — there is deliberately no `approval` state here. The redesign brief
 * asked for "review / approval if required" between draft and ordered, but the
 * purchase_orders table has no approval column, no approver, and no threshold,
 * and no command performs one. Rendering an approval step would be a lie about
 * what the system does, so the pipeline shows the five states that exist and
 * the gap is documented rather than faked.
 */
export type PoStatus = "draft" | "ordered" | "partial" | "received" | "cancelled";

export const PO_PIPELINE: PoStatus[] = ["draft", "ordered", "partial", "received"];

export interface PoStatusMeta {
  labelKey: string;
  /** Maps to the shared status tones used by tables and badges. */
  tone: "muted" | "info" | "warn" | "ok" | "danger";
  /** Position in the pipeline; -1 for terminal states outside it. */
  step: number;
}

export const PO_STATUS: Record<PoStatus, PoStatusMeta> = {
  draft:     { labelKey: "poDraft",     tone: "muted", step: 0 },
  ordered:   { labelKey: "poOrdered",   tone: "info",  step: 1 },
  partial:   { labelKey: "poPartial",   tone: "warn",  step: 2 },
  received:  { labelKey: "poReceived",  tone: "ok",    step: 3 },
  cancelled: { labelKey: "poCancelled", tone: "danger", step: -1 },
};

export function poStatus(raw: string): PoStatus {
  return (Object.prototype.hasOwnProperty.call(PO_STATUS, raw) ? raw : "draft") as PoStatus;
}

/** Orders that still need someone to do something. */
export function isOpen(status: string): boolean {
  const s = poStatus(status);
  return s === "draft" || s === "ordered" || s === "partial";
}

/**
 * The one action that matters for an order in this state. Purchasing used to
 * show every control at once; a contextual primary keeps the work obvious.
 */
export function primaryActionFor(status: string): "send" | "receive" | null {
  const s = poStatus(status);
  if (s === "draft") return "send";
  if (s === "ordered" || s === "partial") return "receive";
  return null;
}

export interface LineLike { ordered_qty: number; received_qty: number; unit_cost_minor: number }

export function outstanding(line: LineLike): number {
  return Math.max(0, line.ordered_qty - line.received_qty);
}

/** Lines still owed by the supplier — the reconciliation view. */
export function outstandingLines<T extends LineLike>(lines: T[]): T[] {
  return lines.filter(l => outstanding(l) > 0);
}

export function orderedValue(lines: LineLike[]): number {
  return lines.reduce((n, l) => n + l.ordered_qty * l.unit_cost_minor, 0);
}

export function receivedValue(lines: LineLike[]): number {
  return lines.reduce((n, l) => n + l.received_qty * l.unit_cost_minor, 0);
}

/**
 * Margin at the current selling price, in basis points, or null when the
 * selling price is unknown. Cost is what this PO pays; price comes from the
 * catalogue. Anything below `MARGIN_WARN_BP` is surfaced as an exception.
 */
export const MARGIN_WARN_BP = 1500; // 15%

export function marginBp(unitCostMinor: number, sellingPriceMinor: number | undefined): number | null {
  if (!sellingPriceMinor || sellingPriceMinor <= 0) return null;
  return Math.round(((sellingPriceMinor - unitCostMinor) / sellingPriceMinor) * 10000);
}

// ─── Receiving ────────────────────────────────────────────────────────────────
//
// Contract traced from src-tauri/src/commands/purchasing_commands.rs
// (`po_receive_inner`), which is the authority for every rule below:
//
//   • `lines` empty/null  → receive the full outstanding quantity on every line.
//   • `qty <= 0`          → the line is SKIPPED, not rejected. Zero therefore
//                           means "not on this receipt", and is valid input.
//   • `received + qty > ordered` → Validation error, server-side.
//   • status `cancelled`  → rejected. status `received` → rejected.
//   • quantities are decimal strings parsed into `Decimal`.
//   • status is recomputed by the backend from summed quantities; the client
//     must never assume the next status.

export interface ReceiveLineInput {
  po_line_id: string;
  received_qty: string;
}

export type QtyError = "not-a-number" | "negative" | "exceeds-remaining";

/**
 * Validate one "receive now" entry against its line.
 * Zero is valid — it means the line is skipped, matching the backend.
 */
export function validateReceiveQty(raw: string, line: LineLike): QtyError | null {
  const trimmed = raw.trim();
  if (trimmed === "") return null; // empty is treated as zero → skipped
  const n = Number(trimmed);
  if (!Number.isFinite(n)) return "not-a-number";
  if (n < 0) return "negative";
  if (n > outstanding(line)) return "exceeds-remaining";
  return null;
}

/** Lines that will actually be sent: positive, valid quantities only. */
export function receivePayload(
  entries: Record<string, string>,
  lines: (LineLike & { po_line_id: string })[],
): ReceiveLineInput[] {
  const out: ReceiveLineInput[] = [];
  for (const line of lines) {
    const raw = (entries[line.po_line_id] ?? "").trim();
    if (raw === "") continue;
    const n = Number(raw);
    if (!Number.isFinite(n) || n <= 0) continue;
    if (n > outstanding(line)) continue; // the server would reject the batch
    out.push({ po_line_id: line.po_line_id, received_qty: raw });
  }
  return out;
}

/** Prefill: every line's full outstanding quantity. */
export function fillAllRemaining(
  lines: (LineLike & { po_line_id: string })[],
): Record<string, string> {
  return Object.fromEntries(lines.map(l => [l.po_line_id, String(outstanding(l))]));
}

/** True when at least one line carries a positive, valid quantity. */
export function hasReceivableInput(
  entries: Record<string, string>,
  lines: (LineLike & { po_line_id: string })[],
): boolean {
  return receivePayload(entries, lines).length > 0;
}
