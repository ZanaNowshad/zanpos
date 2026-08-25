-- Conversations, as a person means the word.
--
-- `ai_sessions` was never one. `ai_chat_stream` mints `Ulid::new()` on every
-- message, so that table holds one row per *request* — which is right for what
-- it does (usage, tokens, cost, latency, and `ai_usage_log` points at it) and
-- useless for grouping a chat. `ai_chat_messages.session_id` inherited the same
-- meaning: it identifies one exchange, not one thread.
--
-- The visible cost was in `load_history`, which took the last thirty messages
-- for a user regardless of what they belonged to. Yesterday's VAT question and
-- this morning's stock count arrived in the model's context together, and there
-- was no way to look at either on its own, reopen one, or start a clean one —
-- the only control was a text link that deleted the lot.
--
-- So this is the thread. `ai_sessions` keeps doing its accounting job untouched.
CREATE TABLE ai_conversations (
    conversation_id TEXT PRIMARY KEY,
    branch_id       TEXT NOT NULL,
    user_id         TEXT NOT NULL,
    -- Taken from the first thing the operator said, trimmed to a line. Editable
    -- afterwards, because "check the milk price" is a fine name for a thread and
    -- a terrible one for the thread it turned into.
    title           TEXT NOT NULL DEFAULT '',
    -- Denormalised so the list renders from one query. A list that counts
    -- messages per row does a scan per row, and this one is opened from a till.
    message_count   INTEGER NOT NULL DEFAULT 0,
    last_message_at TEXT,
    -- Soft. A thread the operator closed is out of their way, but the audit
    -- trail of what the AI was asked to do should not be erasable by tidying up.
    -- `ai_delete_conversation` sets this; only clearing everything truly deletes.
    archived_at     TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

-- The list query: this operator's live threads, most recent first.
CREATE INDEX idx_ai_conversations_owner
    ON ai_conversations(branch_id, user_id, archived_at, last_message_at DESC);

ALTER TABLE ai_chat_messages ADD COLUMN conversation_id TEXT;
CREATE INDEX idx_ai_chat_messages_conversation
    ON ai_chat_messages(conversation_id, id);

-- Everything said before this migration becomes one thread per user, rather
-- than vanishing from a UI that now only knows how to show threads. It is named
-- for what it is; the messages themselves are untouched and keep their order.
INSERT INTO ai_conversations
    (conversation_id, branch_id, user_id, title, message_count, last_message_at,
     created_at, updated_at)
SELECT
    'conv-legacy-' || branch_id || '-' || user_id,
    branch_id,
    user_id,
    'Earlier conversation',
    COUNT(*),
    MAX(created_at),
    MIN(created_at),
    MAX(created_at)
FROM ai_chat_messages
GROUP BY branch_id, user_id;

UPDATE ai_chat_messages
SET conversation_id = 'conv-legacy-' || branch_id || '-' || user_id
WHERE conversation_id IS NULL;
