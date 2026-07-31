-- Trust spine: passive crash/error diagnostics log and local analytics event
-- queue. Both tables are upload-ready (uploaded_at marks the row as flushed
-- to the storefront worker's telemetry endpoint); nothing writes to
-- analytics_events yet in this pass — no task instruments event-tracking
-- call sites, so it stays empty until a future pass adds writers.
CREATE TABLE diagnostics (
    id TEXT PRIMARY KEY NOT NULL,
    ts TEXT NOT NULL,
    device_id TEXT NOT NULL,
    severity TEXT NOT NULL CHECK(severity IN ('panic','error','warn')),
    kind TEXT NOT NULL,
    message TEXT NOT NULL,
    stack TEXT,
    app_version TEXT NOT NULL,
    extra_json TEXT,
    uploaded_at TEXT
);
CREATE INDEX idx_diagnostics_uploaded_at ON diagnostics(uploaded_at) WHERE uploaded_at IS NULL;
CREATE INDEX idx_diagnostics_ts ON diagnostics(ts);

CREATE TABLE analytics_events (
    id TEXT PRIMARY KEY NOT NULL,
    ts TEXT NOT NULL,
    name TEXT NOT NULL,
    props_json TEXT,
    uploaded_at TEXT
);
CREATE INDEX idx_analytics_events_uploaded_at ON analytics_events(uploaded_at) WHERE uploaded_at IS NULL;
CREATE INDEX idx_analytics_events_ts ON analytics_events(ts);
