/**
 * Shared types and localStorage helpers for sticky notes + reminders.
 * Used by StickyNotesPanel, useReminderChecker, and App.
 */

export type NoteColor = "yellow" | "green" | "blue" | "pink" | "purple";

export interface StickyNote {
  id: string;
  text: string;
  color: NoteColor;
  createdAt: number;
  /** Unix ms timestamp when the reminder should fire. null / absent = no reminder. */
  reminderAt?: number | null;
  /** True after the user dismisses the reminder popup. */
  reminderFired?: boolean;
  /** If the user snoozed, reminder re-fires after this timestamp. */
  reminderSnoozedUntil?: number;
}

export const NOTES_STORAGE_KEY = "zanpos_sticky_notes";

export function loadNotes(): StickyNote[] {
  try {
    const raw = localStorage.getItem(NOTES_STORAGE_KEY);
    return raw ? (JSON.parse(raw) as StickyNote[]) : [];
  } catch {
    return [];
  }
}

export function saveNotes(notes: StickyNote[]): void {
  localStorage.setItem(NOTES_STORAGE_KEY, JSON.stringify(notes));
}

/** Patch a single note in localStorage without touching the others. */
export function patchNote(id: string, patch: Partial<StickyNote>): void {
  const updated = loadNotes().map(n => (n.id === id ? { ...n, ...patch } : n));
  saveNotes(updated);
}

/** Format a Unix ms timestamp into a human-readable short string. */
export function formatReminderTime(ts: number): string {
  return new Date(ts).toLocaleString([], {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** Format a Date for datetime-local without converting it to UTC. */
export function toLocalDateTimeInput(date: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

export function tomorrowMorningInput(from: Date): string {
  const next = new Date(from);
  next.setDate(next.getDate() + 1);
  next.setHours(9, 0, 0, 0);
  return toLocalDateTimeInput(next);
}

/** Default datetime-local string: tomorrow at 09:00. */
export function defaultReminderInput(): string {
  return tomorrowMorningInput(new Date());
}
