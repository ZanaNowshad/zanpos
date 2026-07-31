-- Incoming WhatsApp messages from the configured owner contact / store group,
-- surfaced in the POS notification centre. Populated by whatsapp_poll_messages
-- which pulls from the Baileys sidecar and filters to the two configured JIDs.
-- PK = the Baileys message id so re-polling the same message is a no-op
-- (INSERT OR IGNORE).
CREATE TABLE IF NOT EXISTS wa_messages (
    id          TEXT PRIMARY KEY,
    chat_jid    TEXT NOT NULL,
    chat_name   TEXT,
    is_group    INTEGER NOT NULL DEFAULT 0,
    sender_jid  TEXT,
    sender_name TEXT,
    body        TEXT NOT NULL,
    ts          INTEGER NOT NULL,
    read        INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_wa_messages_unread ON wa_messages (read, ts);
