import { useCallback, useState } from "react";

export interface CustomItemSuggestion { id: string; name: string; price: string }

const STORAGE_KEY = "zanpos_custom_suggestions";

function read(): CustomItemSuggestion[] {
  // A corrupt or absent entry must not take the till down on boot, so a parse
  // failure degrades to "no suggestions" rather than throwing.
  try { return JSON.parse(localStorage.getItem(STORAGE_KEY) || "[]"); } catch { return []; }
}

/**
 * Saved custom-item suggestions, held in localStorage.
 *
 * The modal that writes them is unmounted while the cashier uses them, so the
 * list is re-read on demand rather than kept in sync — `refresh` is called when
 * that modal closes.
 */
export function useCustomItemSuggestions() {
  const [suggestions, setSuggestions] = useState<CustomItemSuggestion[]>(read);
  const refreshSuggestions = useCallback(() => setSuggestions(read()), []);
  return { suggestions, refreshSuggestions };
}
