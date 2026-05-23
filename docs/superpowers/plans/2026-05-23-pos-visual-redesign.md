# POS Visual Redesign — 10/10 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the 30-point POS visual redesign spec to reach 10/10 quality on layout clarity, visual hierarchy, state communication, typography consistency, and operational feedback.

**Architecture:** Primarily CSS changes in `src/App.css` (design tokens + component styles), with targeted React changes in `BarcodeInput.tsx` (hint placement), `CartPanel.tsx` (discount/totals logic), and `PosPage.tsx` (top-bar grouping + action-bar grouping + Clear disabled logic). No new Rust or Tauri changes required.

**Tech Stack:** React 19, TypeScript, Tauri v2, CSS custom properties, Inter + IBM Plex Mono + Syne fonts, Lucide icons (already installed).

---

## File Map

| File | Changes |
|------|---------|
| `src/App.css` | Design tokens, top-bar, scan input, category toolbar, product cards, cart, totals, payment buttons, action bar, sidebar, status bar, state/disabled/locked styles |
| `src/components/BarcodeInput.tsx` | Move hint text below input (structural change) |
| `src/components/CartPanel.tsx` | Discount neutral when zero; tabular-nums on money; payment button disabled tooltip |
| `src/pages/PosPage.tsx` | Top-bar three-group layout; action-bar three-group JSX; Clear disabled when empty |

---

## Task 1 — Design Tokens: colours, spacing, radius, typography

**Files:**
- Modify: `src/App.css` — replace/extend `:root` and `[data-theme="light"]` blocks

- [ ] **Step 1: Add semantic colour tokens to `:root`**

In `src/App.css`, inside the `:root { ... }` block (around line 10), add these tokens **after** the existing ones:

```css
  /* ── Semantic focus / interactive ── */
  --color-focus:        #2563EB;
  --color-focus-ring:   rgba(37,99,235,0.18);

  /* ── Soft fills for state surfaces ── */
  --color-success-soft: rgba(34,197,94,0.10);
  --color-danger-soft:  rgba(239,68,68,0.10);
  --color-warning-soft: rgba(251,191,36,0.12);
  --color-info-soft:    rgba(96,165,250,0.10);

  /* ── Spacing scale (8px grid) ── */
  --space-1: 4px;
  --space-2: 8px;
  --space-3: 12px;
  --space-4: 16px;
  --space-5: 20px;
  --space-6: 24px;
  --space-8: 32px;

  /* ── Radius scale ── */
  --radius-sm:   8px;
  --radius-md:   12px;
  --radius-lg:   16px;
  --radius-xl:   20px;
  --radius-pill: 999px;
```

- [ ] **Step 2: Add same tokens to `[data-theme="light"]` block**

In `src/App.css`, inside `[data-theme="light"] { ... }` (around line 65), add:

```css
  --color-focus:        #2563EB;
  --color-focus-ring:   rgba(37,99,235,0.16);
  --color-success-soft: rgba(22,163,74,0.08);
  --color-danger-soft:  rgba(220,38,38,0.08);
  --color-warning-soft: rgba(217,119,6,0.10);
  --color-info-soft:    rgba(37,99,235,0.08);
```

(The spacing and radius tokens are theme-neutral, so they only need to be in `:root`.)

- [ ] **Step 3: Add global tabular-nums utility**

After the `.money { ... }` rule (around line 154), add:

```css
/* ── Tabular numerics — apply to all currency/numeric values ── */
.num {
  font-variant-numeric: tabular-nums;
  font-family: var(--font-mono);
}
```

- [ ] **Step 4: Commit**

```
git add src/App.css
git commit -m "style(tokens): add semantic colour, spacing, radius, and tabular-nums tokens"
```

---

## Task 2 — Top Bar: three-group layout, 56px height, cleaner hierarchy

**Files:**
- Modify: `src/App.css` — `.top-bar` and related rules
- Modify: `src/pages/PosPage.tsx` — top-bar JSX structure

- [ ] **Step 1: Restructure top-bar JSX in `PosPage.tsx`**

Find the top-bar section (around line 449):
```jsx
      {/* ── Top bar ── */}
      <div className="top-bar">
        <span className="top-bar-logo">ZAN<span>POS</span></span>
        ...
      </div>
```

Replace the entire `<div className="top-bar"> ... </div>` block with:

```jsx
      {/* ── Top bar ── */}
      <div className="top-bar">
        {/* Left: brand */}
        <div className="top-bar-left">
          <span className="top-bar-logo">ZAN<span>POS</span></span>
          <span className="top-bar-sep">·</span>
          <span className="top-bar-branch">{DEVICE.branch_name}</span>
        </div>

        {/* Centre: operational status pills */}
        <div className="top-bar-center">
          <span className="top-bar-pill top-bar-pill-success">Shift Open</span>
          <SyncChip status={syncStatus} />
          <WhatsAppStatusPill
            sessionRole={sessionUser.role_name}
            onOpenQR={() => setShowWaQR(true)}
          />
        </div>

        {/* Right: time + user + actions */}
        <div className="top-bar-right">
          <span className="top-bar-time">{clockTime}</span>
          <span className="top-bar-cashier">{sessionUser.display_name}</span>
          {lastReceiptNumber && (
            <button className="top-bar-btn" onClick={handleReprintLast} title={`Reprint #${lastReceiptNumber} (Ctrl+P)`}>
              Reprint
            </button>
          )}
          {onToggleTheme && (
            <button
              className="top-bar-btn top-bar-theme"
              onClick={onToggleTheme}
              title={theme === "dark" ? "Switch to Light Mode" : "Switch to Dark Mode"}
            >
              {theme === "dark" ? "☀" : "🌙"}
            </button>
          )}
          <button className="top-bar-btn top-bar-btn-danger" onClick={() => setShowShiftClose(true)}>
            Close Shift
          </button>
          <button className="top-bar-btn top-bar-logout" onClick={onLogout} title="Ctrl+L">
            Logout
          </button>
        </div>
      </div>
```

- [ ] **Step 2: Update top-bar CSS**

In `src/App.css`, find and replace the entire `.top-bar` block and all `.top-bar-*` rules (lines ~166–205) with:

```css
/* ── Top bar ── */
.top-bar {
  display: flex;
  align-items: center;
  height: 56px;
  padding: 0 var(--space-4);
  background: var(--surface);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
  gap: var(--space-4);
}

.top-bar-left {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  min-width: 0;
  flex-shrink: 0;
}

.top-bar-center {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex: 1;
  justify-content: center;
}

.top-bar-right {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex-shrink: 0;
}

.top-bar-logo {
  font-family: var(--font-display);
  font-size: 1rem;
  font-weight: 800;
  letter-spacing: 0.04em;
  color: var(--text);
  white-space: nowrap;
  flex-shrink: 0;
}
.top-bar-logo span { color: var(--gold); }

.top-bar-sep { color: var(--border); margin: 0 2px; }

.top-bar-branch {
  font-weight: 600;
  font-size: 0.88rem;
  color: var(--text);
  max-width: 320px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* Status pill — compact, pill-shaped */
.top-bar-pill {
  height: 26px;
  padding: 0 10px;
  border-radius: var(--radius-pill);
  font-size: 0.76rem;
  font-weight: 600;
  display: inline-flex;
  align-items: center;
  gap: 5px;
  white-space: nowrap;
}
.top-bar-pill-success {
  background: var(--color-success-soft);
  color: var(--success);
}
.top-bar-pill-warning {
  background: var(--color-warning-soft);
  color: var(--warning);
}
.top-bar-pill-danger {
  background: var(--color-danger-soft);
  color: var(--error);
}

.top-bar-time {
  color: var(--text);
  font-family: var(--font-mono);
  font-size: 1rem;
  font-weight: 700;
  letter-spacing: 0.06em;
  font-variant-numeric: tabular-nums;
  margin-right: var(--space-1);
}

.top-bar-cashier {
  color: var(--text-dim);
  font-size: 0.76rem;
  padding: 3px 9px;
  background: var(--surface2);
  border-radius: var(--radius-sm);
  border: 1px solid var(--border);
  letter-spacing: 0.02em;
  white-space: nowrap;
}

.top-bar-btn {
  height: 32px;
  padding: 0 12px;
  border-radius: var(--radius-md);
  border: 1px solid var(--border);
  background: var(--surface2);
  color: var(--text-dim);
  font-size: 0.78rem;
  font-weight: 600;
  cursor: pointer;
  font-family: var(--font);
  white-space: nowrap;
  transition: background 0.1s, color 0.1s;
}
.top-bar-btn:hover { background: var(--surface3); color: var(--text); }

.top-bar-btn-danger {
  background: var(--color-danger-soft);
  border-color: rgba(239,68,68,0.3);
  color: var(--error);
}
.top-bar-btn-danger:hover { background: rgba(239,68,68,0.18); color: var(--error); }

.top-bar-logout { color: var(--text-muted); }
.top-bar-theme  { font-size: 1rem; padding: 0 8px; }
```

- [ ] **Step 3: Verify TypeScript compiles**

```
npx tsc --noEmit 2>&1
```
Expected: no errors related to PosPage.tsx top-bar section.

- [ ] **Step 4: Commit**

```
git add src/App.css src/pages/PosPage.tsx
git commit -m "style(top-bar): three-group layout, 56px height, pill status, cleaner hierarchy"
```

---

## Task 3 — Scan Input: fix hint placement, larger touch target

**Files:**
- Modify: `src/components/BarcodeInput.tsx` — move hint outside input row
- Modify: `src/App.css` — barcode input styles

- [ ] **Step 1: Update `BarcodeInput.tsx` JSX — hint below input**

Find the `return (` block in `BarcodeInput.tsx` (line 104). Replace the entire return with:

```tsx
  return (
    <div className="barcode-input-wrap">
      <div className="barcode-input-row">
        <span className="barcode-input-icon">
          <ScanBarcode size={18} strokeWidth={1.75} />
        </span>
        <input
          ref={inputRef}
          className={`barcode-input${scanState ? ` scan-${scanState}` : ""}`}
          type="text"
          value={value}
          placeholder="Scan barcode or search product…"
          onChange={handleChange}
          onKeyDown={handleKeyDown}
          disabled={disabled}
          autoComplete="off"
          autoCorrect="off"
          spellCheck={false}
        />
        <kbd className="barcode-input-kbd">F2</kbd>
      </div>
      <p className="barcode-input-hint">Tip: type <code>3*barcode</code> to add a quantity</p>
    </div>
  );
```

- [ ] **Step 2: Update barcode input CSS in `App.css`**

Find and replace the entire `/* ── Barcode input ── */` section (lines ~311–378) with:

```css
/* ── Scan / search input ── */
.barcode-input-wrap {
  padding: var(--space-3) var(--space-4) var(--space-2);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
  background: var(--surface);
}

.barcode-input-row {
  position: relative;
  display: flex;
  align-items: center;
}

.barcode-input-icon {
  position: absolute;
  left: 16px;
  color: var(--text-muted);
  pointer-events: none;
  z-index: 1;
  display: flex;
  align-items: center;
}

.barcode-input-kbd {
  position: absolute;
  right: 14px;
  color: var(--text-muted);
  font-size: 0.72rem;
  pointer-events: none;
  font-family: var(--font-mono);
  border: 1px solid var(--border);
  border-bottom-width: 2px;
  border-radius: 5px;
  padding: 2px 6px;
  letter-spacing: 0.02em;
  background: var(--surface2);
}

.barcode-input {
  width: 100%;
  height: 52px;
  padding: 0 52px 0 44px;
  background: var(--bg);
  border: 1.5px solid var(--border);
  border-radius: var(--radius-lg);
  color: var(--text);
  font-size: 1.1rem;
  font-family: var(--font);
  font-weight: 500;
  outline: none;
  transition: border-color 0.12s, box-shadow 0.12s;
}
.barcode-input:focus {
  border-color: var(--color-focus);
  box-shadow: 0 0 0 3px var(--color-focus-ring);
}
.barcode-input::placeholder {
  color: var(--text-muted);
  font-size: 1rem;
  font-weight: 400;
}
.barcode-input:disabled { opacity: 0.4; }
.barcode-input.scan-success {
  border-color: var(--success);
  box-shadow: 0 0 0 3px rgba(34,197,94,0.18);
  animation: scan-flash 0.5s ease-out forwards;
}
.barcode-input.scan-error {
  border-color: var(--error);
  box-shadow: 0 0 0 3px rgba(239,68,68,0.18);
}
@keyframes scan-flash {
  0%   { box-shadow: 0 0 0 4px rgba(34,197,94,0.3); }
  100% { box-shadow: 0 0 0 3px rgba(34,197,94,0.18); }
}

.barcode-input-hint {
  margin-top: 5px;
  font-size: 0.72rem;
  color: var(--text-muted);
  padding-left: 4px;
  line-height: 1.4;
}
.barcode-input-hint code {
  font-family: var(--font-mono);
  font-size: 0.7rem;
  background: var(--surface2);
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 0 4px;
}
```

- [ ] **Step 3: Commit**

```
git add src/components/BarcodeInput.tsx src/App.css
git commit -m "style(scan): fix hint below input, 52px height, blue focus ring"
```

---

## Task 4 — Category Toolbar: Custom Item separated, pill-shaped tabs

**Files:**
- Modify: `src/App.css` — category tab rules

The JSX already has the correct structure (`cat-tab-custom` separated by `cat-tab-divider`). Only CSS changes are needed.

- [ ] **Step 1: Replace category tab CSS in `App.css`**

Find the `/* ── Category / filter toolbar ── */` or `.category-tabs` section and replace entirely:

```css
/* ── Category / filter toolbar ── */
.category-tabs {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-4);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
  overflow-x: auto;
  scrollbar-width: none;
}
.category-tabs::-webkit-scrollbar { display: none; }

.cat-tab {
  height: 34px;
  padding: 0 14px;
  border-radius: var(--radius-pill);
  border: 1px solid transparent;
  background: transparent;
  color: var(--text-muted);
  font-size: 0.8rem;
  font-weight: 600;
  font-family: var(--font);
  cursor: pointer;
  white-space: nowrap;
  transition: background 0.12s, color 0.12s, border-color 0.12s;
  flex-shrink: 0;
}
.cat-tab:hover { background: var(--surface2); color: var(--text-dim); }
.cat-tab-active {
  background: var(--accent2);
  color: var(--accent);
  border-color: rgba(240,165,0,0.25);
}
[data-theme="light"] .cat-tab-active {
  background: #FFF4E5;
  color: #B45309;
  border-color: rgba(180,83,9,0.2);
}

/* Custom Item — action button, not a tab */
.cat-tab-custom {
  height: 34px;
  padding: 0 14px;
  border-radius: var(--radius-md);
  border: 1px solid rgba(240,165,0,0.35);
  background: var(--accent2);
  color: var(--accent);
  font-size: 0.8rem;
  font-weight: 700;
  font-family: var(--font);
  cursor: pointer;
  white-space: nowrap;
  flex-shrink: 0;
  transition: background 0.12s, border-color 0.12s;
}
.cat-tab-custom:hover { background: rgba(240,165,0,0.18); border-color: rgba(240,165,0,0.5); }
[data-theme="light"] .cat-tab-custom {
  background: #FFF7ED;
  border-color: rgba(180,83,9,0.25);
  color: #B45309;
}

/* Divider between action and category tabs */
.cat-tab-divider {
  width: 1px;
  height: 20px;
  background: var(--border);
  flex-shrink: 0;
  margin: 0 var(--space-1);
}

.cat-tab-spacer { flex: 1; min-width: var(--space-2); }

/* "Show unavailable" toggle — right-aligned */
.unavailable-toggle {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 0.76rem;
  color: var(--text-muted);
  cursor: pointer;
  flex-shrink: 0;
  white-space: nowrap;
  user-select: none;
}
.unavailable-toggle input[type="checkbox"] {
  width: 14px;
  height: 14px;
  accent-color: var(--accent);
  cursor: pointer;
}
```

- [ ] **Step 2: Commit**

```
git add src/App.css
git commit -m "style(toolbar): pill-shaped category tabs, Custom Item as action button"
```

---

## Task 5 — Product Cards: denser layout, richer typography, state feedback

**Files:**
- Modify: `src/App.css` — product card rules
- Modify: `src/components/ProductGrid.tsx` — remove image slot from card markup

- [ ] **Step 1: Remove image slot from `ProductGrid.tsx`**

In `src/components/ProductGrid.tsx`, the card JSX currently uses `product-card-body` inside the button. The existing markup already has no `product-card-img-wrap` — verify by reading lines 44–68. If the image slot is not present, no change needed. If it is present, remove the `<div className="product-card-img-wrap">` block entirely. The card should render as:

```tsx
          <button
            key={p.product_id}
            className={`product-card${isOos ? " product-card-oos" : ""}${inCart ? " in-cart" : ""}`}
            onClick={() => !isOos && onSelect(p)}
            disabled={isOos}
            aria-disabled={isOos}
          >
            <div className="product-card-body">
              <span className="product-card-name">{p.name}</span>
              <span className="product-card-price">
                {DEVICE.currency} {formatMoney(p.price_minor, DEVICE.currency_exponent)}
              </span>
              <span className="product-card-sku">{p.sku || p.barcode || "No SKU"}</span>
              {stockBadge(p)}
            </div>
          </button>
```

- [ ] **Step 2: Replace product card CSS in `App.css`**

Find and replace the `/* ── Product grid ── */` and `/* ── Product card ── */` sections (lines ~406–563):

```css
/* ── Product grid ── */
.product-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(178px, 1fr));
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
  overflow-y: auto;
  flex: 1;
  align-content: start;
  content-visibility: auto;
}

.product-grid-msg {
  grid-column: 1 / -1;
  padding: 48px 24px;
  color: var(--text-muted);
  text-align: center;
  font-size: 0.88rem;
}

/* Product card — compact, vertical, touch-friendly */
.product-card {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: var(--space-3) var(--space-3) var(--space-2);
  cursor: pointer;
  text-align: left;
  display: flex;
  flex-direction: column;
  min-height: 130px;
  gap: 0;
  transition: border-color 0.12s, background 0.12s, box-shadow 0.12s, transform 0.08s;
  position: relative;
  overflow: hidden;
}
.product-card:hover {
  background: var(--surface2);
  border-color: rgba(240,165,0,0.32);
  box-shadow: 0 4px 12px rgba(15,23,42,0.10);
  transform: translateY(-1px);
}
.product-card:active {
  transform: scale(0.985);
  background: var(--surface3);
  box-shadow: none;
}

/* In-cart: amber accent border */
.product-card.in-cart {
  border-color: rgba(240,165,0,0.5);
  background: rgba(240,165,0,0.04);
}

/* Out-of-stock */
.product-card-oos {
  opacity: 0.4;
  cursor: not-allowed;
  pointer-events: none;
}
.product-card-oos:hover { transform: none; box-shadow: none; }

.product-card-body {
  display: flex;
  flex-direction: column;
  gap: 3px;
  flex: 1;
}

.product-card-name {
  font-weight: 700;
  font-size: 0.88rem;
  line-height: 1.3;
  color: var(--text);
  overflow: hidden;
  display: -webkit-box;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
  margin-bottom: 2px;
}

.product-card-price {
  color: var(--gold);
  font-size: 1.1rem;
  font-weight: 800;
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
  letter-spacing: -0.01em;
  line-height: 1.2;
}

.product-card-sku {
  color: var(--text-muted);
  font-size: 0.68rem;
  letter-spacing: 0.04em;
  font-family: var(--font-mono);
  margin-top: 1px;
}

/* Stock badges */
.stock-badge {
  display: inline-flex;
  align-items: center;
  font-size: 0.66rem;
  font-weight: 700;
  letter-spacing: 0.03em;
  padding: 2px 7px;
  border-radius: var(--radius-sm);
  margin-top: var(--space-1);
  width: fit-content;
}
.stock-badge-out {
  background: var(--color-danger-soft);
  color: var(--error);
  border: 1px solid rgba(239,68,68,0.22);
}
.stock-badge-low {
  background: var(--color-warning-soft);
  color: var(--warning);
  border: 1px solid rgba(251,191,36,0.25);
}
.stock-badge-in {
  background: var(--color-success-soft);
  color: var(--success);
  border: 1px solid rgba(34,197,94,0.2);
}

/* Skeleton loader */
.product-card-skeleton {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  min-height: 130px;
  overflow: hidden;
}
.product-card-skeleton-img { display: none; }
.product-card-skeleton-body { padding: var(--space-3); display: flex; flex-direction: column; gap: var(--space-2); }
.skeleton {
  background: linear-gradient(90deg, var(--surface2) 25%, var(--surface3) 50%, var(--surface2) 75%);
  background-size: 200% 100%;
  animation: skeleton-shimmer 1.4s infinite;
  border-radius: var(--radius-sm);
}
.skeleton-text    { height: 14px; width: 80%; }
.skeleton-text-sm { height: 12px; width: 50%; }
@keyframes skeleton-shimmer {
  0%   { background-position: 200% 0; }
  100% { background-position: -200% 0; }
}
```

- [ ] **Step 3: Commit**

```
git add src/App.css src/components/ProductGrid.tsx
git commit -m "style(products): denser cards, 1.1rem price, tabular-nums, richer states"
```

---

## Task 6 — Cart Panel: empty state, totals discipline, tabular numerals

**Files:**
- Modify: `src/components/CartPanel.tsx` — discount neutral when zero, tabular-nums
- Modify: `src/App.css` — cart styles

- [ ] **Step 1: Fix discount row in `CartPanel.tsx`**

In `CartPanel.tsx`, find the totals block (around lines 82–100). Change the discount row so it only uses colour when non-zero:

```tsx
      {/* Totals */}
      <div className="cart-totals">
        <div className="cart-total-row cart-subtotal-row">
          <span>Subtotal</span>
          <span className="num">{fmt(grossTotal)}</span>
        </div>
        {totalDiscount > 0 && (
          <div className="cart-total-row cart-discount-row cart-discount-active">
            <span>Discount</span>
            <span className="num">−{fmt(totalDiscount)}</span>
          </div>
        )}
        {totalDiscount === 0 && (
          <div className="cart-total-row cart-discount-row">
            <span>Discount</span>
            <span className="num cart-total-neutral">{fmt(0)}</span>
          </div>
        )}
        <div className="cart-total-row">
          <span>Tax</span>
          <span className="num">{fmt(taxTotal)}</span>
        </div>
        <div className="cart-total-row cart-net-total">
          <span>TOTAL</span>
          <span className="num cart-grand-total">{fmt(netTotal)}</span>
        </div>
      </div>
```

- [ ] **Step 2: Replace cart CSS in `App.css`**

Find and replace all cart panel rules (search for `.cart-panel`, `.cart-lines`, `.cart-totals`, `.cart-method-row`, `.cart-pay-fast-btn` etc.) with:

```css
/* ── Cart panel ── */
.cart-panel {
  display: flex;
  flex-direction: column;
  background: var(--surface);
  border-left: 1px solid var(--border);
  overflow: hidden;
}

.cart-panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-4) var(--space-5);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}
.cart-panel-title {
  font-size: 1.1rem;
  font-weight: 800;
  color: var(--text);
}
.cart-item-count {
  font-size: 0.78rem;
  font-weight: 500;
  color: var(--text-muted);
  margin-left: var(--space-2);
}
.cart-recent-hint {
  font-size: 0.72rem;
  color: var(--text-muted);
  background: var(--surface2);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  padding: 2px 8px;
}

/* Lines */
.cart-lines {
  flex: 1;
  overflow-y: auto;
  padding: var(--space-2) var(--space-3);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

/* Processing banner */
.cart-processing-banner {
  background: var(--color-warning-soft);
  border: 1px solid rgba(251,191,36,0.3);
  color: var(--warning);
  border-radius: var(--radius-md);
  padding: var(--space-2) var(--space-3);
  font-size: 0.78rem;
  font-weight: 600;
  text-align: center;
}

/* Empty state */
.cart-empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  padding: var(--space-8);
  color: var(--text-muted);
}
.cart-empty-icon { font-size: 2.2rem; opacity: 0.5; }
.cart-empty-label { font-size: 0.9rem; font-weight: 600; color: var(--text-dim); }
.cart-empty-hint  { font-size: 0.76rem; text-align: center; line-height: 1.5; }
.cart-empty-hint kbd { margin: 0 2px; }

/* Line row */
.cart-line-wrap {
  background: var(--surface2);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  padding: var(--space-2) var(--space-3);
  display: flex;
  align-items: stretch;
  gap: var(--space-2);
  transition: border-color 0.1s;
  min-height: 76px;
}
.cart-line-wrap:hover { border-color: rgba(240,165,0,0.25); }
.cart-line-recent { border-color: rgba(240,165,0,0.4); background: rgba(240,165,0,0.04); }

.cart-line-name-btn {
  flex: 1;
  background: none;
  border: none;
  cursor: pointer;
  text-align: left;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.cart-line-name-btn:disabled { cursor: default; }

.cart-line-name {
  font-size: 0.84rem;
  font-weight: 600;
  color: var(--text);
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.cart-line-note { font-size: 0.72rem; color: var(--text-muted); font-style: italic; }
.cart-line-sku  { font-size: 0.68rem; color: var(--text-muted); font-family: var(--font-mono); }
.cart-line-unit { font-size: 0.72rem; color: var(--text-dim); font-family: var(--font-mono); font-variant-numeric: tabular-nums; }

.cart-line-controls {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  flex-shrink: 0;
}

.cart-qty-btn {
  width: 28px;
  height: 28px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border);
  background: var(--surface3);
  color: var(--text-dim);
  cursor: pointer;
  font-size: 1rem;
  font-weight: 700;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: background 0.1s;
  flex-shrink: 0;
}
.cart-qty-btn:hover { background: var(--surface4); color: var(--text); }
.cart-qty-btn:disabled { opacity: 0.35; cursor: not-allowed; }

.cart-qty-val {
  min-width: 24px;
  text-align: center;
  font-size: 0.88rem;
  font-weight: 700;
  color: var(--text);
  font-variant-numeric: tabular-nums;
}

.cart-line-total-col {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 1px;
  min-width: 64px;
}
.cart-line-discount {
  font-size: 0.68rem;
  color: var(--success);
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
}
.cart-line-total {
  font-size: 0.88rem;
  font-weight: 700;
  color: var(--gold);
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
}

.cart-line-del-btn {
  width: 28px;
  height: 28px;
  border-radius: var(--radius-sm);
  border: none;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  font-size: 1.1rem;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: background 0.1s, color 0.1s;
  flex-shrink: 0;
}
.cart-line-del-btn:hover { background: var(--color-danger-soft); color: var(--error); }
.cart-line-del-btn:disabled { opacity: 0.3; cursor: not-allowed; }

/* Totals */
.cart-totals {
  padding: var(--space-3) var(--space-5);
  border-top: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  flex-shrink: 0;
}

.cart-total-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 0.84rem;
  font-weight: 500;
  color: var(--text-dim);
}
.cart-subtotal-row { font-size: 0.84rem; }

/* Discount only coloured when non-zero */
.cart-discount-row .num { color: var(--text-muted); }
.cart-discount-active .num { color: var(--success) !important; }
.cart-total-neutral { color: var(--text-muted) !important; }

.cart-net-total {
  font-size: 0.94rem;
  font-weight: 800;
  color: var(--text);
  padding-top: var(--space-2);
  border-top: 1px solid var(--border);
  margin-top: var(--space-1);
}
.cart-grand-total {
  font-size: 1.8rem !important;
  font-weight: 900 !important;
  color: var(--gold) !important;
  letter-spacing: -0.02em;
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
}

/* Locked cart */
.cart-panel-locked .cart-lines { opacity: 0.6; pointer-events: none; }
```

- [ ] **Step 3: Commit**

```
git add src/App.css src/components/CartPanel.tsx
git commit -m "style(cart): tabular-nums, neutral discount when zero, 1.8rem grand total"
```

---

## Task 7 — Payment Buttons: height, disabled states, locked vs. empty

**Files:**
- Modify: `src/App.css` — payment button rules

- [ ] **Step 1: Replace payment button CSS in `App.css`**

Find and replace all `.cart-method-row`, `.cart-method-btn`, `.cart-pay-fast-btn`, `.cart-pay-split-btn` rules:

```css
/* ── Payment buttons ── */
.cart-method-row {
  display: flex;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  border-top: 1px solid var(--border);
  flex-shrink: 0;
}

.cart-method-btn {
  flex: 1;
  height: 52px;
  border-radius: var(--radius-md);
  border: 1.5px solid var(--border);
  background: var(--surface2);
  color: var(--text);
  font-size: 0.88rem;
  font-weight: 700;
  font-family: var(--font);
  cursor: pointer;
  transition: background 0.12s, border-color 0.12s, transform 0.08s;
}
.cart-method-btn:hover:not(:disabled) { background: var(--surface3); border-color: rgba(240,165,0,0.3); }
.cart-method-btn:active:not(:disabled) { transform: scale(0.97); }
.cart-method-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}
/* Colour-coded payment methods */
.cart-method-btn[title*="Cash"]:not(:disabled)   { border-color: rgba(34,197,94,0.3);  }
.cart-method-btn[title*="Card"]:not(:disabled)   { border-color: rgba(37,99,235,0.3);  }
.cart-method-btn[title*="Wallet"]:not(:disabled) { border-color: rgba(240,165,0,0.3);  }

/* Fast-cash + split */
.cart-pay-fast-btn {
  width: 100%;
  height: 52px;
  margin: 0 var(--space-4) var(--space-2);
  border-radius: var(--radius-md);
  border: none;
  background: var(--accent);
  color: var(--accent-t);
  font-size: 1rem;
  font-weight: 800;
  font-family: var(--font);
  cursor: pointer;
  font-variant-numeric: tabular-nums;
  transition: background 0.12s, transform 0.08s;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
}
.cart-pay-fast-btn:hover:not(:disabled) { background: var(--accent-h); }
.cart-pay-fast-btn:active:not(:disabled) { transform: scale(0.98); }
.cart-pay-fast-btn:disabled { opacity: 0.45; cursor: not-allowed; background: var(--surface3); color: var(--text-muted); }

.cart-pay-split-btn {
  width: 100%;
  height: 40px;
  margin: 0 var(--space-4) var(--space-2);
  border-radius: var(--radius-md);
  border: 1px solid var(--border);
  background: transparent;
  color: var(--text-dim);
  font-size: 0.82rem;
  font-weight: 600;
  cursor: pointer;
  transition: background 0.1s;
}
.cart-pay-split-btn:hover:not(:disabled) { background: var(--surface2); }
.cart-pay-split-btn:disabled { opacity: 0.45; cursor: not-allowed; }
```

- [ ] **Step 2: Commit**

```
git add src/App.css
git commit -m "style(payment): 52px buttons, clear empty-cart disabled state, colour-coded methods"
```

---

## Task 8 — Action Bar + Shortcut Badges: three groups, 48px height, standardised badges

**Files:**
- Modify: `src/App.css` — action bar and kbd rules
- Modify: `src/pages/PosPage.tsx` — disable Clear when cart is empty

- [ ] **Step 1: Ensure Clear is disabled when `lineCount === 0` in `PosPage.tsx`**

In `PosPage.tsx`, find the Clear button (around line 633):
```jsx
          <button
            className="action-btn action-btn-danger"
            onClick={handleClearCartRequest}
            disabled={lineCount === 0}
```
This `disabled={lineCount === 0}` should already be present. If it is, no change needed. `handleClearCartRequest` already guards with `if (lineCount === 0) return;` but the `disabled` prop ensures the button also looks disabled visually.

- [ ] **Step 2: Replace action bar CSS in `App.css`**

Find and replace the `/* ── Action bar ── */` / `.action-bar` section:

```css
/* ── Bottom action bar ── */
.action-bar {
  display: flex;
  align-items: center;
  gap: var(--space-6);
  padding: 0 var(--space-4);
  height: 64px;
  background: var(--surface);
  border-top: 1px solid var(--border);
  flex-shrink: 0;
}

.action-group {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

/* Dividers between groups are provided by gap + the parent's gap */
.action-group + .action-group {
  padding-left: var(--space-6);
  border-left: 1px solid var(--border);
}

.action-btn {
  height: 44px;
  padding: 0 14px;
  border-radius: var(--radius-md);
  border: 1px solid var(--border);
  background: var(--surface2);
  color: var(--text-dim);
  font-size: 0.8rem;
  font-weight: 700;
  font-family: var(--font);
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  white-space: nowrap;
  transition: background 0.1s, color 0.1s, border-color 0.1s;
}
.action-btn:hover:not(:disabled) { background: var(--surface3); color: var(--text); }
.action-btn:active:not(:disabled) { transform: scale(0.97); }
.action-btn:disabled { opacity: 0.38; cursor: not-allowed; }

.action-btn-danger {
  background: var(--color-danger-soft);
  border-color: rgba(239,68,68,0.25);
  color: var(--error);
}
.action-btn-danger:hover:not(:disabled) { background: rgba(239,68,68,0.18); }

/* Lock indicator — only for permission-locked actions */
.action-lock {
  font-size: 0.72rem;
  opacity: 0.7;
}

/* Refund — distinct danger style */
.action-btn-refund {
  background: var(--color-danger-soft);
  border-color: rgba(239,68,68,0.3);
  color: var(--error);
}
.action-btn-refund:hover:not(:disabled) { background: rgba(239,68,68,0.18); }
```

- [ ] **Step 3: Replace global `kbd` rules in `App.css`**

Find and replace the existing `kbd { ... }` block:

```css
/* ── Keyboard shortcut badges — one consistent component ── */
kbd {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  height: 20px;
  min-width: 26px;
  padding: 0 5px;
  border-radius: 5px;
  font-family: var(--font-mono);
  font-size: 0.68rem;
  font-weight: 700;
  line-height: 1;
  color: var(--text-muted);
  background: var(--surface3);
  border: 1px solid var(--border);
  border-bottom-width: 2px;
  letter-spacing: 0.02em;
  white-space: nowrap;
  font-style: normal;
  vertical-align: middle;
  flex-shrink: 0;
}
kbd.kbd-primary {
  color: var(--accent);
  background: var(--accent2);
  border-color: rgba(240,165,0,0.3);
}
```

- [ ] **Step 4: Commit**

```
git add src/App.css src/pages/PosPage.tsx
git commit -m "style(action-bar): three groups, 44px buttons, standardised kbd badges"
```

---

## Task 9 — Left Sidebar + Status Bar

**Files:**
- Modify: `src/App.css` — sidebar selected state, status bar

- [ ] **Step 1: Update sidebar selected state in `App.css`**

Find `.pos-sidebar-item.active { ... }` and replace just the active rules:

```css
.pos-sidebar-item.active {
  background: rgba(240,165,0,0.06);
  color: var(--accent);
}
.pos-sidebar-item.active svg { color: var(--accent); }

/* Slim left rail — less visual weight than a full tile */
.pos-sidebar-item.active::before {
  content: '';
  position: absolute;
  left: 0;
  top: 8px;
  bottom: 8px;
  width: 3px;
  background: var(--accent);
  border-radius: 0 3px 3px 0;
}
```

- [ ] **Step 2: Replace status bar CSS in `App.css`**

Find and replace the `.pos-status-bar` section:

```css
/* ── Status bar (bottom strip) ── */
.pos-status-bar {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: 0 var(--space-4);
  height: 28px;
  background: var(--surface);
  border-top: 1px solid var(--border);
  font-size: 0.72rem;
  color: var(--text-muted);
  flex-shrink: 0;
  overflow: hidden;
}
.status-sep { color: var(--border2); }
.status-spacer { flex: 1; }
.status-shift-open { color: var(--success); font-weight: 600; }
.status-online  { color: var(--success); font-weight: 600; }
.status-offline { color: var(--warning); font-weight: 600; }
.status-pending { color: var(--text-muted); }
.status-last-sale { color: var(--text-dim); }
.status-shortcuts { display: flex; align-items: center; gap: 4px; flex-shrink: 0; }
.status-shortcuts kbd { height: 16px; min-width: 20px; padding: 0 3px; font-size: 0.6rem; }
```

- [ ] **Step 3: Commit**

```
git add src/App.css
git commit -m "style(sidebar+status): slim active rail, 28px status strip, compact shortcuts"
```

---

## Task 10 — Offline Banner, Light-Mode Overrides, Final Cleanup

**Files:**
- Modify: `src/App.css` — offline banner, light-mode overrides, misc cleanup
- Modify: `src/pages/PosPage.tsx` — pos-layout grid column width to 88px

- [ ] **Step 1: Set sidebar width to 88px in `PosPage.tsx`**

In `PosPage.tsx`, find:
```jsx
      <div className="pos-main" style={{ gridTemplateColumns: `88px minmax(0, 1fr) ${cartWidth}px` }}>
```
This is already `88px`. No change needed — just verify.

- [ ] **Step 2: Add offline banner CSS to `App.css`**

After the `.pos-status-bar` section, add:

```css
/* ── Offline mode banner ── */
.pos-offline-banner {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-4);
  background: var(--color-warning-soft);
  border-bottom: 1px solid rgba(251,191,36,0.3);
  font-size: 0.78rem;
  font-weight: 600;
  color: var(--warning);
  flex-shrink: 0;
}

/* ── Error banner ── */
.error-banner {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-4);
  background: var(--color-danger-soft);
  border-bottom: 1px solid rgba(239,68,68,0.25);
  font-size: 0.8rem;
  color: var(--error);
  cursor: pointer;
  flex-shrink: 0;
}
.error-banner-action {
  margin-left: auto;
  padding: 3px 10px;
  border-radius: var(--radius-sm);
  border: 1px solid rgba(239,68,68,0.3);
  background: transparent;
  color: var(--error);
  font-size: 0.76rem;
  font-weight: 600;
  cursor: pointer;
}
.error-dismiss {
  color: var(--error);
  cursor: pointer;
  font-size: 1rem;
  opacity: 0.7;
}
.error-dismiss:hover { opacity: 1; }
```

- [ ] **Step 3: Update light-mode overrides in `App.css`**

Find the existing `[data-theme="light"] ...` overrides block (around lines 113–151). Replace with:

```css
/* ── Light mode refinements ── */
[data-theme="light"] .barcode-input:focus {
  box-shadow: 0 0 0 3px rgba(37,99,235,0.14);
  border-color: var(--color-focus);
}
[data-theme="light"] .product-card {
  box-shadow: 0 1px 3px rgba(0,0,0,0.06);
}
[data-theme="light"] .product-card:hover {
  box-shadow: 0 4px 12px rgba(0,0,0,0.10);
}
[data-theme="light"] .cart-pay-fast-btn:disabled {
  background: var(--surface3);
  color: var(--text-muted);
  box-shadow: none;
}
[data-theme="light"] .cart-method-btn:disabled { opacity: 0.38; }
[data-theme="light"] .action-btn:disabled { opacity: 0.38; }
[data-theme="light"] .stock-badge-out {
  background: rgba(220,38,38,0.06);
  color: #B91C1C;
  border-color: rgba(220,38,38,0.18);
}
[data-theme="light"] .top-bar-pill-success {
  background: rgba(22,163,74,0.10);
  color: #15803D;
}
```

- [ ] **Step 4: Final TypeScript compile check**

```
npx tsc --noEmit 2>&1
```
Expected: zero errors.

- [ ] **Step 5: Commit**

```
git add src/App.css src/pages/PosPage.tsx
git commit -m "style(polish): offline banner, error banner, light-mode overrides, final cleanup"
```

---

## Verification Checklist

After all tasks complete, visually verify:

1. **Top bar** — three distinct groups, 56px tall, no clutter between clock and logo
2. **Scan input** — hint text sits below the input, not overlapping; blue focus ring; 52px height
3. **Category toolbar** — "Custom Item" is an orange button, categories are pill-shaped, no underlines
4. **Product cards** — dense vertical layout, 1.1rem price, stock badge, hover lifts, active scales
5. **Cart empty state** — centered icon + "Cart is empty" text, no blank scrollable void
6. **Totals** — discount row is neutral gray when `0.000`, gold when non-zero
7. **Grand total** — 1.8rem, gold, monospace
8. **Payment buttons** — 52px, visually disabled (opacity 0.45) when cart empty
9. **Action bar** — three groups separated by vertical lines, Clear disabled when cart empty
10. **Sidebar** — slim 3px left rail on active item, no heavy beige tile
11. **Status bar** — 28px, 0.72rem, not duplicating top-bar status
12. **Light mode** — all overrides apply correctly
