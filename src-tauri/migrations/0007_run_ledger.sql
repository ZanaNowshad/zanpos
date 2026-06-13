-- AI bulk-operation Run ledger: durable, checkpointed, reversible tasks.
CREATE TABLE ai_runs (
    run_id            TEXT PRIMARY KEY,
    op_id             TEXT NOT NULL,
    selector_json     TEXT NOT NULL,
    params_json       TEXT NOT NULL,
    status            TEXT NOT NULL DEFAULT 'previewing',
    total_count       INTEGER NOT NULL DEFAULT 0,
    done_count        INTEGER NOT NULL DEFAULT 0,
    checkpoint_cursor TEXT,
    error             TEXT,
    created_by        TEXT NOT NULL,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL
);
CREATE INDEX idx_ai_runs_status ON ai_runs(status);

CREATE TABLE ai_run_undo_log (
    entry_id     TEXT PRIMARY KEY,
    run_id       TEXT NOT NULL REFERENCES ai_runs(run_id),
    batch_seq    INTEGER NOT NULL,
    reverse_json TEXT NOT NULL,
    applied      INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL
);
CREATE INDEX idx_ai_run_undo_run ON ai_run_undo_log(run_id);
