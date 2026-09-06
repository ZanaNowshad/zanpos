import type { SessionToken } from "../types";
import { useCallback } from "react";
import { DEVICE } from "../types";
import { aiCancelAction, aiExecuteAction } from "../tauri/commands";

/**
 * Approve or cancel an AI action that was persisted for later review.
 *
 * Both send an identity and nothing else. The backend re-loads the row,
 * verifies the payload hash, and re-checks scope and expiry before executing —
 * so an approval cannot smuggle in different data than the one that was
 * reviewed. Keeping that property visible is why these two live together.
 */
export function usePersistedAiActions(options: {
  sessionToken: SessionToken;
  /** Run after a successful execution so the caller can refresh what it shows. */
  onApplied: () => void;
}) {
  const { sessionToken, onApplied } = options;

  const confirmPersistedAction = useCallback(async (actionId: string): Promise<boolean> => {
    try {
      await aiExecuteAction(sessionToken, {
        action_id: actionId,
        history: [],
        assistant_text: "",
        currency_exponent: DEVICE.currency_exponent,
      });
      onApplied();
      return true;
    } catch {
      return false;
    }
  }, [sessionToken, onApplied]);

  const cancelPersistedAction = useCallback(async (actionId: string): Promise<boolean> => {
    try {
      await aiCancelAction(sessionToken, actionId);
      return true;
    } catch {
      return false;
    }
  }, [sessionToken]);

  return { confirmPersistedAction, cancelPersistedAction };
}
