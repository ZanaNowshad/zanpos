# ZANPOS 2.0.0 — Release Notes

**Artifact:** `ZANPOS_2.0.0_x64-setup.exe`
**SHA-256:** `bf0fead21a108e8628791eec70f7b6007caec6ed0607fb07bd234bb2118ccdac`
**Built:** 2026-08-13 02:16:25 · unsigned · x64 NSIS

Supersedes the earlier build `11ef9fecfec274209a20b804a1dc28a2b9e9c0051c0edc5dec8d3142650a6dd2`, which must not be used.

## What changed in this build

### Accessibility
Clickable status pills in the command header and `StatusPill` were spans with
hand-rolled key handlers; the header's only listened for Enter, so Space — which
every user expects on something announced as a button — did nothing. Both now
render a native `<button>` when they have an action and a plain
`<span role="status">` when they do not.

The confirm dialog dismissed on backdrop click through a handler on a
presentational `<div>`, reachable by mouse only. The backdrop is now a real
button kept out of the tab order and the accessibility tree, since it only
duplicates Escape and the close button. The dialog's `stopPropagation` became
dead code in the process and was removed.

`lint:a11y` now passes with zero errors.

### Catalogue
Category and Cost columns added. `cost_minor` was missing from the TypeScript
`AdminProduct` even though `admin_list_products` already returned it. A counts
strip reports the server's total for the filtered catalogue, with low/out-of-stock
figures explicitly labelled "on this page" because inventory has no server-side
aggregate.

### Inventory
The screen was unreachable: the dev mock had no `inventory_get_levels_paged`
fixture, so it threw on `.items` and had never been visually reviewed. With it
visible, the page inset turned out to sit only on the toolbar, leaving the table
and pagination running hard into the workspace edge. Both fixed.

### Purchasing
Two EN column headers were lowercase while every sibling was Title Case. Row
actions moved onto the shared row-action style.

### System Health
The primary action tile was pinned to a hardcoded blue through a rule overriding
an earlier correct one, so it ignored the selected theme. It now follows the
theme accent.

### Correctness
Nine `react-hooks/exhaustive-deps` warnings resolved — four by adding a genuinely
missing dependency (which then exposed an unnecessary one), five suppressed with
stated reasons where adding the dependency would loop or break intent, including
two self-referential callbacks that render retry controls bound to themselves.

## Known limitations

- **Unsigned.** SmartScreen will warn.
- **Install not tested** — no isolated environment on this machine. See INSTALL.md.
- **Rust test suite not re-run in this session** — a running elevated dev process
  holds `src-tauri/target/debug`, which `cargo test` must write to. Last known
  result 421/421.
- POS register and payment-dialog visual parity remain uncompared; the reference
  images were never available in an inspectable form.
