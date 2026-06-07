# ZANPOS Visual Gaps Audit — 2026-06-07

| # | Component | Gap | Severity | Exact Fix (file:line) |
|---|-----------|-----|----------|-----------------------|
| V01 | ProductGrid | Empty state is bare text "No products found." — no icon, no warmth, no call to action. First-launch cashier sees a void. | critical | `ProductGrid.tsx:43` — replace `<div className="product-grid-msg">No products found.</div>` with warm empty state: Lucide `PackageSearch` icon + "No products set up yet" heading + "Ask your manager to add products in Back Office" hint |
| V02 | AdminChatPage | 30+ tool entries use hardcoded hex colors (#6366f1, #0ea5e9, #f59e0b, #22c55e, #8b5cf6, #ef4444, #10b981) completely outside the CSS variable system | major | `AdminChatPage.tsx:76–130` — replace all `color: "#..."` with `color: "var(--accent)"` for CTAs, `var(--info)` for data reads, `var(--warning)"` for mutations, `var(--error)` for destructive; creates 6 semantic buckets instead of 30 arbitrary colors |
| V03 | All modals | `modal-in` animation exists (0.16s cubic-bezier) but there is **no `modal-out`** — every modal snaps closed with zero animation. The open feels premium; the close feels broken. | major | `App.css:1949` — add `@keyframes modal-out { from { opacity:1; transform: scale(1); } to { opacity:0; transform: scale(0.97); } }` and apply to `.modal-overlay.closing` via JS class toggle on close |
| V04 | AdminChatPage | Emoji used as functional tool-category icons: 📊 💲 📦 ⚠ 💰 ↩ 📋 ☁ 📅 📈 🏆 🕐 📂 🔒 🔗 ⏰ 👤 🔄 ✏ 🔍 🏷 | major | `AdminChatPage.tsx:76–130` — replace all emoji icon strings with Lucide icon component names (e.g., `BarChart2`, `DollarSign`, `Package`, `AlertTriangle`); render inline `<Icon size={14} />` in the tool-use chip |
| V05 | StickyNotesPanel | Sticky note colors hardcoded: `#fef08a`, `#bbf7d0`, `#bae6fd`, `#fbcfe8`, `#e9d5ff` — these are Tailwind palette values, not CSS variables | major | `StickyNotesPanel.tsx:9–13` — map to CSS variables: yellow→`var(--warning)`, green→`var(--success)`, blue→`var(--info)`, pink/purple add `--note-pink` and `--note-purple` to `:root` |
| V06 | CustomersTab / ReportsTab / DeliveriesTab | Empty states use raw emoji as the "icon" (`bo-empty-icon` class): 👤 📊 🧾 — emoji render differently across Windows versions and look informal in a business POS | minor | `CustomersTab.tsx:122`, `ReportsTab.tsx:283,319,384` — replace emoji spans with `<UserX size={32} />`, `<BarChart2 size={32} />`, `<Receipt size={32} />` from lucide-react, styled with `color: var(--text-muted)` |
| V07 | App.css — global | Three uses of `transition: all` (lines 2150, 2259, 2342) — `transition: all` triggers on every CSS property change including layout properties, causing repaints on scroll and filter operations | minor | `App.css:2150,2259,2342` — replace `transition: all` with explicit property lists: `transition: background 0.1s, border-color 0.1s, box-shadow 0.1s, transform 0.1s` |
| V08 | Design token system | No `--transition-fast` / `--transition-base` / `--transition-slow` CSS tokens — 28 different transition durations scattered (0.08s, 0.1s, 0.12s, 0.14s, 0.15s, 0.16s, 0.18s, 0.25s, 0.35s) with no system | minor | `App.css:1–110 (:root block)` — add: `--t-fast: 80ms ease; --t-base: 120ms ease; --t-slow: 220ms ease;` then do a global find-replace to standardise transitions |
| V09 | ProductGrid | No skeleton loading state while products fetch from DB — grid area is blank until Tauri command resolves | minor | `ProductGrid.tsx:42` — add a `isLoading` prop; when true render `<div className="product-grid">{Array(12).fill(0).map((_,i) => <div key={i} className="product-card-skeleton" />)}</div>` using existing `.skeleton-shimmer` animation |
| V10 | CartPanel | No visual success feedback after `finalize_sale` — the cart just clears. No pulse, no celebration, no "Sale complete" moment | minor | `CartPanel.tsx` — after successful finalize, briefly apply `.cart-success-flash` class to the cart panel (2 keyframe pulses of `box-shadow: 0 0 0 3px var(--success)`) before clearing |
| V11 | BarcodeInput | Error shake animation exists (scan-flash at `App.css:804`) but only flashes the border — no text feedback on the input itself for unknown barcode | minor | `App.css:798` — extend scan-flash to include `transform: translateX(0) → translateX(-4px) → translateX(4px) → translateX(0)` for a physical shake feel on failed scans |
| V12 | Global | `modal-in` uses `cubic-bezier(0.2, 0, 0, 1)` — an overshoot spring — which is premium. But buttons, cards, and dropdowns all use linear `ease` — inconsistent motion personality | minor | `App.css` — apply `cubic-bezier(0.2, 0, 0, 1)` to `.product-card:active`, `.btn:active`, and panel slide-ins for a unified spring-based motion language |

## Confirmed Good
- Scan-flash animation on barcode input (`App.css:798`) ✓
- Skeleton shimmer keyframe defined (`App.css:1062`) ✓  
- Modal-in animation with proper easing (`App.css:1949`) ✓
- DM Sans + Bricolage Grotesque font stack fully applied ✓
- Gold design system (--accent, --accent2) consistently used in main components ✓
- No hardcoded font-family inline styles found in .tsx files ✓
- BarcodeInput auto-focus is aggressive and well-implemented ✓

## Summary
12 findings: 1 critical, 4 major, 7 minor. The biggest wins are V01 (dead empty state on first launch), V02+V04 (AdminChatPage color/icon chaos), and V03 (jarring modal close). The transition token system (V08) would pay compound dividends across all future work.
