import React, { useState, useRef, useEffect } from "react";
import type { MigrationContext, MigrationChatResponse, MigrationPreview } from "../types";
import { migrationAgentChat, migrationConfirmExecute } from "../tauri/commands";
import { open as openFilePicker } from "@tauri-apps/plugin-dialog";

// ─── Types ────────────────────────────────────────────────────────────────────

type MigrationPhase = "discovery" | "connected" | "mapping" | "preview" | "executing" | "done";

interface DisplayMessage {
  id: string;
  role: "user" | "assistant" | "system";
  text: string;
}

interface Props {
  onDone: () => void;
}

// ─── Phase config ──────────────────────────────────────────────────────────────

const PHASES: { id: MigrationPhase; label: string }[] = [
  { id: "discovery", label: "Discovery" },
  { id: "connected", label: "Connect" },
  { id: "mapping",   label: "Mapping" },
  { id: "preview",   label: "Preview" },
  { id: "executing", label: "Confirm" },
  { id: "done",      label: "Done" },
];

const PHASE_ORDER: MigrationPhase[] = ["discovery", "connected", "mapping", "preview", "executing", "done"];

// ─── Initial welcome message ──────────────────────────────────────────────────

const WELCOME: DisplayMessage = {
  id: "welcome",
  role: "assistant",
  text: `👋 Welcome to the **ZANPOS Migration Agent**!

I'll help you import your existing POS data into ZANPOS. Here's what I can migrate:
- 📦 Products & Categories
- 👤 Customers
- 🛒 Sales history
- 📦 Stock levels / Inventory

I support **SQLite**, **MySQL/MariaDB**, **SQL Server**, and **CSV/Excel** files.

To get started, tell me about your old POS system:
- What software were you using? (e.g., "Square POS", "custom SQLite database")
- Is the old POS software currently running on this computer?
- Or if you know the database file path, just paste it here.

Type your answer below, or attach a CSV/Excel file using the 📎 button.`,
};

// ─── Simple markdown renderer ─────────────────────────────────────────────────

function SimpleMarkdown({ text }: { text: string }) {
  // Very lightweight: bold (**text**), inline code (`code`), newlines → <br>
  const parts = text.split(/(\*\*[^*]+\*\*|`[^`]+`)/g);
  const rendered = parts.map((part, i) => {
    if (part.startsWith("**") && part.endsWith("**")) {
      return <strong key={i}>{part.slice(2, -2)}</strong>;
    }
    if (part.startsWith("`") && part.endsWith("`")) {
      return (
        <code
          key={i}
          style={{ background: "var(--border)", padding: "1px 4px", borderRadius: 3, fontSize: "0.85em" }}
        >
          {part.slice(1, -1)}
        </code>
      );
    }
    // Split on newlines and insert <br>
    return part.split("\n").map((line, j, arr) => (
      <React.Fragment key={`${i}-${j}`}>
        {line}
        {j < arr.length - 1 && <br />}
      </React.Fragment>
    ));
  });
  return <>{rendered}</>;
}

// ─── Migration Preview Card ───────────────────────────────────────────────────

function MigrationPreviewCard({
  preview,
  description,
  onConfirm,
  executing,
}: {
  preview: MigrationPreview;
  description: string;
  onConfirm: () => void;
  executing: boolean;
}) {
  const hasSkipped = preview.tables.some((t) => t.skipped > 0);
  return (
    <div className="mig-preview-card">
      <div className="mig-preview-title">🗂️ Migration Preview</div>
      {description && (
        <p style={{ margin: "0 0 10px", fontSize: "0.85rem", color: "var(--text-dim)" }}>
          {description}
        </p>
      )}

      <table className="mig-preview-table">
        <thead>
          <tr>
            <th>Table</th>
            <th style={{ textAlign: "right" }}>Rows</th>
            {hasSkipped && <th style={{ textAlign: "right" }}>Skipped</th>}
          </tr>
        </thead>
        <tbody>
          {preview.tables.map((t, i) => (
            <tr key={i}>
              <td>{t.target_table}</td>
              <td style={{ textAlign: "right", fontWeight: 600 }}>{t.rows}</td>
              {hasSkipped && (
                <td style={{ textAlign: "right", color: "#f59e0b" }}>
                  {t.skipped > 0 ? t.skipped : "—"}
                </td>
              )}
            </tr>
          ))}
        </tbody>
      </table>

      <div className="mig-preview-total">
        Total: {preview.total_rows} row{preview.total_rows !== 1 ? "s" : ""} to import
      </div>

      {preview.warnings.map((w, i) => (
        <div key={i} className="mig-warning-badge">
          ⚠️ {w}
        </div>
      ))}

      <button className="mig-confirm-btn" onClick={onConfirm} disabled={executing}>
        {executing ? "⏳ Migrating…" : "✅ CONFIRM — Execute Migration"}
      </button>
      <p style={{ margin: "8px 0 0", fontSize: "0.75rem", color: "var(--text-dim)", textAlign: "center" }}>
        This will INSERT data into your ZANPOS database. Existing data is not modified.
      </p>
    </div>
  );
}

// ─── Main Component ───────────────────────────────────────────────────────────

export default function MigrationAgentPage({ onDone }: Props) {
  const [messages, setMessages] = useState<DisplayMessage[]>([WELCOME]);
  const [input, setInput] = useState("");
  const [loading, setLoading] = useState(false);
  const [context, setContext] = useState<MigrationContext>({});
  const [phase, setPhase] = useState<MigrationPhase>("discovery");
  const [pendingScript, setPendingScript] = useState<string | null>(null);
  const [pendingPreview, setPendingPreview] = useState<MigrationPreview | null>(null);
  const [pendingDescription, setPendingDescription] = useState("");
  const [executing, setExecuting] = useState(false);

  const chatEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  // Build history for backend (role + content only)
  const history = messages
    .filter((m) => m.role !== "system")
    .map((m) => ({ role: m.role === "user" ? "user" as const : "assistant" as const, content: m.text }));

  // Auto-scroll to bottom
  useEffect(() => {
    chatEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, loading]);

  const addMessage = (msg: Omit<DisplayMessage, "id">) => {
    setMessages((prev) => [...prev, { ...msg, id: Math.random().toString(36).slice(2) }]);
  };

  // Advance phase based on response content
  const inferPhase = (text: string, currentPhase: MigrationPhase): MigrationPhase => {
    const lower = text.toLowerCase();
    if (
      currentPhase === "discovery" &&
      (lower.includes("connected") ||
        lower.includes("connect ok") ||
        lower.includes("sqlite file ok") ||
        (lower.includes("mysql") && lower.includes("ok")))
    )
      return "connected";
    if (
      currentPhase === "connected" &&
      (lower.includes("schema") || lower.includes("tables") || lower.includes("mapping"))
    )
      return "mapping";
    if (currentPhase === "mapping" && lower.includes("preview")) return "preview";
    return currentPhase;
  };

  // ── Send message ──────────────────────────────────────────────────────────
  const handleSend = async () => {
    const msg = input.trim();
    if (!msg || loading) return;

    setInput("");
    setLoading(true);
    addMessage({ role: "user", text: msg });

    try {
      const response: MigrationChatResponse = await migrationAgentChat({
        history,
        message: msg,
        user_id: "migration_user",
        context,
      });

      if (response.type === "NoApiKey") {
        addMessage({
          role: "system",
          text: "⚠️ No AI provider configured. Please set up an AI API key in Back Office → AI Settings first.",
        });
      } else if (response.type === "PendingMigration") {
        setPendingScript(response.script);
        setPendingPreview(response.preview);
        setPendingDescription(response.description);
        setPhase("preview");
        addMessage({
          role: "assistant",
          text:
            response.description ||
            "I've analyzed your data and prepared a migration script. Please review the preview below and click CONFIRM to proceed.",
        });
      } else {
        const newPhase = inferPhase(response.content, phase);
        if (newPhase !== phase) setPhase(newPhase);
        addMessage({ role: "assistant", text: response.content });
      }
    } catch (err) {
      addMessage({ role: "system", text: `Error: ${String(err)}` });
    } finally {
      setLoading(false);
    }
  };

  // ── Confirm migration ─────────────────────────────────────────────────────
  const handleConfirm = async () => {
    if (!pendingScript) return;
    setExecuting(true);
    setPhase("executing");

    try {
      const result = await migrationConfirmExecute(pendingScript, "migration_user");
      setPhase("done");
      setPendingScript(null);
      setPendingPreview(null);
      addMessage({ role: "assistant", text: result });
      addMessage({
        role: "system",
        text: "✅ Migration complete! You can now start using ZANPOS with your imported data.",
      });
    } catch (err) {
      setPhase("preview");
      addMessage({ role: "system", text: `❌ Migration failed: ${String(err)}` });
    } finally {
      setExecuting(false);
    }
  };

  // ── File attachment ───────────────────────────────────────────────────────
  const handleAttach = async () => {
    try {
      const selected = await openFilePicker({
        multiple: false,
        filters: [
          {
            name: "Database / Spreadsheet",
            extensions: ["db", "sqlite", "sqlite3", "csv", "xlsx", "xls"],
          },
          { name: "All Files", extensions: ["*"] },
        ],
      });
      // selected is null | string | string[] in Tauri v2
      if (selected === null || selected === undefined) return;
      const filePath = Array.isArray(selected) ? selected[0] : selected;
      if (!filePath) return;

      const ext = filePath.split(".").pop()?.toLowerCase() ?? "";
      const dbType: MigrationContext["db_type"] =
        ext === "csv" || ext === "xlsx" || ext === "xls" ? "csv" : "sqlite";

      setContext((prev) => ({
        ...prev,
        db_type: dbType,
        attached_file_path: filePath,
        path_or_connstr: filePath,
      }));
      addMessage({
        role: "system",
        text: `📎 Attached: ${filePath}`,
      });
      // Pre-fill input to trigger analysis
      setInput(`I've attached the file: ${filePath}`);
    } catch {
      // User cancelled picker — ignore
    }
  };

  // ── Keyboard handling ─────────────────────────────────────────────────────
  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  // ── Phase stepper ─────────────────────────────────────────────────────────
  const currentPhaseIdx = PHASE_ORDER.indexOf(phase);

  return (
    <div className="mig-page">
      {/* Phase stepper header */}
      <div className="mig-header">
        <span className="mig-header-title">Migration Agent</span>
        {PHASES.map((p, i) => {
          const idx = PHASE_ORDER.indexOf(p.id);
          const isDone = idx < currentPhaseIdx;
          const isActive = idx === currentPhaseIdx;
          return (
            <React.Fragment key={p.id}>
              {i > 0 && <span className="mig-phase-sep">›</span>}
              <span
                className={`mig-phase-step${isActive ? " mig-phase-step--active" : ""}${isDone ? " mig-phase-step--done" : ""}`}
              >
                {isDone ? "✓ " : ""}
                {p.label}
              </span>
            </React.Fragment>
          );
        })}

        {/* Skip / Close button */}
        <button
          onClick={onDone}
          style={{
            marginLeft: "auto",
            padding: "4px 12px",
            background: "transparent",
            border: "1px solid var(--border)",
            borderRadius: 6,
            cursor: "pointer",
            color: "var(--text-dim)",
            fontSize: "0.8rem",
          }}
        >
          Skip → Launch POS
        </button>
      </div>

      {/* Chat area */}
      <div className="mig-chat-area">
        {messages.map((msg) => (
          <div key={msg.id} className={`mig-bubble mig-bubble--${msg.role}`}>
            <SimpleMarkdown text={msg.text} />
          </div>
        ))}

        {/* Preview card — shown inline after preview response */}
        {pendingPreview && phase === "preview" && (
          <MigrationPreviewCard
            preview={pendingPreview}
            description={pendingDescription}
            onConfirm={handleConfirm}
            executing={executing}
          />
        )}

        {/* Thinking indicator */}
        {loading && (
          <div className="mig-thinking">
            <div className="mig-dot-pulse">
              <span />
              <span />
              <span />
            </div>
            Analysing…
          </div>
        )}

        {/* Done banner */}
        {phase === "done" && (
          <div className="mig-done-banner">
            🎉 Your data has been imported successfully!
            <br />
            <button className="mig-launch-btn" style={{ marginTop: 12 }} onClick={onDone}>
              Launch POS 🚀
            </button>
          </div>
        )}

        <div ref={chatEndRef} />
      </div>

      {/* Input bar */}
      {phase !== "done" && (
        <div className="mig-input-bar">
          <button className="mig-attach-btn" onClick={handleAttach} title="Attach CSV/Excel/SQLite file">
            📎
          </button>
          <textarea
            ref={inputRef}
            className="mig-input"
            placeholder={
              phase === "preview"
                ? "Ask a question about the migration, or click CONFIRM above…"
                : "Describe your old POS system or paste a file path…"
            }
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            rows={1}
            disabled={loading || executing}
          />
          <button
            className="mig-send-btn"
            onClick={handleSend}
            disabled={loading || executing || !input.trim()}
          >
            {loading ? "…" : "Send"}
          </button>
        </div>
      )}
    </div>
  );
}
