export type ZanAiSurface = "pos" | "office";

export interface ZanAiRuntimeIdentity {
  sessionToken: string;
  userId: string;
  branchId: string;
}

export interface ZanAiSurfaceContext {
  surface: ZanAiSurface;
  summary: string;
  structured?: unknown;
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
