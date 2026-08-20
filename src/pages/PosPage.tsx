import type { PosPageProps } from "./posPageProps";
import { usePosPageState } from "./usePosPageState";
import PosPageView from "./PosPageView";

/**
 * The till.
 *
 * Deliberately thin: `usePosPageState` owns the hooks and handlers,
 * `PosPageView` owns the markup. Keeping the seam here means the render can
 * be read without scrolling past three hundred lines of wiring, and the
 * wiring can be changed without touching JSX.
 */
export default function PosPage(props: PosPageProps) {
  return <PosPageView {...usePosPageState(props)} />;
}
