# Proposal 01 — NBR / GCC E-Invoicing QR on Receipts

**Status:** Draft proposal · **Effort:** S–M · **Owner sign-off required**

> Design only. No code or schema changes are made by this document. All cited
> tables and files were read in the current `feature/ai-tools-suppliers-wip` tree.

## (a) Problem & why it matters for Bahrain/Gulf retail

GCC tax authorities are converging on machine-readable invoice data. Saudi
Arabia's ZATCA already mandates a base64 TLV (Tag-Length-Value) QR code on every
simplified tax invoice; the UAE FTA and Bahrain's NBR are moving the same
direction with phased e-invoicing programmes. A scannable QR lets an inspector
or customer verify the seller, VAT registration, timestamp, and tax amounts
without reading the printed text, and it is the single most common audit ask in
GCC retail.

ZANPOS is already most of the way there. The receipt builder
[`receiptLines.ts`](../../src/utils/receiptLines.ts) prints `TRN: <tax_number>`
(from `branches.tax_number`), a `Subtotal (excl. VAT)` line, a `VAT` line, and a
`TOTAL` line, all in integer minor units via `formatMoney()`. What is missing is
the encoded QR itself. This is therefore an *incremental* feature, not a new
subsystem.

## (b) Proposed approach

Adopt the ZATCA Phase-1 TLV scheme as the concrete encoding (it is the most
mature published GCC spec and is a reasonable superset for NBR until NBR
publishes its own field list — see open questions). The QR encodes a byte string
of consecutive TLV triples, then base64-encodes the whole buffer:

| Tag | Field | Source in ZANPOS |
|-----|-------|------------------|
| 1 | Seller name | `branches.name` |
| 2 | VAT registration number | `branches.tax_number` |
| 3 | Timestamp (ISO-8601) | `sales.sold_at` |
| 4 | Invoice total (incl. VAT) | `sales.net_total_minor`, formatted to 3 dp |
| 5 | VAT total | `sales.tax_total_minor`, formatted to 3 dp |

Generation must happen in **Rust**, not the frontend, for two reasons grounded
in the codebase: (1) money is `i64` minor units and the amount strings must be
produced with the same half-up formatting the rest of the system uses
(`domain::money::format_minor`), and (2) the strict CSP documented in
[`AGENTS.md`](../../AGENTS.md) (`default-src 'self'`) means the frontend should
receive a finished payload, not pull a QR library or remote service. A new Rust
command returns either the base64 TLV string (frontend renders it with the
already-bundled `jsbarcode`/QR path) or a pre-rasterised module matrix.

For the thermal path, ESC/POS printers support a native GS-code QR symbol. The
byte-builder in [`thermal_commands.rs`](../../src-tauri/src/commands/thermal_commands.rs)
(`build_receipt_bytes`) currently emits only text; it would gain a helper that
appends the `GS ( k` QR command sequence (store data → set size → print) after
the totals block. Crucially the existing `MAX_RECEIPT_LINES` truncation and
RTS/CTS flow control already protect the buffer, so the QR bytes ride the same
`write_to_port` path.

## (c) Data-model changes

Minimal. The QR is derived data, so storing it is optional but recommended for
reprint fidelity and audit:

- `sales` — add nullable `einvoice_qr_tlv TEXT` (base64 payload captured at
  finalize time, frozen against later branch-name edits). Follows the existing
  nullable-column-with-default pattern; no migration to existing rows needed
  because it is nullable.
- `branches` — reuse existing `tax_number`; optionally add
  `einvoice_scheme TEXT NOT NULL DEFAULT 'zatca_p1'` so the encoder can branch
  if NBR later diverges.
- No new tables. No change to money representation.

## (d) Backend command surface (Tauri commands)

- `einvoice_build_qr(sale_id) -> { tlv_base64, fields }` — pure read; rebuilds
  the payload from `sales` + `branches` (for preview and reprint).
- The receipt-print command (`print_receipt_raw` in `thermal_commands.rs`) gains
  an optional `qr_tlv: Option<String>` argument so the QR bytes are appended
  server-side; the on-screen `ReceiptPreview` renders the same payload.
- Capture path: when a sale is finalized (`sale_repo::finalize_sale`), persist
  the frozen `einvoice_qr_tlv`. No new public command — internal write.

## (e) Frontend touchpoints

- [`receiptLines.ts`](../../src/utils/receiptLines.ts) — add a QR placeholder
  marker so the on-screen preview shows the code position.
- [`ReceiptPreview.tsx`](../../src/components/ReceiptPreview.tsx) and
  `ReceiptDesignEditor.tsx` — render the QR image (reuse the lazy `jsbarcode`
  chunk noted in `vite.config.ts`, or a small QR module).
- `src/tauri/commands.ts` — add the `einvoice_build_qr` wrapper.
- `SetupWizard.tsx` / `settings/StoreTab.tsx` — surface a "VAT QR on receipts"
  toggle and validate that `tax_number` is present before enabling.

## (f) Effort estimate

**S–M.** The TLV encoder + base64 is a few dozen lines and pure. The ESC/POS QR
sequence and the preview rendering are the bulk of the work. No schema migration
is strictly required if the QR is rebuilt on demand; the optional `sales` column
nudges it toward M.

## (g) Risks & open questions for the product owner

- **Biggest open question:** *Which authority's field schema do we target on day
  one — ZATCA Phase-1 TLV as-is, or do we wait for NBR Bahrain's published
  field list?* This determines tags, date format, and whether a cryptographic
  stamp (ZATCA Phase-2 XML signing) is in scope. The proposal assumes ZATCA
  Phase-1 TLV with no signature.
- Phase-2 ZATCA adds signed XML invoices and a CSID — explicitly **out of scope**
  here; confirm Bahrain does not require signing for simplified receipts yet.
- `branches.tax_number` is free-text and not format-validated today; a malformed
  TRN would produce a technically-valid-but-wrong QR.
- Thermal QR rendering varies by printer firmware; the `GS ( k` model 2 symbol is
  widely supported on Epson/Star but should be verified on the customer's actual
  hardware (the project supports both serial and Windows-spooler printers).
