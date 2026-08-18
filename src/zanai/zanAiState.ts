import type {
  ZanAiRuntimeIdentity,
  ZanAiSurfaceContext,
  ZanAiUiAction,
  ZanAiUiState,
} from "./zanAiTypes";

export function sameRuntimeIdentity(
  left: ZanAiRuntimeIdentity,
  right: ZanAiRuntimeIdentity,
): boolean {
  return (
    left.sessionToken === right.sessionToken &&
    left.userId === right.userId &&
    left.branchId === right.branchId
  );
}

export function reduceZanAiUiState(state: ZanAiUiState, action: ZanAiUiAction): ZanAiUiState {
  switch (action.type) {
    case "set_surface":
      return { ...state, activeSurface: action.surface };
    case "open_widget":
      return { ...state, widgetOpen: true, unreadCount: 0 };
    case "minimize_widget":
      return { ...state, widgetOpen: false };
    case "assistant_result":
      return state.activeSurface === "pos" && !state.widgetOpen
        ? { ...state, unreadCount: state.unreadCount + 1 }
        : state;
  }
}

export function selectSendContext(
  registered: ZanAiSurfaceContext,
  explicit?: ZanAiSurfaceContext,
): ZanAiSurfaceContext {
  return explicit ?? registered;
}

export function serializeSurfaceContext(context: ZanAiSurfaceContext): string {
  if (context.structured === undefined) return context.summary;
  return JSON.stringify({
    surface: context.surface,
    summary: context.summary,
    context: context.structured,
  });
}
