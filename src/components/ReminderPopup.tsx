import { useEffect } from "react";
import type { StickyNote } from "../utils/stickyNotes";
import { formatReminderTime, patchNote } from "../utils/stickyNotes";

interface Props {
  note: StickyNote;
  /** Called when the user dismisses OR snoozes — parent advances the queue. */
  onClose: () => void;
}

export default function ReminderPopup({ note, onClose }: Props) {
  const handleDismiss = () => {
    patchNote(note.id, { reminderFired: true });
    onClose();
  };

  // Dismiss via Escape key
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") { e.preventDefault(); handleDismiss(); }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [note.id]);

  const handleSnooze = () => {
    patchNote(note.id, {
      reminderSnoozedUntil: Date.now() + 10 * 60 * 1000, // 10 minutes
    });
    onClose();
  };

  return (
    <div className="reminder-overlay">
      <div className="reminder-card" role="alertdialog" aria-modal="true" aria-labelledby="reminder-title">
        <div className="reminder-bell">🔔</div>
        <div id="reminder-title" className="reminder-label">Reminder</div>
        {note.reminderAt && (
          <div className="reminder-time">{formatReminderTime(note.reminderAt)}</div>
        )}
        <div className="reminder-text">{note.text || <em>No note text</em>}</div>
        <div className="reminder-actions">
          <button className="reminder-snooze-btn" onClick={handleSnooze}>
            ⏱ Snooze 10 min
          </button>
          <button className="reminder-dismiss-btn" onClick={handleDismiss}>
            ✓ Dismiss
          </button>
        </div>
      </div>
    </div>
  );
}
