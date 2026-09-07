"""Every Tauri command authenticates its caller, or is listed here with a reason.

The defect this exists to prevent is RBAC-1: a command that takes the acting
user from its own payload. `require_role(db, actor_user_id, ...)` answers "does
this id hold a role" — never "is the caller that user" — so any caller who knew
an owner's id was authorised as that owner.

The migration is done, and this is what keeps it done. A new command that reads
its actor from the payload does not fail any test: it works, it looks like the
code around it, and nothing objects. So the contract is stated positively —
a command either resolves its caller from a session, or it appears in
`PRE_AUTH` with the reason it cannot.

Adding a name to `PRE_AUTH` is the point at which someone has to justify it, in
writing, in a diff a reviewer reads.
"""

from __future__ import annotations

import re

from .core import Project

# How a command proves who is calling. Any one of these in the signature or the
# body means the actor came from the session store rather than the payload.
SESSION_MARKERS = (
    "session_token",
    "session_actor(",
    "sessions.resolve(",
    "AuthenticatedActor",
    "cart_actor(",
    # The manager-approval path consumes a single-use token this process minted
    # in `auth_validate_manager_pin`; the id it returns is server-derived.
    "manager_approval(",
)

# Commands that cannot authenticate, each with the reason. Anything reachable
# before a session can exist, or whose authorisation is something other than a
# session — possession of a signed licence, a hub pairing token, an encrypted
# backup and its key.
PRE_AUTH: dict[str, str] = {
    # ── Authentication itself ────────────────────────────────────────────────
    "auth_list_users": "login discovery; runs before any session exists",
    "auth_login_pin": "mints the session",
    "auth_validate_manager_pin": "mints the manager override token",
    "auth_verify_owner_pin": "verifies a PIN; grants nothing on its own",
    "shift_get_active": (
        "the PIN screen calls this before sign-in. It grants nothing — it only "
        "refuses a known but deactivated account, i.e. a former employee's id "
        "still in use."
    ),
    # ── First run, before a store exists ─────────────────────────────────────
    "setup_wizard_complete": "creates the first owner",
    "setup_pull_catalog": "seeds a new terminal during setup",
    "app_config_load": "read at boot, before login",
    "app_config_get_timeout": "idle timeout, read at boot",
    "business_flags_load": "read at boot",
    "operational_settings_load": "read at boot",
    "onboarding_get_state": "drives the wizard before an owner exists",
    "migration_zanpos_stats": "counts rows in a foreign DB during migration",
    "license_get_entitlement": "read at boot to decide what the app may do",
    "license_import_file": "the signed licence is the authorisation",
    # ── Hub discovery and pairing ────────────────────────────────────────────
    "start_lan_discovery": "LAN broadcast during setup",
    "stop_lan_discovery": "LAN broadcast during setup",
    "list_discovered_hubs": "LAN broadcast during setup",
    "hub_test_connection": "tests a URL and token before joining",
    "hub_join": "the hub pairing token is the authorisation",
    # ── Local device and process, no store data ──────────────────────────────
    "system_keyboard_open": "opens the on-screen keyboard",
    "thermal_list_ports": "enumerates local serial ports",
    "startup_health_check": "app lifecycle, runs before login",
    "startup_restart_sidecar": "app lifecycle, restarts a local process",
    "product_pick_image": "opens a file dialog",
    "log_diagnostic": "local telemetry write",
    "flush_diagnostics_now": "local telemetry flush",
    # ── Updater ──────────────────────────────────────────────────────────────
    "check_for_updates": "runs at launch, before login",
    "check_critical_update": "runs at launch, before login",
    # ── Optional session: enforced only once a store has users ───────────────
    "backup_restore_file": (
        "restoring onto a dead till has to work before a store has an owner; "
        "it takes an optional session and requires an owner once users exist"
    ),
    "setup_save_benefit_number": (
        "written during the wizard before an owner exists; takes an optional "
        "session and requires manager/owner once setup is complete"
    ),
}

_COMMAND = re.compile(
    r"#\[tauri::command\][^\n]*\n(?:#\[[^\]]*\]\s*\n)*\s*pub\s+(?:async\s+)?fn\s+(\w+)\s*\(",
    re.M,
)


def _span(source: str, open_paren: int, opener: str, closer: str) -> str:
    depth, cursor = 0, open_paren
    while cursor < len(source):
        if source[cursor] == opener:
            depth += 1
        elif source[cursor] == closer:
            depth -= 1
            if depth == 0:
                return source[open_paren : cursor + 1]
        cursor += 1
    return source[open_paren:]


# The sink-inventory self-test synthesises a duplicate `#[tauri::command]` here
# to prove the *sink* controls notice a copied call. That copy has no caller and
# authorises nothing; flagging it would make every sink mutation also report an
# authorization failure, which buries the signal this check exists to give.
SINK_FIXTURE = "src-tauri/src/commands/__validator_fixture.rs"


def commands(project: Project) -> list[tuple[str, str, str]]:
    """(path, name, signature+body) for every `#[tauri::command]` in the tree."""
    found: list[tuple[str, str, str]] = []
    for path, parsed in project.files.items():
        if path == SINK_FIXTURE:
            continue
        source = parsed.source
        for match in _COMMAND.finditer(source):
            name = match.group(1)
            sig = _span(source, match.end() - 1, "(", ")")
            brace = source.find("{", match.end() - 1 + len(sig))
            body = _span(source, brace, "{", "}") if brace >= 0 else ""
            found.append((path, name, sig + body))
    return found


def validate_authorization(project: Project) -> list[tuple[str, str]]:
    """Flag any command that neither authenticates nor is listed as pre-auth.

    A listed command is not re-checked for *also* mentioning a session. Several
    legitimately do: `auth_login_pin` mints one, `setup_wizard_complete` returns
    one, and `backup_restore_file` and `setup_save_benefit_number` require one
    only once a store has users. Telling those apart from "this no longer needs
    to be listed" needs to know whether the check is conditional, which reading
    for a marker cannot do — so the check is not attempted rather than
    approximated. Entries that stop being commands at all are still caught.
    """
    issues: list[tuple[str, str]] = []
    seen: set[str] = set()
    for path, name, text in commands(project):
        seen.add(name)
        if any(marker in text for marker in SESSION_MARKERS):
            continue
        if name not in PRE_AUTH:
            issues.append((
                "PAYLOAD_DERIVED_ACTOR",
                f"{path}::{name} does not resolve its caller from a session. "
                f"Use rbac::session_actor, or add it to PRE_AUTH with the reason.",
            ))
    for name in sorted(set(PRE_AUTH) - seen):
        issues.append((
            "AUTHORIZATION_ALLOWLIST_STALE",
            f"PRE_AUTH lists {name}, which is no longer a Tauri command",
        ))
    return issues
