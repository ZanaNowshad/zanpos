import { useState } from "react";
import { MessageSquarePlus, History, Trash2, X, Pencil, Check } from "lucide-react";
import type { AiConversation } from "../types";
import { useLanguage } from "../hooks/useLanguage";
import { officeAiFormat, officeAiTranslator } from "../i18n/officeAiStrings";
import "./conversations.css";

/**
 * Starting a new chat, and getting back to an old one.
 *
 * What this replaces was a text link reading "· Clear chat", the same size and
 * weight as "· Export", sitting under the composer — and its single behaviour
 * was to delete every message the operator had ever exchanged with ZanAI, for
 * the whole branch, with no confirmation and no way back. It was simultaneously
 * too easy to press by accident and the only way to start a fresh subject.
 *
 * Those are two different wants and they are now two different controls. **New
 * chat** keeps everything and opens a clean thread. **History** lists the
 * threads to reopen. Deleting is per-thread, archives rather than destroys, and
 * asks first.
 */

function relativeTime(iso: string | null, t: ReturnType<typeof officeAiTranslator>): string {
  if (!iso) return "";
  const then = new Date(iso.replace(" ", "T")).getTime();
  if (Number.isNaN(then)) return "";
  const minutes = Math.max(0, Math.round((Date.now() - then) / 60000));
  if (minutes < 1) return t("justNow");
  if (minutes < 60) return officeAiFormat(t("minutesAgo"), { count: minutes });
  const hours = Math.round(minutes / 60);
  if (hours < 24) return officeAiFormat(t("hoursAgo"), { count: hours });
  return officeAiFormat(t("daysAgo"), { count: Math.round(hours / 24) });
}

function ConversationRow({ conversation, active, onOpen, onDelete, onRename }: {
  conversation: AiConversation;
  active: boolean;
  onOpen: () => void;
  onDelete: () => void;
  onRename: (title: string) => void;
}) {
  const { language } = useLanguage();
  const t = officeAiTranslator(language);
  const [renaming, setRenaming] = useState(false);
  const [draft, setDraft] = useState(conversation.title);
  // Deleting is one tap away from opening, so the row asks before it acts.
  const [confirming, setConfirming] = useState(false);

  if (renaming) {
    return (
      <li className="oa-conv-row oa-conv-row-editing">
        <input
          className="oa-conv-rename"
          value={draft}
          autoFocus
          aria-label={t("renameConversation")}
          onChange={event => setDraft(event.target.value)}
          onKeyDown={event => {
            if (event.key === "Enter") { onRename(draft); setRenaming(false); }
            if (event.key === "Escape") { setDraft(conversation.title); setRenaming(false); }
          }}
        />
        <button
          className="oa-conv-icon"
          onClick={() => { onRename(draft); setRenaming(false); }}
          aria-label={t("save")}
        >
          <Check size={14} />
        </button>
      </li>
    );
  }

  return (
    <li className={`oa-conv-row${active ? " oa-conv-row-active" : ""}`}>
      <button className="oa-conv-open" onClick={onOpen}>
        <span className="oa-conv-title">{conversation.title || t("untitledConversation")}</span>
        <span className="oa-conv-meta">
          {relativeTime(conversation.last_message_at, t)}
          {" · "}
          {officeAiFormat(t("messageCount"), { count: conversation.message_count })}
        </span>
      </button>
      {confirming ? (
        <span className="oa-conv-confirm">
          <button className="oa-conv-danger" onClick={onDelete}>{t("delete")}</button>
          <button className="oa-conv-icon" onClick={() => setConfirming(false)} aria-label={t("close")}>
            <X size={14} />
          </button>
        </span>
      ) : (
        <span className="oa-conv-actions">
          <button
            className="oa-conv-icon"
            onClick={() => { setDraft(conversation.title); setRenaming(true); }}
            aria-label={t("renameConversation")}
          >
            <Pencil size={13} />
          </button>
          <button
            className="oa-conv-icon"
            onClick={() => setConfirming(true)}
            aria-label={t("deleteConversation")}
          >
            <Trash2 size={13} />
          </button>
        </span>
      )}
    </li>
  );
}

interface Props {
  conversations: AiConversation[];
  activeId: string | null;
  loading: boolean;
  onNew: () => void;
  onOpen: (id: string) => void;
  onDelete: (id: string) => void;
  onRename: (id: string, title: string) => void;
}

export default function ConversationBar({
  conversations, activeId, loading, onNew, onOpen, onDelete, onRename,
}: Props) {
  const { language } = useLanguage();
  const t = officeAiTranslator(language);
  const [open, setOpen] = useState(false);

  return (
    <div className="oa-conv-bar">
      <div className="oa-conv-controls">
        <button className="oa-conv-new" onClick={() => { onNew(); setOpen(false); }}>
          <MessageSquarePlus size={15} />
          {t("newChat")}
        </button>
        <button
          className={`oa-conv-history${open ? " oa-conv-history-open" : ""}`}
          onClick={() => setOpen(value => !value)}
          aria-expanded={open}
        >
          <History size={15} />
          {t("chatHistory")}
          {conversations.length > 0 && (
            <span className="oa-conv-count">{conversations.length}</span>
          )}
        </button>
      </div>

      {open && (
        <div className="oa-conv-list" role="region" aria-label={t("chatHistory")}>
          {loading && <p className="oa-conv-empty">{t("loadingConversations")}</p>}
          {!loading && conversations.length === 0 && (
            <p className="oa-conv-empty">{t("noConversationsYet")}</p>
          )}
          <ul>
            {conversations.map(conversation => (
              <ConversationRow
                key={conversation.conversation_id}
                conversation={conversation}
                active={conversation.conversation_id === activeId}
                onOpen={() => { onOpen(conversation.conversation_id); setOpen(false); }}
                onDelete={() => onDelete(conversation.conversation_id)}
                onRename={title => onRename(conversation.conversation_id, title)}
              />
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
