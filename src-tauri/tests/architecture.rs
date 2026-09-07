//! The boundaries the rest of the codebase is allowed to assume.
//!
//! ZANPOS is four layers with one seam between them that nothing type-checks:
//!
//! ```text
//! React  ──invoke("name")──▶  generate_handler![…]  ──▶  #[tauri::command] fn
//! ```
//!
//! The command name is a string on one side and an identifier on the other, and
//! nothing in either toolchain compares them. A command can be written and never
//! registered — it compiles, it is covered by unit tests, and it is unreachable
//! from the application. A screen can invoke a name that no longer exists — it
//! type-checks, and fails at runtime in front of a cashier.
//!
//! Both were clean when this was written (304 commands, 304 registered, 0 broken
//! calls). That is the point: this file is what keeps them clean, because the
//! seam has no other guard.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Walk a tree, skipping the package-manager caches that tooling drops inside
/// the source directories. They contain unrelated Rust and TypeScript and are
/// not part of either crate.
fn source_files(root: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if name.starts_with('.') || name == "node_modules" || name == "target" {
                    continue;
                }
                stack.push(path);
            } else if extensions
                .iter()
                .any(|ext| name.ends_with(&format!(".{ext}")))
            {
                // A module that declares itself test-only is not production code
                // however it is named. `db::invariants` is a directory module, so
                // it cannot use the `_tests.rs` suffix to say so.
                if name.ends_with(".rs")
                    && std::fs::read_to_string(&path)
                        .unwrap_or_default()
                        .contains("#![cfg(test)]")
                {
                    continue;
                }
                out.push(path);
            }
        }
    }
    out
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// Every `#[tauri::command]` in the crate, mapped to the file that defines it.
///
/// Matched on the attribute rather than on a naming convention, because the
/// attribute is what actually makes a function callable across the boundary.
fn defined_commands() -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    for file in source_files(&crate_root().join("src"), &["rs"]) {
        let text = read(&file);
        let label = file
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        for (index, _) in text.match_indices("#[tauri::command") {
            // Skip past the attribute (which may carry arguments) to the `fn`
            // that follows, then take the identifier after it.
            let rest = &text[index..];
            let Some(fn_at) = rest.find(" fn ") else {
                continue;
            };
            // Guard against matching a `fn` far below a malformed attribute.
            if fn_at > 200 {
                continue;
            }
            let name: String = rest[fn_at + 4..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                found.insert(name, label.clone());
            }
        }
    }
    found
}

/// The identifiers listed inside `tauri::generate_handler![…]`.
fn registered_commands() -> BTreeSet<String> {
    // Comments are stripped before the search, not after. lib.rs documents the
    // registration in prose above it — including the literal `generate_handler![]`
    // — and matching that comment yields an empty block whose bracket depth
    // closes immediately, so every command reads as unregistered.
    let lib: String = read(&crate_root().join("src").join("lib.rs"))
        .lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    let start = lib
        .find("generate_handler![")
        .expect("lib.rs must register commands through tauri::generate_handler!");
    let body = &lib[start..];
    let mut depth = 0usize;
    let mut end = body.len();
    for (index, ch) in body.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    end = index;
                    break;
                }
            }
            _ => {}
        }
    }

    body[..end]
        .split(',')
        .filter_map(|entry| entry.trim().rsplit("::").next())
        .map(|name| name.trim().to_string())
        .filter(|name| {
            !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
        .collect()
}

/// Every command name the frontend asks for, from anywhere in the React tree.
fn invoked_commands() -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let frontend = crate_root().join("..").join("src");
    for file in source_files(&frontend, &["ts", "tsx"]) {
        let text = read(&file);
        let mut rest = text.as_str();
        while let Some(at) = rest.find("invoke") {
            rest = &rest[at + "invoke".len()..];
            // Allow the generic parameter form `invoke<T>("name")`.
            let after_generic = match rest.find('(') {
                Some(paren)
                    if rest[..paren]
                        .chars()
                        .all(|c| !c.is_whitespace() || c == ' ') =>
                {
                    &rest[paren + 1..]
                }
                _ => continue,
            };
            let trimmed = after_generic.trim_start();
            let Some(quote) = trimmed.chars().next() else {
                continue;
            };
            if quote != '"' && quote != '\'' {
                continue;
            }
            let name: String = trimmed[1..].chars().take_while(|c| *c != quote).collect();
            if !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            {
                names.insert(name);
            }
        }
    }
    names
}

/// A command that is written but never registered is unreachable: it compiles,
/// its unit tests pass, and no screen can ever call it. Nothing else in either
/// toolchain notices.
#[test]
fn every_command_is_registered_with_tauri() {
    let defined = defined_commands();
    let registered = registered_commands();

    let missing: Vec<String> = defined
        .iter()
        .filter(|(name, _)| !registered.contains(*name))
        .map(|(name, file)| format!("{name}  [{file}]"))
        .collect();

    assert!(
        missing.is_empty(),
        "{} command(s) are defined but not in generate_handler! — they compile, \
         they are tested, and the application cannot reach them:\n  {}",
        missing.len(),
        missing.join("\n  ")
    );
}

/// The other direction: a screen calling a name no command answers to fails at
/// runtime, in front of whoever pressed the button.
#[test]
fn every_command_the_frontend_calls_exists() {
    let defined = defined_commands();
    let invoked = invoked_commands();

    // Tauri ships built-in commands under a `plugin:` prefix; those are not ours
    // and are filtered out by the lowercase-identifier rule already.
    let unknown: Vec<&String> = invoked
        .iter()
        .filter(|name| !defined.contains_key(*name))
        .collect();

    assert!(
        unknown.is_empty(),
        "the frontend invokes {} command(s) that no #[tauri::command] defines:\n  {:?}",
        unknown.len(),
        unknown
    );
}

/// Commands that exist, are registered, and that nothing in the app calls.
///
/// Not a failure on its own — a command can legitimately wait for the screen
/// that will use it. It is pinned so that the list cannot grow silently, because
/// each entry is a feature the application carries the cost of and gets no value
/// from, and the reason differs per entry.
#[test]
fn the_set_of_unreachable_features_is_known_and_accounted_for() {
    let defined = defined_commands();
    let registered = registered_commands();
    let invoked = invoked_commands();

    // Each of these is deliberate today, and each has a different reason.
    let accounted: BTreeMap<&str, &str> = BTreeMap::from([
        // Per-device hub pairing is implemented end to end — token issue,
        // revocation, digest storage, hub-side verification, tests — and has no
        // interface. The consequence is not cosmetic: with no way to pair, the
        // legacy shared store token stays the only authentication path, and
        // `hub::rest::check_auth` logs "pair it to retire that path" on every
        // request for a retirement nothing can perform.
        ("hub_pair_device", "no pairing UI exists"),
        ("hub_list_devices", "no pairing UI exists"),
        ("hub_revoke_device", "no pairing UI exists"),
        // mDNS is behind the `mdns-discovery` feature, which is not in the
        // default set, so these return "not compiled in" in every shipped build.
        (
            "start_lan_discovery",
            "mdns-discovery feature is off by default",
        ),
        (
            "stop_lan_discovery",
            "mdns-discovery feature is off by default",
        ),
        (
            "list_discovered_hubs",
            "mdns-discovery feature is off by default",
        ),
        // The X report is wired to a screen; the Z report is not.
        ("report_z_report", "only the X report has a screen"),
        // Row-level parity is reached through the reconciliation panel instead.
        ("sync_parity_report", "superseded by reconciliation_preview"),
    ]);

    let unreachable: BTreeSet<&str> = defined
        .keys()
        .filter(|name| registered.contains(*name) && !invoked.contains(*name))
        .map(String::as_str)
        .collect();

    let newly: Vec<&&str> = unreachable
        .iter()
        .filter(|name| !accounted.contains_key(*name))
        .collect();
    assert!(
        newly.is_empty(),
        "new command(s) that nothing calls: {newly:?}. Either wire them to a \
         screen or record why they exist here."
    );

    let now_reachable: Vec<&&str> = accounted
        .keys()
        .filter(|name| !unreachable.contains(*name))
        .collect();
    assert!(
        now_reachable.is_empty(),
        "these are listed as unreachable but are now called — drop them from the \
         list: {now_reachable:?}"
    );
}

// ── Nothing reaches the network without a deadline ───────────────────────────

/// Every outbound HTTP client bounds how long it will wait.
///
/// `reqwest` applies no timeout unless one is requested. A client built without
/// one will wait indefinitely on a host that accepts the connection and then
/// says nothing — which is not a rare failure, it is what a half-open connection
/// or an overloaded endpoint looks like.
///
/// Forty-one of the forty-two clients in this codebase set one. The exception
/// was the off-site backup upload: a background loop, so the hang was silent,
/// on the feature whose entire purpose is surviving the loss of the machine.
#[test]
fn every_outbound_http_client_sets_a_timeout() {
    let mut missing = Vec::new();
    for (label, text) in production_sources() {
        for marker in ["Client::new()", "Client::builder()"] {
            let mut rest = text.as_str();
            while let Some(at) = rest.find(marker) {
                // The builder chain runs until the statement ends. Look ahead far
                // enough to cover a long chain of headers, but not into the next
                // statement.
                let window_end = rest[at..]
                    .find(".send()")
                    .or_else(|| rest[at..].find(";"))
                    .map(|end| at + end)
                    .unwrap_or(rest.len());
                if !rest[at..window_end].contains("timeout") {
                    missing.push(format!("{label}  ({marker})"));
                }
                rest = &rest[at + marker.len()..];
            }
        }
    }

    assert!(
        missing.is_empty(),
        "HTTP client(s) built without a timeout:\n  {}\nreqwest waits for ever by \
         default. In a background loop that is a feature that stops working and \
         reports nothing.",
        missing.join("\n  ")
    );
}

// ── Keeping the compiler able to speak ───────────────────────────────────────

/// No module may switch dead-code reporting off for its whole file.
///
/// Ten did. Between them they hid twenty unused items, including two complete
/// abandoned implementations that shipped in every installer: `ai::oauth` — 135
/// lines of provider sign-in for an application that authenticates by API key,
/// referenced only by its own `mod` declaration — and a 250-line
/// `run_tool_loop` in `ai_admin_commands`, whose own doc comment described it as
/// "shared by the blocking and streaming paths" when it was called by neither.
///
/// Both were invisible for the same reason: `#![allow(dead_code)]` at the top of
/// a file silences the one check that would have named them. An item that must
/// be kept can say so on its own line, with a reason. A whole file cannot.
#[test]
fn no_module_silences_dead_code_reporting_for_its_whole_file() {
    let blanket: Vec<String> = source_files(&crate_root().join("src"), &["rs"])
        .into_iter()
        .filter(|path| read(path).contains("#![allow(dead_code)]"))
        .map(|path| {
            path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
        })
        .collect();

    assert!(
        blanket.is_empty(),
        "file-wide dead-code suppression in {blanket:?}. It hides every unused \
         item in the file, for ever, including ones added later. Put \
         `#[allow(dead_code)]` on the specific item and say why it is kept."
    );
}

// ── The other untyped seam: events ───────────────────────────────────────────

/// String literals passed to `emit`/`emit_to`/`emit_all` anywhere in the crate.
fn emitted_events() -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    for file in source_files(&crate_root().join("src"), &["rs"]) {
        let label = file
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let text = strip_test_items(&read(&file));
        for marker in [".emit(", ".emit_to(", ".emit_all(", "emit_filter("] {
            let mut rest = text.as_str();
            while let Some(at) = rest.find(marker) {
                rest = &rest[at + marker.len()..];
                let trimmed = rest.trim_start();
                if !trimmed.starts_with('"') {
                    continue;
                }
                let name: String = trimmed[1..].chars().take_while(|c| *c != '"').collect();
                if !name.is_empty() {
                    found.insert(name, label.clone());
                }
            }
        }
    }
    found
}

/// Event names the React tree subscribes to.
fn listened_events() -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for file in source_files(&crate_root().join("..").join("src"), &["ts", "tsx"]) {
        let text = read(&file);
        let mut rest = text.as_str();
        while let Some(at) = rest.find("listen") {
            rest = &rest[at + "listen".len()..];
            let Some(paren) = rest.find('(') else {
                continue;
            };
            if rest[..paren].contains('\n') {
                continue;
            }
            let trimmed = rest[paren + 1..].trim_start();
            let Some(quote) = trimmed.chars().next() else {
                continue;
            };
            if quote != '"' && quote != '\'' {
                continue;
            }
            let name: String = trimmed[1..].chars().take_while(|c| *c != quote).collect();
            if !name.is_empty() {
                names.insert(name);
            }
        }
    }
    names
}

/// An event with no listener is a message the backend believes it delivered.
///
/// The 24-hour maintenance loop emitted its findings — quarantined sync rows, a
/// failed `PRAGMA quick_check`, orphaned foreign keys — one at a time on
/// `"proactive-alert"`. The UI listens to `"proactive-alerts"`, plural, with an
/// array payload. Nothing received any of them, they were never written to the
/// `proactive_alerts` table either, and the cycle logged how many alerts it had
/// raised. Three of the application's most serious health signals went nowhere,
/// and the logs said otherwise.
///
/// Nothing in Tauri, TypeScript or Rust compares these two strings. This does.
#[test]
fn every_event_the_backend_emits_has_a_listener() {
    let emitted = emitted_events();
    let listened = listened_events();

    // Emitted for a consumer outside the React tree, or for a payload the app
    // deliberately does not react to.
    let accounted: BTreeSet<&str> = BTreeSet::from([
        // Tauri's own lifecycle channels, emitted by the framework's plugins.
        "tauri://update",
        "tauri://update-status",
    ]);

    let orphans: Vec<String> = emitted
        .iter()
        .filter(|(name, _)| !listened.contains(*name) && !accounted.contains(name.as_str()))
        .map(|(name, file)| format!("{name}  [{file}]"))
        .collect();

    assert!(
        orphans.is_empty(),
        "{} event(s) are emitted with nothing listening:\n  {}\nEither wire a \
         listener or persist the payload somewhere the UI reads — an emit with \
         no subscriber succeeds and delivers nothing.",
        orphans.len(),
        orphans.join("\n  ")
    );
}

/// And the reverse: a screen waiting on an event no one sends waits for ever,
/// showing whatever it showed before.
#[test]
fn every_event_the_frontend_waits_for_is_emitted() {
    let emitted = emitted_events();
    let listened = listened_events();

    let never_sent: Vec<&String> = listened
        .iter()
        // Tauri's built-in channels are namespaced and are not ours to send.
        .filter(|name| !name.starts_with("tauri://"))
        .filter(|name| !emitted.contains_key(*name))
        .collect();

    assert!(
        never_sent.is_empty(),
        "the frontend listens for event(s) nothing emits: {never_sent:?}"
    );
}

// ── Which way the dependencies point ─────────────────────────────────────────

/// Files under a directory, as (relative label, production text).
fn layer(dir: &str) -> Vec<(String, String)> {
    let root = crate_root().join("src").join(dir);
    source_files(&root, &["rs"])
        .into_iter()
        .filter(|path| {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            !(name == "tests.rs" || name.ends_with("_tests.rs"))
        })
        .map(|path| {
            let label = format!(
                "{dir}/{}",
                path.file_name().unwrap_or_default().to_string_lossy()
            );
            (label, strip_test_items(&read(&path)))
        })
        .collect()
}

/// The stack only points one way:
///
/// ```text
/// commands  ──▶  db::repositories / inventory  ──▶  domain
/// ```
///
/// `domain` is the pure layer — money arithmetic, cart totals, validation — and
/// depends on nothing. The repositories own persistence. The command layer is
/// the Tauri boundary and may use both.
///
/// One violation existed when this was written: `sale_repo` reached up into
/// `commands::setup_commands` for `currency_exponent`, an ISO 4217 property of a
/// currency that had been defined in the command layer. It now lives in
/// `domain::money` beside `format_minor`, the function that consumes it. This
/// test is what stops the next such rule landing in whichever layer was
/// convenient at the time.
#[test]
fn the_lower_layers_never_reach_up_into_the_command_layer() {
    let mut offenders = Vec::new();
    for dir in ["db", "inventory", "domain"] {
        for (label, text) in layer(dir) {
            if text.contains("crate::commands::") {
                // Report the first reference so the message names the rule, not
                // just the file.
                let at = text.find("crate::commands::").unwrap_or(0);
                let snippet: String = text[at..].chars().take(60).collect();
                offenders.push(format!("{label} → {}", snippet.trim_end()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "the persistence and domain layers reach up into the Tauri command \
         layer:\n  {}\nA rule needed by a repository is not a command; move it \
         down into `domain` (or into the repository itself) so the dependency \
         points one way.",
        offenders.join("\n  ")
    );
}

/// `domain` is the bottom of the stack and must stay free of persistence, so the
/// business rules can be tested without a database and cannot quietly start
/// issuing queries.
#[test]
fn the_domain_layer_owns_no_persistence() {
    let offenders: Vec<String> = layer("domain")
        .into_iter()
        .filter(|(_, text)| {
            text.contains("sqlx::query")
                || text.contains("crate::db::")
                || text.contains("SqlitePool")
        })
        .map(|(label, _)| label)
        .collect();
    assert!(
        offenders.is_empty(),
        "domain modules touching the database: {offenders:?}. The domain layer \
         is the one place whose rules can be exercised without a pool; a query \
         here takes that away."
    );
}

// ── Who is allowed to write the books ────────────────────────────────────────

/// Strip `#[cfg(test)]` items, leaving the code that actually ships.
///
/// Deliberately not "truncate at the first `#[cfg(test)]`". Several modules —
/// `commands::device_state` among them — declare their tests part-way down and
/// carry real code below, so truncating there silently drops production code
/// from the scan and every pin below quietly covers less than it says it does.
/// Each attribute is removed together with exactly the item that follows it: a
/// brace-matched `mod tests { … }`, or a one-line `mod tests;` declaration whose
/// sibling file is excluded by name.
fn strip_test_items(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    const ATTR: &str = "#[cfg(test)]";
    while let Some(at) = rest.find(ATTR) {
        out.push_str(&rest[..at]);
        let after = &rest[at + ATTR.len()..];
        match (after.find('{'), after.find(';')) {
            // `mod tests;` — drop through the semicolon.
            (brace, Some(semi)) if brace.is_none_or(|b| semi < b) => {
                rest = &after[semi + 1..];
            }
            // `mod tests { … }` — drop through the matching brace.
            (Some(brace), _) => {
                let mut depth = 0usize;
                let mut end = after.len();
                for (index, ch) in after[brace..].char_indices() {
                    match ch {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                end = brace + index + 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                rest = &after[end..];
            }
            _ => {
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Production Rust only: test modules and fixtures write these tables freely and
/// should. Sibling `tests.rs` files are excluded by name.
fn production_sources() -> Vec<(String, String)> {
    source_files(&crate_root().join("src"), &["rs"])
        .into_iter()
        .filter(|path| {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            !(name == "tests.rs" || name.ends_with("_tests.rs"))
        })
        .map(|path| {
            let label = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            (label, strip_test_items(&read(&path)))
        })
        .collect()
}

/// The stripper has to remove the tests and keep everything else. If it removed
/// too much, every pin below would pass by scanning an empty file.
#[test]
fn stripping_tests_keeps_the_code_that_ships() {
    let sample = "fn before() {}\n\
                  #[cfg(test)]\nmod tests;\n\
                  fn between() {}\n\
                  #[cfg(test)]\nmod tests { fn inner() { if x { y } } }\n\
                  fn after() {}\n";
    let stripped = strip_test_items(sample);
    assert!(stripped.contains("fn before"), "{stripped}");
    assert!(
        stripped.contains("fn between"),
        "code below a mid-file test declaration was dropped: {stripped}"
    );
    assert!(
        stripped.contains("fn after"),
        "code below a test module was dropped: {stripped}"
    );
    assert!(!stripped.contains("fn inner"), "{stripped}");

    // And on the real tree: a file that declares its tests part-way down must
    // still contribute the code below them.
    let device_state = production_sources()
        .into_iter()
        .find(|(name, _)| name == "device_state.rs")
        .map(|(_, text)| text)
        .expect("device_state.rs is in the scan");
    assert!(
        device_state.contains("pub async fn roster"),
        "device_state.rs declares its tests mid-file and everything below them \
         was dropped from the scan"
    );
}

/// SQL in this codebase is written across several lines with varied indentation,
/// so the text is flattened to single spaces before matching. Without that,
/// `INSERT INTO stock_levels\n    (stock_level_id, …)` is invisible to a plain
/// substring search and the pin silently covers less than it claims.
fn flatten(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Files containing a statement of one of `verbs` against `table`.
fn writers_of(table: &str, verbs: &[&str]) -> BTreeSet<String> {
    let needles: Vec<String> = verbs.iter().map(|verb| format!("{verb} {table}")).collect();
    production_sources()
        .into_iter()
        .filter(|(_, text)| {
            let flat = flatten(text);
            needles.iter().any(|needle| {
                flat.match_indices(needle.as_str()).any(|(at, _)| {
                    // Reject `stock_levels_archive` matching `stock_levels`.
                    flat[at + needle.len()..]
                        .chars()
                        .next()
                        .is_none_or(|c| !c.is_alphanumeric() && c != '_')
                })
            })
        })
        .map(|(name, _)| name)
        .collect()
}

const INSERTS: &[&str] = &[
    "INSERT INTO",
    "INSERT OR IGNORE INTO",
    "INSERT OR REPLACE INTO",
];
const ALL_WRITES: &[&str] = &[
    "INSERT INTO",
    "INSERT OR IGNORE INTO",
    "INSERT OR REPLACE INTO",
    "UPDATE",
    "DELETE FROM",
];

/// A sale and its payments are minted in exactly one place.
///
/// `receipt_number` and `idempotency_key` are both `NOT NULL UNIQUE`, and the
/// per-device receipt sequence is claimed with an atomic `UPDATE … RETURNING`
/// inside the checkout transaction. Those guarantees are properties of *that*
/// transaction. A second path that inserts a sale does not inherit them: it
/// would mint receipt numbers from a counter it did not reserve, and duplicate
/// or gap the sequence a shop reconciles its till against.
///
/// This is the invariant with the least room for negotiation in the codebase,
/// and the one whose violation would be least visible — a second writer would
/// look like a working feature until the numbers were audited.
///
/// Deliberately about *creation*, not mutation. Voiding a sale, marking a
/// delivery paid and applying a replicated row all legitimately `UPDATE sales`;
/// none of them mints a receipt number, which is the thing that must have one
/// owner. `sync_v2::apply` is exempt because it does not create sales either —
/// it replicates ones another terminal already minted, through the append-only
/// path that reads a unique-constraint violation back as a duplicate.
#[test]
fn only_the_sale_repository_creates_sales_and_payments() {
    for table in ["sales", "payments"] {
        let creators = writers_of(table, INSERTS);
        let unexpected: Vec<&String> = creators
            .iter()
            .filter(|name| {
                !matches!(
                    name.as_str(),
                    // The one owner of the live receipt sequence.
                    "sale_repo.rs"
                        // Replication, not creation — see above.
                        | "apply.rs"
                        // The legacy-POS importer. It does create sale rows, but
                        // it never enters the live sequence: receipt numbers are
                        // namespaced `IMP-{original}` so an imported sale cannot
                        // collide with one this shop mints, and the insert is
                        // `OR IGNORE` so re-running an import is idempotent.
                        | "migration_commands.rs"
                )
            })
            .collect();
        assert!(
            unexpected.is_empty(),
            "{table} rows are created outside sale_repo.rs by {unexpected:?}. \
             Checkout reserves the per-device receipt sequence and the \
             idempotency key inside one transaction; a second creator does not \
             inherit that and can duplicate or gap the numbers a shop balances \
             its till against."
        );
    }
}

/// Stock, by contrast, is written from many places — and that is the standing
/// architectural debt this pins rather than pretends away.
///
/// `inventory/movements.rs` is the intended service ("creates a stock_movement
/// record and updates stock_levels atomically"), and the sale, refund and void
/// paths do go through it. The receive/adjust commands, the purchasing receipt
/// path and several ZanAI tools each reimplement the same read → compute →
/// write instead, with their own arithmetic and their own precision: `movements`
/// formats to four decimal places through `f64`, `inventory_commands` uses exact
/// `Decimal`, `sync_v2::apply` recomputes to three, and checkout does the
/// subtraction in SQL with `CAST AS REAL`.
///
/// Consolidating them is a large change to the most financially sensitive code
/// in the application and is not attempted here. What must not happen is the set
/// growing by accident: every entry below is a place that has to be found and
/// changed together whenever the stock rules move.
#[test]
fn the_places_that_write_stock_are_a_known_set() {
    let known: BTreeSet<&str> = BTreeSet::from([
        // The intended service, its lot allocator, and the replication path.
        // Refund and void reach stock only through the service — they appear in
        // a raw grep for `stock_levels`, but only inside their test modules.
        "movements.rs",
        "lots.rs",
        "apply.rs",
        // Checkout adjusts the level inside the sale transaction and hands the
        // resulting quantities to `movements::deduct_sale` for the ledger row.
        "sale_repo.rs",
        // Reimplement the service's read → compute → write inline.
        "inventory_commands.rs",
        "purchasing_commands.rs",
        // ZanAI calls the service for `manual_adjust` and `stock_take`, and
        // writes its own movements for the rest.
        "tools.rs",
        "ops.rs",
        "batch.rs",
        "intent_engine.rs",
        // Seeding and repair paths that only ever create a missing row.
        "admin_commands.rs",
        "migration_commands.rs",
        // Folding one product into another sums the two stock rows. Split out of
        // product_dedup_repo.rs when that file outgrew the size limit; the write
        // itself is unchanged and still inside the merge transaction.
        "product_merge.rs",
    ]);

    let mut writers = writers_of("stock_levels", ALL_WRITES);
    writers.extend(writers_of("stock_movements", ALL_WRITES));

    let unknown: Vec<&String> = writers
        .iter()
        .filter(|name| !known.contains(name.as_str()))
        .collect();
    assert!(
        unknown.is_empty(),
        "stock is now written from {unknown:?} as well. Prefer \
         inventory::movements, which keeps the movement and the level in one \
         transaction; if a new direct writer is genuinely necessary, add it here \
         so the next person changing the stock rules can find it."
    );

    let gone: Vec<&&str> = known
        .iter()
        .filter(|name| !writers.contains(**name))
        .collect();
    assert!(
        gone.is_empty(),
        "these no longer write stock — remove them from the list so it keeps \
         meaning something: {gone:?}"
    );
}

/// The audit found 304 commands. The exact number is not the point; a sudden
/// collapse would mean the attribute scan silently stopped matching, which would
/// make the two tests above pass by finding nothing.
#[test]
fn the_command_scan_still_finds_the_boundary() {
    let defined = defined_commands();
    assert!(
        defined.len() > 250,
        "only {} #[tauri::command] found — the scan is broken, and the contract \
         tests above are passing vacuously",
        defined.len()
    );
    assert!(
        registered_commands().len() > 250,
        "generate_handler! parse returned too few entries — the contract tests \
         above are passing vacuously"
    );
    assert!(
        invoked_commands().len() > 200,
        "frontend invoke() scan found too few calls — the contract test above is \
         passing vacuously"
    );
}

// ── Which financial rows may be rewritten, and which may not ────────────────

/// The books are append-only. Corrections are new rows, never edits.
///
/// A sale says what was collected, a payment says how, a refund says what went
/// back. Together they are the audit trail, and the trail only means anything
/// because nothing goes back and changes an earlier entry — a refund that
/// lowered `sales.net_total_minor` instead of writing a `refunds` row would
/// balance perfectly and leave no evidence the money was ever taken.
///
/// So `payments`, `refunds` and `refund_items` are immutable once written, and
/// `sales` is immutable except for `status`, which is how a void is recorded.
/// `sale_items.refunded_amount_minor` is the running total the refund ceiling is
/// enforced against; it is a derived counter, not a restatement of the sale.
///
/// Editable by contrast: `delivery_orders.payment_status`, because a delivery is
/// settled after the fact and both settling and reverting write an audited
/// before/after pair, and `stock_levels`, which is a cache of the movement
/// ledger. Neither is a record of money collected.
///
/// This is a scan, so it cannot see through a query built at runtime. It catches
/// the shape every existing writer uses, which is what makes a new one stand out.
#[test]
fn nothing_rewrites_a_row_that_records_money_collected() {
    // Statement → the reason it is allowed. An entry here is a decision.
    let allowed: BTreeMap<&str, &str> = [
        (
            "DELETE FROM payments WHERE sale_id IN (",
            "retention pruning of sales already synced and past the window",
        ),
        (
            "DELETE FROM refund_items WHERE refund_id IN (",
            "retention pruning, with the parent refund",
        ),
        (
            "DELETE FROM refunds WHERE original_sale_id IN (",
            "retention pruning, with the parent sale",
        ),
        (
            "UPDATE sale_items\n             SET refunded_amount_minor = refunded_amount_minor + ?",
            "the refund ceiling counter, incremented atomically as the guard",
        ),
        (
            "UPDATE sales SET status = 'voided'",
            "a void is a status change on the sale, recorded with an audit row",
        ),
        (
            "DELETE FROM sales WHERE sale_id IN (",
            "retention pruning of sales already synced and past the window",
        ),
        (
            "UPDATE sales SET status = ?, updated_at = ?, sync_status = 'pending'",
            "a refund marks the sale refunded; the amounts are untouched",
        ),
        (
            "UPDATE sales SET is_delivery = 1",
            "a classification flag, not an amount",
        ),
    ]
    .into_iter()
    .collect();

    let mut offences: Vec<String> = Vec::new();
    for path in source_files(&crate_root().join("src"), &["rs"]) {
        let text = read(&path);
        // Test modules write fixtures directly; they are not the application.
        // Both spellings: a file-level `#![cfg(test)]`, and the `mod tests;`
        // convention where the module body lives in `…/tests.rs`.
        let is_test_file = path
            .file_name()
            .is_some_and(|n| n == "tests.rs" || n.to_string_lossy().ends_with("_tests.rs"));
        if is_test_file || text.starts_with("#![cfg(test)]") {
            continue;
        }
        let rel = path
            .strip_prefix(crate_root())
            .unwrap_or(&path)
            .display()
            .to_string()
            .replace('\\', "/");

        for table in ["payments", "refunds", "refund_items", "sales"] {
            for verb in ["UPDATE", "DELETE FROM"] {
                let needle = format!("{verb} {table}");
                let mut from = 0;
                while let Some(i) = text[from..].find(&needle) {
                    let at = from + i;
                    from = at + needle.len();
                    // `UPDATE sales` also matches `UPDATE sales_archive`; require a
                    // word boundary.
                    if text[from..]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_')
                    {
                        continue;
                    }
                    let window = &text[at..text.len().min(at + 200)];
                    if allowed.keys().any(|a| window.starts_with(a)) {
                        continue;
                    }
                    // Only count it inside a SQL string literal.
                    if !text[..at].ends_with('"') && !text[..at].contains('"') {
                        continue;
                    }
                    let line = text[..at].matches('\n').count() + 1;
                    offences.push(format!(
                        "{rel}:{line} — {}",
                        window.lines().next().unwrap_or("").trim()
                    ));
                }
            }
        }
    }

    assert!(
        offences.is_empty(),
        "these rewrite rows that record money collected. A correction belongs in a \
         new row — a refund, a void, an adjustment — so the original stays \
         readable. If one of these is genuinely a permitted edit, add it to \
         `allowed` with the reason:\n  {}",
        offences.join("\n  ")
    );
}

// ── One authority for what a product costs ──────────────────────────────────

/// Every SQL string literal in a file, so a scan looks at statements rather than
/// at whatever text happens to surround a keyword.
fn sql_literals(text: &str) -> Vec<(usize, String)> {
    let bytes: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == '"' {
            let start = i;
            i += 1;
            let mut buf = String::new();
            while i < bytes.len() && bytes[i] != '"' {
                if bytes[i] == '\\' {
                    i += 1;
                    if i < bytes.len() && bytes[i] != '\n' {
                        buf.push(bytes[i]);
                    }
                } else {
                    buf.push(bytes[i]);
                }
                i += 1;
            }
            out.push((start, buf.split_whitespace().collect::<Vec<_>>().join(" ")));
        }
        i += 1;
    }
    out
}

/// Nothing resolves a current selling price by hand.
///
/// Selling prices live in `product_prices`, and picking the row in force takes
/// four conditions and a tie-break: shop-wide, `selling`, started, not ended, and
/// newest-wins when two are open. Fourteen queries across the POS, the back
/// office and the assistant each had their own idea of how many of those to
/// apply. The ones missing `effective_from` treated a price scheduled for next
/// week as today's. The ones missing the tie-break returned whichever of two open
/// rows the join reached first — and in a list query a LEFT JOIN matching two
/// rows returns the product twice, so the manager saw the same item at two
/// prices.
///
/// The answer has one home now. A query either joins `v_current_selling_price`,
/// or splices `pricing::PRICE_IN_FORCE`, or writes the correlated
/// `price_id = (…)` subquery those two are made of. Anything else is a second
/// authority on one number, which is what produced a cashier seeing a price the
/// till then refused to charge.
///
/// Two things legitimately read the table directly: listing price *history*, and
/// anything about `promotional` prices, which are not what the till charges.
#[test]
fn no_query_resolves_a_selling_price_by_hand() {
    let mut offences: Vec<String> = Vec::new();

    for path in source_files(&crate_root().join("src"), &["rs"]) {
        if path
            .file_name()
            .is_some_and(|n| n == "tests.rs" || n.to_string_lossy().ends_with("_tests.rs"))
        {
            continue;
        }
        // The module that owns the definition is allowed to state it.
        if path.ends_with("pricing.rs") {
            continue;
        }
        let text = read(&path);
        let rel = path
            .strip_prefix(crate_root())
            .unwrap_or(&path)
            .display()
            .to_string()
            .replace('\\', "/");

        for (at, stmt) in sql_literals(&text) {
            let low = stmt.to_lowercase();
            if !low.contains("product_prices") {
                continue;
            }
            // Either a whole query that pulls a price, or a join fragment held in
            // a `const` and spliced into one. The fragment form is how a seventh
            // copy of this predicate hid from an earlier version of this scan.
            let is_query = low.contains("select") && low.contains("price_minor");
            let is_join_fragment = low.contains("join product_prices")
                && (low.contains("price_type") || low.contains("effective_"));
            if !is_query && !is_join_fragment {
                continue;
            }
            if low.starts_with("insert") || low.starts_with("update") || low.starts_with("delete") {
                continue;
            }
            // A different question: promotions, or the history of a price.
            if low.contains("promotional") {
                continue;
            }
            // Selecting the window columns themselves is a history listing.
            if low.contains("effective_to,") || low.contains("effective_from,") {
                continue;
            }
            // The sanctioned forms.
            if low.contains("{price_in_force}")
                || low.contains("v_current_selling_price")
                || low.contains("price_id = (")
                || low.contains("price_id=(")
            {
                continue;
            }

            let line = text[..at.min(text.len())].matches('\n').count() + 1;
            offences.push(format!("{rel}:{line} — {}", &low[..low.len().min(120)]));
        }
    }

    assert!(
        offences.is_empty(),
        "these resolve a selling price with their own predicate instead of \
         `v_current_selling_price`, `pricing::PRICE_IN_FORCE`, or the correlated \
         `price_id = (…)` subquery. Two authorities on one number is how a \
         cashier ends up seeing a price the till refuses:\n  {}",
        offences.join("\n  ")
    );
}

// ── Stock is a ledger, not a number ─────────────────────────────────────────

/// Nothing changes a shelf count without recording why.
///
/// `stock_levels` is a cache of the `stock_movements` ledger. A write to the
/// cache with no movement beside it makes the quantity unexplainable: the
/// movements add up to one figure, the cache reports another, and only a
/// physical count can say which is right.
///
/// Both halves have to be written by the same transaction, too. The sale used to
/// decrement the cache inside its transaction and write the movement afterwards
/// in a separate one; its own error message said what that cost — "Stock levels
/// were already updated in the sale transaction" — and nothing ever retried. The
/// refund had the mirror of it, crediting stock after its commit with the
/// failure only logged.
///
/// This is a scan, so it checks the shape rather than the semantics: a file that
/// writes the cache must also write the ledger, either directly or through the
/// `movements` module that owns the paired writes. A new writer that does
/// neither stands out, which is the point.
#[test]
fn nothing_moves_stock_without_recording_the_movement() {
    // Files that legitimately write the cache without a movement, and why.
    let allowed: BTreeMap<&str, &str> = [
        (
            "apply.rs",
            "sync applies a peer's stock rows; movements arrive as their own rows              and `recompute_stock_level` derives the cache from them",
        ),
        (
            "migration_commands.rs",
            "the importer seeds opening balances before any movement exists",
        ),
        (
            "stock_repo.rs",
            "read paths and the opening-balance upsert used at product creation",
        ),
    ]
    .into_iter()
    .collect();

    let mut offences: Vec<String> = Vec::new();
    for path in source_files(&crate_root().join("src"), &["rs"]) {
        let text = read(&path);
        if path
            .file_name()
            .is_some_and(|n| n == "tests.rs" || n.to_string_lossy().ends_with("_tests.rs"))
        {
            continue;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let writes_cache = [
            "INSERT INTO stock_levels",
            "UPDATE stock_levels",
            "DELETE FROM stock_levels",
        ]
        .iter()
        .any(|needle| text.contains(needle));
        if !writes_cache || allowed.contains_key(name.as_str()) {
            continue;
        }

        // Either it writes movements itself, or it delegates to the module that
        // writes the pair transactionally.
        let writes_ledger = text.contains("INSERT INTO stock_movements")
            || text.contains("movements::record_")
            || text.contains("movements::return_")
            || text.contains("record_merge_movements_tx")
            || text.contains("record_sale_movements_tx");
        if writes_ledger {
            continue;
        }

        let rel = path
            .strip_prefix(crate_root())
            .unwrap_or(&path)
            .display()
            .to_string()
            .replace('\\', "/");
        offences.push(rel);
    }

    assert!(
        offences.is_empty(),
        "these change a shelf count without recording a movement for it. A stock \
         change with no ledger entry is a quantity nobody can explain — write the \
         movement in the same transaction, or add the file to `allowed` with the \
         reason:\n  {}",
        offences.join("\n  ")
    );
}
