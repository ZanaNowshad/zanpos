-- Licensing spine: local mirror of an offline, Ed25519-signed license file.
-- Verification and grace-period calculation live in src-tauri/src/license.rs.
--
-- This table is informational only. Per the product spec's ABSOLUTE RULE, it
-- must NEVER be used to block till operations (completing a sale, printing a
-- receipt, opening the cash drawer, reading or exporting existing data,
-- EOD/shift close, security updates) — only the back office may degrade when
-- a license lapses past its grace period.
CREATE TABLE licenses (
    license_key       TEXT PRIMARY KEY,
    tier              TEXT NOT NULL,   -- core | plus
    store_name        TEXT,
    issued_at         TEXT NOT NULL,
    expires_at        TEXT,
    signature         TEXT NOT NULL,
    last_validated_at TEXT,
    grace_until       TEXT
);
