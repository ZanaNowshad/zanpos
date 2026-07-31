/**
 * Derives whether a full-screen prompt (e.g. the critical-update modal) is
 * safe to show right now. Extracted out of PosPage.tsx so gating logic has a
 * single, independently testable source of truth instead of being inlined
 * wherever a blocking prompt might render.
 */
export interface SaleBlockingState {
  hasOpenCart: boolean;
  hasBlockingModal: boolean;
}

export function useSaleBlockingState(
  lineCount: number,
  activeModalKind: string,
): SaleBlockingState {
  return {
    hasOpenCart: lineCount > 0,
    hasBlockingModal: activeModalKind !== "none",
  };
}
