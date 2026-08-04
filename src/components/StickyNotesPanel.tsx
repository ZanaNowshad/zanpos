import { useEffect, useRef, useState } from "react";
import {
  type NoteColor, type StickyNote,
  NOTES_STORAGE_KEY, loadNotes, saveNotes,
  formatReminderTime, defaultReminderInput,
  toLocalDateTimeInput, tomorrowMorningInput,
} from "../utils/stickyNotes";
import { useFocusTrap } from "../hooks/useFocusTrap";

// T19: bg/border reference CSS vars so themes can override the palette
const COLORS: { id: NoteColor; label: string; bg: string; border: string }[] = [
  { id: "yellow", label: "Yellow", bg: "var(--note-yellow-bg)", border: "var(--note-yellow-bd)" },
  { id: "green",  label: "Green",  bg: "var(--note-green-bg)",  border: "var(--note-green-bd)"  },
  { id: "blue",   label: "Blue",   bg: "var(--note-blue-bg)",   border: "var(--note-blue-bd)"   },
  { id: "pink",   label: "Pink",   bg: "var(--note-pink-bg)",   border: "var(--note-pink-bd)"   },
  { id: "purple", label: "Purple", bg: "var(--note-purple-bg)", border: "var(--note-purple-bd)" },
];

function uid() {
  return `note_${Date.now()}_${Math.random().toString(36).slice(2, 7)}`;
}

interface Props {
  onClose: () => void;
}

export default function StickyNotesPanel({ onClose }: Props) {
  const [notes, setNotes]             = useState<StickyNote[]>(loadNotes);
  const [editingId, setEditingId]     = useState<string | null>(null);
  const [draftText, setDraftText]     = useState("");
  const [draftReminderOn, setDraftReminderOn]   = useState(false);
  const [draftReminderAt, setDraftReminderAt]   = useState("");
  const [newColor, setNewColor]       = useState<NoteColor>("yellow");
  const [deleteConfirm, setDeleteConfirm] = useState<string | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  useFocusTrap(panelRef, onClose);

  // Persist every change
  useEffect(() => { saveNotes(notes); }, [notes]);

  // Keep in sync with external changes (e.g. ReminderPopup patched a note)
  useEffect(() => {
    const handler = (e: StorageEvent) => {
      if (e.key === NOTES_STORAGE_KEY) setNotes(loadNotes());
    };
    window.addEventListener("storage", handler);
    return () => window.removeEventListener("storage", handler);
  }, []);

  // Focus textarea when editing starts
  useEffect(() => {
    if (editingId && textareaRef.current) {
      textareaRef.current.focus();
      const len = textareaRef.current.value.length;
      textareaRef.current.setSelectionRange(len, len);
    }
  }, [editingId]);

  const addNote = () => {
    const note: StickyNote = {
      id: uid(),
      text: "",
      color: newColor,
      createdAt: Date.now(),
    };
    const updated = [note, ...notes];
    setNotes(updated);
    setDraftText("");
    setDraftReminderOn(false);
    setDraftReminderAt("");
    setEditingId(note.id);
  };

  const startEdit = (note: StickyNote) => {
    setDraftText(note.text);
    setDraftReminderOn(!!note.reminderAt);
    setDraftReminderAt(
      note.reminderAt
        ? toLocalDateTimeInput(new Date(note.reminderAt))
        : defaultReminderInput(),
    );
    setEditingId(note.id);
    setDeleteConfirm(null);
  };

  const commitEdit = () => {
    if (!editingId) return;
    const reminderAt = draftReminderOn && draftReminderAt
      ? new Date(draftReminderAt).getTime()
      : null;
    setNotes(prev => prev.map(n =>
      n.id === editingId
        ? {
            ...n,
            text: draftText,
            reminderAt,
            // Clear fired/snooze state if the user changed the time
            reminderFired: reminderAt ? false : undefined,
            reminderSnoozedUntil: undefined,
          }
        : n
    ));
    setEditingId(null);
  };

  const deleteNote = (id: string) => {
    setNotes(prev => prev.filter(n => n.id !== id));
    if (editingId === id) setEditingId(null);
    setDeleteConfirm(null);
  };

  const changeColor = (id: string, color: NoteColor) => {
    setNotes(prev => prev.map(n => n.id === id ? { ...n, color } : n));
  };

  const colorMeta = (c: NoteColor) => COLORS.find(x => x.id === c)!;

  // Minimum datetime value = now (can't set a reminder in the past)
  const nowInput = toLocalDateTimeInput(new Date());

  return (
  <div tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }} className="stickynotes-overlay" onClick={e => e.target === e.currentTarget && onClose()}>
      <div className="stickynotes-panel" ref={panelRef} role="dialog" aria-modal="true" aria-labelledby="stickynotes-title">

        {/* Header */}
        <div className="stickynotes-header">
          <div className="stickynotes-heading">
            <span>Register workspace</span>
            <h2 className="stickynotes-title" id="stickynotes-title">Quick notes</h2>
            <p>Capture a reminder without leaving the sale.</p>
          </div>
          <div className="stickynotes-header-actions">
            <div className="stickynotes-color-picker">
              {COLORS.map(c => (
                <button
                  key={c.id}
                  className={`stickynotes-color-dot${newColor === c.id ? " active" : ""}`}
                  style={{ background: c.bg, borderColor: c.border }}
                  onClick={() => setNewColor(c.id)}
                  title={c.label}
                  aria-label={`Use ${c.label.toLowerCase()} for new note`}
                  aria-pressed={newColor === c.id}
                />
              ))}
            </div>
            <button className="stickynotes-add-btn" onClick={addNote} title="New note">
              + New Note
            </button>
            <button className="stickynotes-close-btn" onClick={onClose} title="Close">×</button>
          </div>
        </div>

        {/* Notes grid */}
        <div className="stickynotes-grid">
          {notes.length === 0 && (
            <div className="stickynotes-empty">
              <strong>No notes yet</strong>
              <span>Capture a reminder, customer request, or shift handoff.</span>
              <button type="button" onClick={addNote}>New note</button>
            </div>
          )}

          {notes.map(note => {
            const meta      = colorMeta(note.color);
            const isEditing = editingId === note.id;
            const hasReminder = !!note.reminderAt;
            const reminderFired = !!note.reminderFired;

            return (
              <div
                key={note.id}
                className={`stickynote${isEditing ? " stickynote-editing" : ""}${hasReminder ? " stickynote-has-reminder" : ""}`}
                style={{ background: meta.bg, borderColor: meta.border }}
              >
                {/* Toolbar */}
                <div className="stickynote-toolbar">
                  <div className="stickynote-colors">
                    {COLORS.map(c => (
                      <button
                        key={c.id}
                        className={`stickynotes-color-dot stickynotes-color-dot-sm${note.color === c.id ? " active" : ""}`}
                        style={{ background: c.bg, borderColor: c.border }}
                        onClick={() => changeColor(note.id, c.id)}
                        title={c.label}
                        aria-label={`Change note color to ${c.label.toLowerCase()}`}
                        aria-pressed={note.color === c.id}
                      />
                    ))}
                  </div>
                  {deleteConfirm === note.id ? (
                    <div className="stickynote-delete-confirm">
                      <button className="stickynote-confirm-yes" onClick={() => deleteNote(note.id)}>Delete</button>
                      <button className="stickynote-confirm-no"  onClick={() => setDeleteConfirm(null)}>Cancel</button>
                    </div>
                  ) : (
                    <button className="stickynote-delete-btn" onClick={() => setDeleteConfirm(note.id)} title="Delete note">🗑</button>
                  )}
                </div>

                {/* Body — view or edit */}
                {isEditing ? (
                  <>
                    <textarea
                      ref={textareaRef}
                      className="stickynote-textarea"
                      style={{ background: meta.bg }}
                      value={draftText}
                      onChange={e => setDraftText(e.target.value)}
                      placeholder="Write your note…"
                      rows={4}
                    />

                    {/* ── Reminder section — touch-friendly presets ── */}
                    <div className="stickynote-reminder-section">
                      <button
                        type="button"
                        className={`stickynote-reminder-toggle-btn${draftReminderOn ? " active" : ""}`}
                        onClick={() => {
                          const next = !draftReminderOn;
                          setDraftReminderOn(next);
                          if (next && !draftReminderAt) setDraftReminderAt(defaultReminderInput());
                        }}
                      >
                        🔔 {draftReminderOn ? "Reminder On" : "Set Reminder"}
                      </button>
                      {draftReminderOn && (
                        <div className="stickynote-reminder-presets">
                          {[
                            { label: "In 30 min", minutes: 30 },
                            { label: "In 1 hour", minutes: 60 },
                            { label: "In 2 hours", minutes: 120 },
                            { label: "Tomorrow, 9:00", minutes: null },
                          ].map(({ label, minutes }) => (
                            <button
                              key={label}
                              type="button"
                              className="stickynote-preset-btn"
                              onClick={() => {
                                const now = new Date();
                                setDraftReminderAt(minutes === null
                                  ? tomorrowMorningInput(now)
                                  : toLocalDateTimeInput(new Date(now.getTime() + minutes * 60000)));
                              }}
                            >
                              {label}
                            </button>
                          ))}
                          <input
                            type="datetime-local"
                            className="stickynote-reminder-dt"
                            value={draftReminderAt}
                            min={nowInput}
                            onChange={e => setDraftReminderAt(e.target.value)}
                          />
                        </div>
                      )}
                    </div>
                  </>
                ) : (
                  <div role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }} 
                    className="stickynote-body"
                    onClick={() => startEdit(note)}
                    title="Click to edit"
                  >
                    {note.text || <span className="stickynote-placeholder">Click to add text…</span>}
                  </div>
                )}

                {/* Reminder badge (view mode) */}
                {!isEditing && hasReminder && (
                  <div className={`stickynote-reminder-badge${reminderFired ? " stickynote-reminder-badge-done" : ""}`}>
                    {reminderFired ? "✓" : "🔔"} {formatReminderTime(note.reminderAt!)}
                  </div>
                )}

                {/* Save button */}
                {isEditing && (
                  <button className="stickynote-save-btn" onClick={commitEdit}>
                    ✓ Done
                  </button>
                )}
              </div>
            );
          })}
        </div>

      </div>
    </div>
  );
}
