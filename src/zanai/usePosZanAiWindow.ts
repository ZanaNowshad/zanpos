import { useCallback, useEffect, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import { clampLauncherPosition, clampWidgetRect, type LauncherPosition, type WidgetRect } from "./widgetState";

const DEFAULT_RECT: WidgetRect = { x: 0, y: 0, width: 420, height: 560 };

function viewport() {
  return {
    width: typeof window === "undefined" ? 1_200 : window.innerWidth,
    height: typeof window === "undefined" ? 800 : window.innerHeight,
  };
}

function loadPreference(storageKey: string) {
  const defaultRect = clampWidgetRect(
    { ...DEFAULT_RECT, x: Number.NaN, y: Number.NaN },
    viewport(),
  );
  if (typeof localStorage === "undefined") return {
    preferredOpen: false,
    rect: defaultRect,
    launcher: clampLauncherPosition({ x: Number.NaN, y: Number.NaN }, viewport()),
  };
  try {
    const stored = JSON.parse(localStorage.getItem(storageKey) ?? "null") as {
      preferredOpen?: unknown;
      rect?: WidgetRect;
      launcher?: LauncherPosition;
    } | null;
    return {
      preferredOpen: stored?.preferredOpen === true,
      rect: clampWidgetRect(stored?.rect ?? DEFAULT_RECT, viewport()),
      launcher: clampLauncherPosition(stored?.launcher ?? { x: Number.NaN, y: Number.NaN }, viewport()),
    };
  } catch {
    return { preferredOpen: false, rect: defaultRect, launcher: clampLauncherPosition({ x: Number.NaN, y: Number.NaN }, viewport()) };
  }
}

export function usePosZanAiWindow(storageKey: string, suppressed: boolean, focusPos: () => void) {
  const [preferredOpen, setPreferredOpen] = useState(() => loadPreference(storageKey).preferredOpen);
  const [rect, setRect] = useState(() => loadPreference(storageKey).rect);
  const [launcher, setLauncher] = useState(() => loadPreference(storageKey).launcher ?? clampLauncherPosition({ x: Number.NaN, y: Number.NaN }, viewport()));
  const [expanded, setExpanded] = useState(false);
  const dragRef = useRef<{ pointerX: number; pointerY: number; rect: WidgetRect } | null>(null);
  const launcherDragRef = useRef<{ pointerX: number; pointerY: number; position: LauncherPosition } | null>(null);
  const launcherMovedRef = useRef(false);

  const visible = preferredOpen && !suppressed;

  useEffect(() => {
    try { localStorage.setItem(storageKey, JSON.stringify({ preferredOpen, rect, launcher })); } catch { /* advisory */ }
  }, [launcher, preferredOpen, rect, storageKey]);

  useEffect(() => {
    if (suppressed && preferredOpen) focusPos();
  }, [focusPos, preferredOpen, suppressed]);

  useEffect(() => {
    const onResize = () => {
      setRect(current => clampWidgetRect(current, viewport()));
      setLauncher(current => clampLauncherPosition(current, viewport()));
    };
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  useEffect(() => {
    const onMove = (event: PointerEvent) => {
      const drag = dragRef.current;
      if (drag) {
        setRect(clampWidgetRect({ ...drag.rect, x: drag.rect.x + event.clientX - drag.pointerX, y: drag.rect.y + event.clientY - drag.pointerY }, viewport()));
      }
      const launcherDrag = launcherDragRef.current;
      if (launcherDrag) {
        const dx = event.clientX - launcherDrag.pointerX;
        const dy = event.clientY - launcherDrag.pointerY;
        if (Math.abs(dx) + Math.abs(dy) > 4) launcherMovedRef.current = true;
        setLauncher(clampLauncherPosition({ x: launcherDrag.position.x + dx, y: launcherDrag.position.y + dy }, viewport()));
      }
    };
    const onUp = () => { dragRef.current = null; launcherDragRef.current = null; };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    };
  }, []);

  const beginDrag = useCallback((event: ReactPointerEvent) => {
    if (expanded) return;
    dragRef.current = { pointerX: event.clientX, pointerY: event.clientY, rect };
  }, [expanded, rect]);

  const beginLauncherDrag = useCallback((event: ReactPointerEvent) => {
    launcherMovedRef.current = false;
    launcherDragRef.current = { pointerX: event.clientX, pointerY: event.clientY, position: launcher };
  }, [launcher]);

  const openFromLauncher = useCallback(() => {
    if (launcherMovedRef.current) { launcherMovedRef.current = false; return; }
    setPreferredOpen(true);
  }, []);

  return {
    visible,
    preferredOpen,
    expanded,
    rect,
    launcher,
    open: openFromLauncher,
    minimize: () => setPreferredOpen(false),
    close: () => setPreferredOpen(false),
    toggleExpanded: () => setExpanded(value => !value),
    beginDrag,
    beginLauncherDrag,
  };
}
