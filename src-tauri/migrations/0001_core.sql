-- branches: one row per store
CREATE TABLE branches (
    branch_id TEXT PRIMARY KEY,
    branch_code TEXT NOT NULL,
    name TEXT NOT NULL,
    currency TEXT NOT NULL DEFAULT 'BHD',
    timezone TEXT NOT NULL DEFAULT 'Asia/Bahrain',
    address TEXT,
    phone TEXT,
    receipt_header TEXT,
    receipt_footer TEXT,
    tax_number TEXT,
    cr_number TEXT,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    version INTEGER NOT NULL DEFAULT 1
);

-- roles
CREATE TABLE roles (
    role_id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL
);

-- devices: one row per terminal
CREATE TABLE devices (
    device_id TEXT PRIMARY KEY,
    branch_id TEXT NOT NULL REFERENCES branches(branch_id),
    device_code TEXT NOT NULL,
    name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'online',
    is_active INTEGER NOT NULL DEFAULT 1,
    next_receipt_seq INTEGER NOT NULL DEFAULT 1,
    last_seen_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    version INTEGER NOT NULL DEFAULT 1,
    sync_status TEXT NOT NULL DEFAULT 'pending',
    sync_attempts INTEGER NOT NULL DEFAULT 0,
    UNIQUE(branch_id, device_code)
);

-- users: PIN-authenticated terminal users
CREATE TABLE users (
    user_id TEXT PRIMARY KEY,
    branch_id TEXT NOT NULL,
    display_name TEXT NOT NULL,
    username TEXT NOT NULL UNIQUE,
    pin_hash TEXT NOT NULL,
    role_id TEXT NOT NULL REFERENCES roles(role_id),
    branch_scope TEXT NOT NULL DEFAULT '[]',
    is_active INTEGER NOT NULL DEFAULT 1,
    failed_pin_attempts INTEGER NOT NULL DEFAULT 0,
    locked_until TEXT,
    last_login_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    version INTEGER NOT NULL DEFAULT 1,
    sync_status TEXT NOT NULL DEFAULT 'pending',
    sync_attempts INTEGER NOT NULL DEFAULT 0
    -- NOTE: pin_hash is NEVER synced to cloud. The sync worker must exclude it.
);

-- app_config: key-value settings store
CREATE TABLE app_config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_users_username ON users(username);
CREATE INDEX idx_users_role ON users(role_id);
CREATE INDEX idx_devices_branch ON devices(branch_id);
CREATE INDEX idx_devices_code ON devices(branch_id, device_code);
