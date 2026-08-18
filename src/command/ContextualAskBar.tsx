import { Sparkles } from "lucide-react";
import type { KeyboardEvent, ReactNode } from "react";
import "./askbar.css";

export interface AskBarSuggestion {
  label: string;
  prompt: string;
}

interface Props {
  /** What page/context is the user on. Sent as context to the AI. */
  context: string;
  /** Pre-written suggested prompts. */
  suggestions?: AskBarSuggestion[];
  /** Called when the user submits a prompt. */
  onSendPrompt: (prompt: string) => void;
  /** Placeholder text for the input. */
  placeholder?: string;
  /** Optional additional content rendered beside the ask bar. */
  children?: ReactNode;
}

/**
 * Inline contextual AI ask bar.
 *
 * Renders at the bottom of key pages (TodayDashboard, Products, Purchasing, Reports)
 * to offer contextual AI assistance without leaving the page.
 */
export default function ContextualAskBar({
  context,
  suggestions,
  onSendPrompt,
  placeholder = "Ask ZanAI about this page…",
  children,
}: Props) {
  const handleKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      const value = (e.target as HTMLInputElement).value.trim();
      if (value) {
        onSendPrompt(`[Context: ${context}] ${value}`);
        (e.target as HTMLInputElement).value = "";
      }
    }
  };

  return (
    <div className="contextual-ask-bar">
      {suggestions && suggestions.length > 0 && (
        <div className="ask-bar-suggestions">
          {suggestions.map((s, i) => (
            <button
              key={i}
              className="ask-bar-chip"
              onClick={() => onSendPrompt(`[Context: ${context}] ${s.prompt}`)}
              title={s.prompt}
            >
              <Sparkles size={12} strokeWidth={1.75} />
              <span>{s.label}</span>
            </button>
          ))}
        </div>
      )}
      <div className="ask-bar-input-row">
        <span className="ask-bar-icon" aria-hidden="true">
          <Sparkles size={16} strokeWidth={1.5} />
        </span>
        <input
          className="ask-bar-input"
          type="text"
          placeholder={placeholder}
          onKeyDown={handleKeyDown}
          aria-label="Ask ZanAI about this page"
        />
        {children}
      </div>
    </div>
  );
}
