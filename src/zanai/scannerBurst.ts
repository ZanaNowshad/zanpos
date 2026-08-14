export interface TimedKey {
  key: string;
  at: number;
}

export type ScannerBurstResult =
  | { kind: "barcode"; value: string }
  | { kind: "text" };

const MAX_AVERAGE_GAP_MS = 35;
const BARCODE_PATTERN = /^(?:[1-9]\d{0,2}[*x])?[A-Za-z0-9-]{4,}$/;

export function classifyScannerBurst(events: TimedKey[]): ScannerBurstResult {
  if (events.length < 5 || events.at(-1)?.key !== "Enter") return { kind: "text" };
  if (events.some(event => !Number.isFinite(event.at))) return { kind: "text" };
  for (let index = 1; index < events.length; index += 1) {
    if (events[index].at < events[index - 1].at) return { kind: "text" };
  }

  const characters = events.slice(0, -1);
  if (characters.some(event => event.key.length !== 1 || /\s/.test(event.key))) {
    return { kind: "text" };
  }
  const value = characters.map(event => event.key).join("");
  if (!BARCODE_PATTERN.test(value)) return { kind: "text" };

  const elapsed = events[events.length - 1].at - events[0].at;
  const averageGap = elapsed / (events.length - 1);
  return averageGap <= MAX_AVERAGE_GAP_MS
    ? { kind: "barcode", value }
    : { kind: "text" };
}

export function removeBurstSuffix(draft: string, burst: string): string {
  return draft.endsWith(burst) ? draft.slice(0, -burst.length) : draft;
}
