import { formatMoney } from "../money";
import type { Cart, SessionUser, Shift, SyncStatus } from "../types";
import type { PosAiContext, PosAiContextLine } from "./zanAiTypes";

const MAX_CONTEXT_LINES = 40;

export interface BuildPosAiContextInput {
  cart: Cart;
  shift: Shift;
  user: Pick<SessionUser, "user_id" | "display_name" | "branch_id">;
  branchName: string;
  deviceId: string;
  netTotalMinor: number;
  taxTotalMinor: number;
  syncStatus: SyncStatus | null;
  capturedAt: string;
}

export function buildPosAiContext(input: BuildPosAiContextInput): PosAiContext {
  const activeLines = input.cart.lines.filter(line => !line.voided);
  const copiedLines: PosAiContextLine[] = activeLines.slice(0, MAX_CONTEXT_LINES).map(line => ({
    product_id: line.product_id,
    name: line.product_name,
    barcode: line.barcode,
    quantity: line.quantity,
    unit_price_minor: line.unit_price_minor,
    line_total_minor: line.line_total_minor,
  }));
  const discountMinor = activeLines.reduce(
    (total, line) => total + line.line_discount_minor,
    input.cart.bill_discount_minor,
  );
  const truncatedLineCount = activeLines.length - copiedLines.length;

  return {
    surface: "pos",
    captured_at: input.capturedAt,
    branch: { id: input.user.branch_id, name: input.branchName },
    device: { id: input.deviceId },
    operator: { id: input.user.user_id, display_name: input.user.display_name },
    shift: { id: input.shift.shift_id, opened_at: input.shift.opened_at },
    cart: {
      item_count: activeLines.length,
      subtotal_minor: input.netTotalMinor - input.taxTotalMinor + discountMinor,
      discount_minor: discountMinor,
      tax_minor: input.taxTotalMinor,
      total_minor: input.netTotalMinor,
      lines: copiedLines,
      ...(truncatedLineCount > 0 ? { truncated_line_count: truncatedLineCount } : {}),
    },
    connection: {
      online: input.syncStatus?.online ?? false,
      pending_sync_count: input.syncStatus?.pending_events ?? null,
    },
  };
}

export function summarizePosAiContext(context: PosAiContext): string {
  return `Till · ${context.cart.item_count} lines · BHD ${formatMoney(context.cart.total_minor)}`;
}
