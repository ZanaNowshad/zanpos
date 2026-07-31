-- Bind AI approvals and bulk runs to the authenticated branch that created them.
-- Existing rows intentionally receive an empty branch and therefore cannot be
-- executed by any authenticated branch after this migration.
ALTER TABLE ai_actions ADD COLUMN branch_id TEXT NOT NULL DEFAULT '';
ALTER TABLE ai_runs ADD COLUMN branch_id TEXT NOT NULL DEFAULT '';

CREATE INDEX idx_ai_actions_branch ON ai_actions(branch_id);
CREATE INDEX idx_ai_runs_branch ON ai_runs(branch_id);
