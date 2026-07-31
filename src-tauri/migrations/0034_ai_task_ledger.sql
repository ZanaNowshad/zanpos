-- AI task ledger: small persistent scratchpad the assistant uses to track
-- multi-step task progress (e.g. "importing 27 products, 12 done").
-- Survives chat restarts and errors so the model resumes instead of
-- re-discovering state through repeated searches.
CREATE TABLE IF NOT EXISTS ai_task_ledger (
    branch_id   TEXT NOT NULL,
    task_key    TEXT NOT NULL DEFAULT 'current',
    description TEXT NOT NULL,
    state_json  TEXT NOT NULL DEFAULT '{}',
    updated_at  TEXT NOT NULL,
    PRIMARY KEY (branch_id, task_key)
);
