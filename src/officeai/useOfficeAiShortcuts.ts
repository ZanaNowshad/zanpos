import { useEffect } from "react";
import type { MutableRefObject } from "react";
import type { OfficeTab } from "./officeAiTypes";
import { entryTabForDomain } from "../navigation/config";
import type { NavDomain } from "../navigation/config";

/**
 * Keyboard shortcuts for the Command shell.
 *
 * Split out of OfficeAIPage for size. Two behaviours are worth keeping visible:
 * a pending AI confirmation owns the keyboard outright, because a stray Escape
 * or digit must never dismiss or navigate away from a mutation awaiting
 * approval; and the first Escape only blurs a focused field, so a cashier
 * mid-entry does not lose the whole workspace to one keypress.
 */
export function useOfficeAiShortcuts(options: {
  chatStateRef: MutableRefObject<string>;
  commandOpen: boolean;
  dockOpen: boolean;
  /** Rail order, so Alt+1..9 addresses exactly what the operator can see. */
  navDomains: NavDomain[];
  setCommandOpen: (open: boolean) => void;
  setDockOpen: (update: (open: boolean) => boolean) => void;
  onOpenTab: (tab: OfficeTab) => void;
  onBackToPOS: () => void;
  focusComposer: () => void;
}): void {
  const {
    chatStateRef, commandOpen, dockOpen, navDomains,
    setCommandOpen, setDockOpen, onOpenTab, onBackToPOS, focusComposer,
  } = options;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (chatStateRef.current === "confirm") return;
      if (e.key === "Escape") {
        if (commandOpen) {
          e.preventDefault();
          setCommandOpen(false);
          return;
        }
        if (dockOpen) {
          e.preventDefault();
          setDockOpen(() => false);
          return;
        }
        const active = document.activeElement as HTMLElement | null;
        if (active && ["TEXTAREA", "INPUT", "SELECT"].includes(active.tagName)) {
          active.blur();
          return;
        }
        e.preventDefault();
        onBackToPOS();
        return;
      }
      if (e.ctrlKey && e.key === "/") {
        e.preventDefault();
        setDockOpen(d => !d);
        window.setTimeout(focusComposer, 120);
        return;
      }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setCommandOpen(true);
        return;
      }
      // Alt+1..9 addresses exactly the rail items that are visible. The old
      // Ctrl+1..4 bound to a four-entry model the user could not see.
      if (e.altKey && !e.ctrlKey && !e.metaKey && e.key >= "1" && e.key <= "9") {
        const idx = parseInt(e.key, 10) - 1;
        if (idx < navDomains.length) {
          e.preventDefault();
          onOpenTab(entryTabForDomain(navDomains[idx]));
        }
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [chatStateRef, commandOpen, dockOpen, navDomains, setCommandOpen, setDockOpen, onOpenTab, onBackToPOS, focusComposer]);
}
