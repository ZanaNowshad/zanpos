export interface WidgetRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface ViewportSize {
  width: number;
  height: number;
}

export interface WidgetVisibilityState {
  preferredOpen: boolean;
  visible: boolean;
}

const MIN_WIDTH = 360;
const MIN_HEIGHT = 420;
const DEFAULT_WIDTH = 420;
const DEFAULT_HEIGHT = 560;
const EDGE_GAP = 24;

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.min(Math.max(value, minimum), maximum);
}

export function clampWidgetRect(candidate: WidgetRect, viewport: ViewportSize): WidgetRect {
  const maxWidth = Math.max(MIN_WIDTH, Math.floor(viewport.width * 0.7));
  const maxHeight = Math.max(MIN_HEIGHT, Math.floor(viewport.height * 0.7));
  const valid = Object.values(candidate).every(Number.isFinite);
  const source = valid ? candidate : {
    width: DEFAULT_WIDTH,
    height: DEFAULT_HEIGHT,
    x: viewport.width - DEFAULT_WIDTH - EDGE_GAP,
    y: viewport.height - DEFAULT_HEIGHT - EDGE_GAP,
  };
  const width = clamp(source.width, MIN_WIDTH, maxWidth);
  const height = clamp(source.height, MIN_HEIGHT, maxHeight);

  return {
    x: clamp(source.x, 0, Math.max(0, viewport.width - width)),
    y: clamp(source.y, 0, Math.max(0, viewport.height - height)),
    width,
    height,
  };
}

export function applyWidgetSuppression(
  state: WidgetVisibilityState,
  suppressed: boolean,
): WidgetVisibilityState {
  return { ...state, visible: suppressed ? false : state.preferredOpen };
}
