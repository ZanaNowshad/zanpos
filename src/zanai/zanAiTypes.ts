import type { SessionToken } from "../types";

export type ZanAiSurface = "pos" | "office";

export interface ZanAiRuntimeIdentity {
  sessionToken: SessionToken;
  userId: string;
  branchId: string;
}

export interface ZanAiSurfaceContext {
  surface: ZanAiSurface;
  summary: string;
  structured?: PosAiContext;
}

export interface PosAiContextLine {
  product_id: string | null;
  name: string;
  barcode: string | null;
  quantity: string;
  unit_price_minor: number;
  line_total_minor: number;
}

export interface PosAiContext {
  surface: "pos";
  captured_at: string;
  branch: { id: string; name: string };
  device: { id: string };
  operator: { id: string; display_name: string };
  shift: { id: string; opened_at: string };
  cart: {
    item_count: number;
    subtotal_minor: number;
    discount_minor: number;
    tax_minor: number;
    total_minor: number;
    lines: PosAiContextLine[];
    truncated_line_count?: number;
  };
  connection: { online: boolean; pending_sync_count: number | null };
}

export interface ZanAiUiState {
  activeSurface: ZanAiSurface;
  widgetOpen: boolean;
  widgetExpanded: boolean;
  unreadCount: number;
}

export type ZanAiUiAction =
  | { type: "set_surface"; surface: ZanAiSurface }
  | { type: "open_widget" }
  | { type: "minimize_widget" }
  | { type: "assistant_result" };
