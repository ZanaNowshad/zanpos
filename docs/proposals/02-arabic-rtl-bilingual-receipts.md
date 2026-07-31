# Proposal 02 — Arabic / RTL UI + Bilingual Thermal Receipts

**Status:** Draft proposal · **Effort:** L · **Owner sign-off required**

> Design only. No code or schema changes are made by this document.

## (a) Problem & why it matters for Bahrain/Gulf retail

Arabic is the official language of Bahrain and the wider GCC. A POS sold to Gulf
retailers is expected to offer an Arabic interface for cashiers and, very
commonly, a bilingual (Arabic + English) receipt — customers and tax inspectors
both expect to read the store name, line items, and VAT labels in Arabic.

**Confirmed current state: ZANPOS has no internationalisation at all.** A search
across `src/` for `i18n`, `useTranslation`, `i18next`, `react-intl`,
`dir="rtl"`, and `lang="ar"` returned **no matches**. All UI strings are
hard-coded English literals in the ~50 components under `src/components/`, and
the receipt builder [`receiptLines.ts`](../../src/utils/receiptLines.ts) emits
single-language, left-to-right, 48-character ASCII lines. This is a greenfield
addition, which is why the effort is Large.

## (b) Proposed approach

Two separable workstreams; the receipt work is the harder half.

**UI layer.** Introduce a lightweight i18n runtime. Recommendation:
`react-i18next` — it is the de-facto React 19 standard, supports lazy namespace
loading (which fits the existing lazy-chunk strategy in `vite.config.ts`), and
its bundle is small. Wrap the app once, set `dir` on the document element from
the active locale, and replace hard-coded strings with translation keys
incrementally (POS checkout path first, back-office tabs second). RTL layout is
driven by `dir="rtl"` plus CSS logical properties; the design-system CSS
variables (per project memory) should already centralise spacing, so the
flip is mostly mechanical rather than per-component.

> Confirm the library against the project before adoption — per `AGENTS.md`
> conventions, nothing is assumed available until it is in `package.json`.
> `react-i18next` is **not currently a dependency.**

**Receipt layer.** This is where RTL bites hardest. The current printer path
builds plain text and relies on monospace column math (`pad()` in
`receiptLines.ts`, `W = 48`). Arabic introduces three problems:

1. **Encoding** — ESC/POS printers do not render UTF-8 Arabic from the default
   code page. The `build_receipt_bytes` function in
   [`thermal_commands.rs`](../../src-tauri/src/commands/thermal_commands.rs)
   writes `line.as_bytes()` directly. Arabic requires either selecting an Arabic
   ESC/POS code page (e.g. CP864 / Windows-1256, via `ESC t n`) and transcoding,
   or — more robustly — rendering the receipt to a **raster bitmap** and printing
   it as a `GS v 0` image. Bitmap printing sidesteps font/code-page and shaping
   problems entirely and is the recommended path for mixed Arabic+English.
2. **Shaping & bidi** — Arabic is cursive (contextual letter forms) and
   right-to-left, with bidirectional runs when prices/Latin SKUs are mixed in.
   The column-padding logic assumes 1 char = 1 fixed-width cell, which is false
   for shaped Arabic. The raster approach lets a real text layout engine handle
   shaping and bidi; the text path would need an Arabic shaping step.
3. **Bilingual layout** — line items would print product name in both languages
   (or per a store setting), and VAT labels become e.g. "ضريبة القيمة المضافة /
   VAT". This needs translated label constants and a product name source in
   Arabic (see data model).

## (c) Data-model changes

- `products` — add `name_ar TEXT` (nullable). Receipts and the catalog can fall
  back to `name` when `name_ar` is null. Mirrors the existing nullable text
  columns on `products`.
- `categories` — optional `name_ar TEXT` for a localised catalog grid.
- `branches` — `receipt_header`, `receipt_footer` already exist; add
  `receipt_header_ar`/`receipt_footer_ar` (nullable) so the bilingual receipt has
  Arabic header/footer text.
- `app_config` — a `ui_locale` key (e.g. `en` / `ar`) and a
  `receipt_language` key (`en` / `ar` / `bilingual`), following the existing
  `flag_*` / config-key convention seeded in
  [`0001_initial.sql`](../../src-tauri/migrations/0001_initial.sql).
- No change to money or numeric representation. (Arabic-Indic digit rendering, if
  wanted, is a *display* transform only — amounts stay `i64` minor units.)

## (d) Backend command surface (Tauri commands)

- `set_ui_locale(locale)` / `get_ui_locale()` — thin wrappers over `app_config`.
- `print_receipt_raster(sale_id, language)` — new command that renders the
  receipt to a bitmap server-side and prints via `write_to_port`, used when
  `receipt_language` involves Arabic. The existing `print_receipt_raw` stays for
  the English/Latin fast path.
- Product/category write commands extended to accept `name_ar`.

## (e) Frontend touchpoints

- New `src/i18n/` setup (provider, locale files `en.json` / `ar.json`).
- Document `dir`/`lang` set from locale (App root).
- Every component under `src/components/` with user-facing text (incremental).
- `ProductFormModal.tsx`, `CategoryFormModal.tsx` — `name_ar` input.
- `settings/StoreTab.tsx` and `ReceiptTab.tsx` — locale + receipt-language
  pickers, Arabic header/footer fields.
- [`receiptLines.ts`](../../src/utils/receiptLines.ts) — bilingual label map and
  language-aware item rendering for the on-screen preview.
- `WindowControls.tsx` / titlebar — mirror control placement under RTL (window is
  frameless per `AGENTS.md`).

## (f) Effort estimate

**L.** UI string extraction across ~50 components plus RTL CSS audit is itself
sizeable; the Arabic thermal receipt (raster rendering, shaping, code-page
handling) is a meaningful sub-project. Recommend phasing: (1) UI i18n scaffold +
RTL on the checkout path, (2) bilingual receipt, (3) full back-office
translation.

## (g) Risks & open questions for the product owner

- **Biggest open question:** *For Arabic receipts, do we commit to raster/bitmap
  printing (robust, printer-agnostic, but slower and image-based) or to ESC/POS
  Arabic code-page text (faster, but printer-dependent and fragile with mixed
  Arabic+Latin shaping)?* This single decision drives most of the receipt effort.
- Who supplies Arabic translations — in-house, or is machine translation
  acceptable for a first pass with human review of tax/legal labels?
- Are Arabic product names mandatory, or is English fallback acceptable when
  `name_ar` is empty (affects data-entry burden for existing catalogs)?
- RTL mirroring can collide with the custom frameless titlebar and any
  absolutely-positioned UI; needs a visual QA pass.
