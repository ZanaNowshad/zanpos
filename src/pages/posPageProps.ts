import type { AiHandoff, ProductPrefill, SessionUser, Shift } from "../types";
import type { Theme } from "../hooks/useTheme";

/**
 * What the till needs from the app shell.
 *
 * `shift` is required, not optional: PosPage is only ever mounted with one
 * open, and the whole page assumes a shift to attribute the sale to. The
 * OfficeAI callbacks are optional because a cashier account has no back office
 * to open — the buttons that use them are hidden for that role.
 */
export interface PosPageProps {
  sessionUser: SessionUser;
  shift: Shift;
  onLogout: () => void;
  onLock?: () => void;
  onShiftClose: (closed: boolean) => void;
  onOpenOfficeAI?: (prefill?: ProductPrefill) => void;
  /** Open OfficeAI's assistant and send this message (and image) to the AI. */
  onAskOfficeAI?: (handoff: AiHandoff) => void;
  theme?: Theme;
  onToggleTheme?: () => void;
}
