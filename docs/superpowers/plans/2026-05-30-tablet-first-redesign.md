# Tablet-First Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade ZANPOS for 15"+ fixed-mount POS touchscreens — 16px base font, DM Sans typography, warm charcoal default dark theme, and ≥56px touch targets throughout.

**Architecture:** This is a pure CSS/JSX visual change — no Rust, no DB, no business logic. The primary change surface is `src/App.css` (12 k lines). A block titled "TOUCH / TABLET FIRST" already exists starting at line ~7225 and will be heavily updated. Secondary JSX files need no new logic, only CSS class-name or inline-style corrections.

**Tech Stack:** React/TypeScript, Tailwind-free, plain CSS custom properties (App.css), Google Fonts

**Spec:** `docs/superpowers/specs/2026-05-30-tablet-first-redesign.md`

---

## File Map

| File | What changes |
|---|---|
| `src/App.css` | All CSS — 90 % of work. Two change zones: `:root` token block (~lines 1–110) and the "TOUCH / TABLET FIRST" block (~lines 7225–7445) |
| `src/pages/LoginScreen.tsx` | Verify no inline `px` overrides on PIN pad or user card elements |
| `src/components/LockScreen.tsx` | Verify no inline `px` overrides on PIN pad or lock user avatar |
| `src/components/Dialpad.tsx` | Verify dialpad keys pick up new CSS — no hardcoded heights |
| `src/components/CartPanel.tsx` | Verify qty buttons pick up new CSS — no hardcoded heights |
| `src/components/PaymentModal.tsx` | Verify method buttons / amount input have no hardcoded heights |

---

## Task 1: Foundation — Font Import, CSS Variables, Warm Dark Theme

**Files:**
- Modify: `src/App.css` (lines 1–110, `:root` block)

### What to change

**Line 5 — Google Fonts `@import`**

Replace:
```css
@import url('https://fonts.googleapis.com/css2?family=Inter:wght@300;400;500;600;700&family=Syne:wght@700;800&family=Syncopate:wght@700&family=JetBrains+Mono:wght@400;500&family=IBM+Plex+Mono:wght@400;500;600;700&display=swap');
```
With:
```css
@import url('https://fonts.googleapis.com/css2?family=DM+Sans:ital,opsz,wght@0,9..40,300;0,9..40,400;0,9..40,500;0,9..40,600;0,9..40,700;0,9..40,800;1,9..40,400&family=Bricolage+Grotesque:opsz,wght@12..96,400;12..96,500;12..96,600;12..96,700;12..96,800&family=IBM+Plex+Mono:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500&family=Syncopate:wght@700&display=swap');
```

**Lines 12–16 — default dark surfaces (inside `:root`)**

Replace the five `--bg` / `--surface*` variables:
```css
  --bg:       #0E0C0A;   /* was #0C0E13 — amber-warmed black */
  --surface:  #1A1713;   /* was #181B27 — warm charcoal */
  --surface2: #221F1A;   /* was #1F2335 */
  --surface3: #2C2822;   /* was #262B42 */
  --surface4: #36322A;   /* was #2E3452 */
```

**Lines 53–55 — typography variables**

Replace:
```css
  --font:         'Inter', 'Segoe UI Variable Text', 'Segoe UI Variable', 'Segoe UI', system-ui, -apple-system, sans-serif;
  --font-display: 'Syne', 'Segoe UI Variable Display', 'Segoe UI Variable', system-ui, sans-serif;
  --font-mono:    'IBM Plex Mono', 'Cascadia Code', 'Consolas', 'Courier New', monospace;
```
With:
```css
  --font:         'DM Sans', 'Segoe UI Variable Text', system-ui, sans-serif;
  --font-display: 'Bricolage Grotesque', 'Syne', system-ui, sans-serif;
  --font-mono:    'IBM Plex Mono', 'Cascadia Code', monospace;
```

**Line 61 — add shadow-gold-lg after `--shadow-gold`**

After the existing `--shadow-gold` line, insert:
```css
  --shadow-gold-lg: 0 8px 32px rgba(240,165,0,0.35);
```

**Lines 73–80 — spacing (add three tokens after existing `--space-8`)**

After `--space-8: 32px;`, insert:
```css
  --space-10: 40px;
  --space-12: 48px;
  --space-16: 64px;
```

**Lines 45–50 — radius tokens (bump)**

Replace:
```css
  --radius:    12px;
  --radius-sm: 8px;
  --radius-md: 12px;
  --radius-lg: 16px;
  --radius-xl: 20px;
  --radius-pill: 999px;
```
With:
```css
  --radius:    14px;
  --radius-sm: 10px;
  --radius-md: 14px;
  --radius-lg: 20px;
  --radius-xl: 28px;
  --radius-pill: 999px;
```

**Line 109 — base font size**

Replace `font-size: 14px;` with `font-size: 16px;`

**After the z-index block (after line 107) — add new touch/button/input tokens**

After the `--z-top: 9999;` line and before `font-size: 14px;`, insert:
```css
  /* ── Touch targets ── */
  --touch-sm: 44px;
  --touch-md: 56px;
  --touch-lg: 72px;
  --touch-xl: 80px;

  /* ── Button heights ── */
  --btn-sm:   44px;
  --btn-md:   56px;
  --btn-lg:   64px;
  --btn-xl:   80px;

  /* ── Input height ── */
  --input-height: 56px;

  /* ── Accent gradient (Charge button, primary CTAs) ── */
  --accent-gradient: linear-gradient(135deg, #F5AD0A 0%, #D49200 100%);
```

- [ ] **Step 1: Apply all of the above changes to `src/App.css`**

  Edit the file making all changes described in this task. Each change is independent — apply them all in sequence. Exact `old_string` → `new_string` for each:

  1. The `@import` line (line 5)
  2. The five dark surface variables (`--bg` through `--surface4`) inside `:root`
  3. The three `--font*` variables
  4. Add `--shadow-gold-lg` after `--shadow-gold`
  5. Add `--space-10/12/16` after `--space-8`
  6. All six radius variables
  7. Insert new touch/button/input/accent-gradient tokens before `font-size: 14px`
  8. Change `font-size: 14px` to `font-size: 16px`

- [ ] **Step 2: Verify the file compiles (no CSS syntax errors)**

  Run: `cd C:\Users\super\ZAN\zanpos && npx vite build --mode development 2>&1 | head -40`
  Expected: build succeeds or only non-CSS warnings

- [ ] **Step 3: Commit**

  ```bash
  git add src/App.css
  git commit -m "feat(ui): DM Sans typography, warm dark theme, touch token foundation"
  ```

---

## Task 2: Top Bar + POS Main Grid

**Files:**
- Modify: `src/App.css` — `.top-bar` block (~line 391) and `.pos-main` (~line 483)

### What to change

**`.top-bar` — height 62→72px, padding `var(--space-5)` → `var(--space-6)`**
```css
/* old */
  height: 62px;
  padding: 0 var(--space-5);
/* new */
  height: 72px;
  padding: 0 var(--space-6);
```

**`.top-bar-logo` — font-size 1rem → 1.25rem (20px)**
```css
/* old */
  font-size: 1rem;
/* new */
  font-size: 1.25rem;
```

**`.top-bar-time` — font-size 1rem → 1.375rem (22px)**
```css
/* old */
  font-size: 1rem;
/* new */
  font-size: 1.375rem;
```

**`.top-bar-pill` — height 26px → 34px**
```css
/* old */
  height: 26px;
/* new */
  height: 34px;
```

**`.top-bar-cashier` — enlarge to 38px via padding (currently `padding: 3px 9px`)**
```css
/* old */
  padding: 3px 9px;
/* new */
  padding: 8px 12px;
```

**`.pos-main` — grid columns: sidebar 88→108px, cart 380→400px**
```css
/* old */
  grid-template-columns: 88px minmax(0, 1fr) 380px;
/* new */
  grid-template-columns: 108px minmax(0, 1fr) 400px;
```

- [ ] **Step 1: Apply all six changes above to `src/App.css`**

- [ ] **Step 2: Verify**

  Run: `npx vite build --mode development 2>&1 | head -20`
  Expected: zero errors

- [ ] **Step 3: Commit**

  ```bash
  git add src/App.css
  git commit -m "feat(ui): top bar 72px, pos-main grid 108/400px columns"
  ```

---

## Task 3: POS Sidebar

**Files:**
- Modify: `src/App.css` — `.pos-sidebar-item` block (~line 506) and its `::before` indicator (~line 546)
- Also update the tablet override for `.pos-sidebar-item` at ~line 7413

### What to change

**`.pos-sidebar-item` (base definition, ~line 506)**
```css
/* old */
  width: 72px;
  min-height: 58px;
  font-size: 0.62rem;
/* new */
  width: 88px;
  min-height: 72px;
  font-size: 0.75rem;
```

**`.pos-sidebar-item svg` — icon size 20→26px (add explicit size)**

After the existing `.pos-sidebar-item svg` block, add a `width`/`height`:
```css
/* old */
.pos-sidebar-item svg {
  flex-shrink: 0;
  stroke-width: 1.75;
}
/* new */
.pos-sidebar-item svg {
  flex-shrink: 0;
  stroke-width: 1.75;
  width: 26px;
  height: 26px;
}
```

**`.pos-sidebar-item.active::before` — active indicator width 3→4px**
```css
/* old */
  width: 3px;
/* new */
  width: 4px;
```

**Tablet override block (~line 7413) — `.pos-sidebar-item` override**
```css
/* old */
.pos-sidebar-item { padding: 12px 6px; width: 68px; }
/* new */
.pos-sidebar-item { padding: 14px 6px; width: 88px; min-height: 72px; }
```

- [ ] **Step 1: Apply all four changes above**

- [ ] **Step 2: Verify**

  Run: `npx vite build --mode development 2>&1 | head -20`

- [ ] **Step 3: Commit**

  ```bash
  git add src/App.css
  git commit -m "feat(ui): sidebar 88×72px items, 26px icons, 4px active rail"
  ```

---

## Task 4: Barcode Input + Category Tabs

**Files:**
- Modify: `src/App.css` — tablet override block for `.barcode-input` (~line 7282) and `.cat-tab` (~line 7273)

### What to change

**Tablet override `.barcode-input` (~line 7282)**
```css
/* old */
.barcode-input {
  padding: 14px 52px 14px 40px;
  font-size: 1rem;
}
/* new */
.barcode-input {
  height: 64px;
  padding: 0 52px 0 40px;
  font-size: 1.1rem;
}
```
_(Using `height: 64px` with `padding: 0` for the vertical keeps the horizontal padding for icon clearance.)_

**Barcode input icon** — find the CSS that sizes the scan icon inside `.barcode-input` (likely `.barcode-icon` or `.barcode-input-icon`) and set it to 24px. If it's sized by `font-size` on the icon wrapper, add:
```css
.barcode-icon, .barcode-input-icon { font-size: 24px; width: 24px; height: 24px; }
```
_(Search for the actual class name first — grep for `barcode-icon` in App.css and update whatever class controls the icon size to 24px.)_

**Barcode dropdown items** — find `.barcode-dropdown-item` (or similar) and set `min-height: 56px; font-size: 1rem`. If the class does not exist in a tablet override yet, add it to the tablet override block.

**Tablet override `.cat-tab` (~line 7273)**
```css
/* old */
.cat-tab {
  padding: 10px 16px;
  min-height: 44px;
  display: inline-flex;
  align-items: center;
  font-size: 0.8rem;
}
/* new */
.cat-tab {
  padding: 0 20px;
  min-height: 44px;
  height: 44px;
  display: inline-flex;
  align-items: center;
  font-size: 0.9rem;
  font-weight: 600;
}
```

**Category toolbar gap** — find `.cat-tabs` or `.category-toolbar` container in App.css and change gap from `~5px` to `8px`. (Grep for `cat-tabs` or `category-toolbar` to find the class.)

- [ ] **Step 1: Grep for the barcode icon class and dropdown item class names**

  Run: `grep -n "barcode-icon\|barcode-input-icon\|barcode-dropdown" C:/Users/super/ZAN/zanpos/src/App.css | head -20`

  Then apply the correct class names in the edits.

- [ ] **Step 2: Apply all category and barcode changes**

- [ ] **Step 3: Grep for category toolbar container**

  Run: `grep -n "cat-tabs\|category-toolbar\|cat-toolbar" C:/Users/super/ZAN/zanpos/src/App.css | head -10`

  Update its `gap` to 8px.

- [ ] **Step 4: Verify**

  Run: `npx vite build --mode development 2>&1 | head -20`

- [ ] **Step 5: Commit**

  ```bash
  git add src/App.css
  git commit -m "feat(ui): barcode input 64px, category tabs 44px/0.9rem/20px pad"
  ```

---

## Task 5: Product Grid + Cards

**Files:**
- Modify: `src/App.css` — tablet override block for product grid/card (~lines 7244–7270)

### What to change

**`.product-grid` — min column 178→190px (tablet override ~line 7246)**
```css
/* old */
.product-grid {
  grid-template-columns: repeat(auto-fill, minmax(178px, 1fr));
  gap: 8px;
}
/* new */
.product-grid {
  grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
  gap: 8px;
}
```

**`.product-card` — min-height 100→140px, max-height update (tablet override ~line 7251)**
```css
/* old */
.product-card {
  min-height: 100px;
  max-height: 116px;
  cursor: pointer;
}
/* new */
.product-card {
  min-height: 140px;
  cursor: pointer;
}
```
_(Remove max-height constraint — it was capping the card, conflicting with the taller spec.)_

**`.product-card-name` — font size 0.92→0.95rem (tablet override ~line 7260)**
```css
/* old */
.product-card-name  { font-size: 0.92rem; }
/* new */
.product-card-name  { font-size: 0.95rem; }
```

**`.product-card-price` — font size 0.96→1.25rem (tablet override ~line 7261)**
```css
/* old */
.product-card-price { font-size: 0.96rem; }
/* new */
.product-card-price { font-size: 1.25rem; }
```

**Stock badge in tablet override** — add to the tablet override block:
```css
/* add after product-card-price line in tablet override */
.stock-badge { height: 28px; font-size: 0.72rem; padding: 4px 7px; }
```

**Product card padding** — find `.product-card` base definition and update padding from `12px 12px 8px` to `14px 14px 10px` (grep for the base `.product-card` definition, usually around line 700–900):
```css
/* grep for: padding: 12px 12px 8px in .product-card context */
/* change to: */
padding: 14px 14px 10px;
```

**Product SKU** — find `.product-card-sku` (base definition) and change `font-size: 0.68rem` → `0.72rem`.

- [ ] **Step 1: Apply all product grid/card changes**

  Do the tablet override changes first (lines 7244–7270), then grep for the base product-card definition to fix padding and SKU font size.

  ```
  grep -n "product-card-sku\|padding: 12px 12px 8px" C:/Users/super/ZAN/zanpos/src/App.css | head -10
  ```

- [ ] **Step 2: Verify**

  Run: `npx vite build --mode development 2>&1 | head -20`

- [ ] **Step 3: Commit**

  ```bash
  git add src/App.css
  git commit -m "feat(ui): product cards 140px min, 1.25rem price, 190px grid column"
  ```

---

## Task 6: Cart Panel

**Files:**
- Modify: `src/App.css` — cart-related CSS

All exact current values must be confirmed by grepping before editing since they span multiple blocks. The values below are from the audit.

### What to change

Grep first to confirm locations:
```
grep -n "cart-line-wrap\|cart-qty-btn\|cart-grand-total\|cart-header\|cart-lines\|cart-panel-header" C:/Users/super/ZAN/zanpos/src/App.css | head -30
```

Then apply:

**Cart header min-height and title size**
```css
/* find .cart-panel-header or similar container — set: */
min-height: 68px;   /* was 56px */

/* find cart panel title font-size (1.1rem) — set: */
font-size: 1.2rem;
```

**`.cart-line-wrap` — min-height 80→92px**
```css
/* old */ min-height: 80px;
/* new */ min-height: 92px;
```

**Cart lines padding and gap** — find the container that wraps `.cart-line-wrap` items and update:
```css
/* old */ padding: 8px 8px;  gap: 6px;
/* new */ padding: 10px 10px; gap: 8px;
```

**`.cart-qty-btn` — 44×44→52×52px (tablet override ~line 7289)**
```css
/* old */
.cart-qty-btn {
  width: 36px;
  height: 32px;
  font-size: 1rem;
}
/* new */
.cart-qty-btn {
  width: 52px;
  height: 52px;
  font-size: 1.2rem;
}
```

**Cart delete button** — find `.cart-line-del` or similar:
```css
/* set width and height to 52px */
width: 52px;
height: 52px;
```

**Cart line name and total fonts**
```css
/* find .cart-line-name — set font-size: 1.05rem (was 1rem) */
/* find .cart-line-total — set font-size: 1.1rem (was 1rem) */
```

**`.cart-grand-total` — 1.8→2.2rem**
```css
/* find the grand total number font-size: 1.8rem */
/* change to: font-size: 2.2rem; */
```

**Cart net total line** — update tablet override (~line 7296):
```css
/* old */
.cart-net-total span:last-child { font-size: 1.5rem; }
/* new */
.cart-net-total span:last-child { font-size: 2.2rem; }
```

- [ ] **Step 1: Run the grep to find exact line numbers, then apply all cart changes**

- [ ] **Step 2: Verify**

  Run: `npx vite build --mode development 2>&1 | head -20`

- [ ] **Step 3: Commit**

  ```bash
  git add src/App.css
  git commit -m "feat(ui): cart rows 92px, qty buttons 52×52, grand total 2.2rem"
  ```

---

## Task 7: Charge Button + Payment Methods + Dialpad

**Files:**
- Modify: `src/App.css` — `.np-fast-cash-btn` (~line 1312), `.np-method-btn` (~line 1356), `.numpad-panel .dialpad-key` (~line 1375), `.numpad-display` (~line 1221), `.numpad-custom-btn` (~line 1255), `.np-total-row` (~line 1285)

### What to change

**`.np-fast-cash-btn` — Charge button hero treatment**
```css
/* old */
  height: 60px;
  background: var(--accent);
  border-radius: var(--radius-md);
  font-size: 1.1rem;
/* new */
  height: var(--btn-xl);   /* 80px */
  background: var(--accent-gradient);
  border-radius: var(--radius-lg);
  font-size: 1.25rem;
  box-shadow: var(--shadow-gold-lg);
```

Also remove `opacity: 0.88` from `:hover` (gradient buttons shouldn't fade — instead adjust brightness):
```css
/* replace */
.np-fast-cash-btn:hover:not(:disabled) { opacity: 0.88; transform: translateY(-1px); }
/* with */
.np-fast-cash-btn:hover:not(:disabled) { filter: brightness(1.08); transform: translateY(-1px); }
```

**`.np-method-btn` — payment method buttons**
```css
/* old */
  height: 52px;
  font-size: 0.92rem;
  border-radius: var(--radius-sm);
/* new */
  height: var(--btn-lg);   /* 64px */
  font-size: 1rem;
  border-radius: var(--radius-md);
```

**`.numpad-panel .dialpad-key` — dialpad keys**
```css
/* old */
.numpad-panel .dialpad-key {
  height: 48px;
  font-size: 1.2rem;
  font-weight: 700;
  border-radius: var(--radius-sm);
}
/* new */
.numpad-panel .dialpad-key {
  height: 60px;
  font-size: 1.35rem;
  font-weight: 700;
  border-radius: var(--radius-sm);
}
```

**`.numpad-display` — min-height**
```css
/* old */ min-height: 58px;
/* new */ min-height: 68px;
```

**`.numpad-multiplier` — font size**
```css
/* old */ font-size: 2rem;
/* new */ font-size: 2.25rem;
```

**`.numpad-custom-btn` — height**
```css
/* old */
  height: 44px;
  font-size: 0.88rem;
/* new */
  height: var(--btn-md);   /* 56px */
  font-size: 0.95rem;
```

**`.np-total-row` — font and padding**
```css
/* old */
  font-size: 0.92rem;
  padding: 3px 0;
/* new */
  font-size: 1rem;
  padding: 5px 0;
```

- [ ] **Step 1: Apply all seven changes above**

- [ ] **Step 2: Verify**

  Run: `npx vite build --mode development 2>&1 | head -20`

- [ ] **Step 3: Commit**

  ```bash
  git add src/App.css
  git commit -m "feat(ui): charge button 80px gradient, method buttons 64px, dialpad 60px"
  ```

---

## Task 8: Action Bar + Pay Button + Generic Buttons

**Files:**
- Modify: `src/App.css` — `.action-btn`, `.pay-button`, `.btn-primary/secondary/danger` tablet override (~line 7314)

### What to change

**`.pay-button` tablet override (~line 7309)**
```css
/* old */
.pay-button {
  height: 64px;
  font-size: 1.05rem;
}
/* new */
.pay-button {
  height: var(--btn-xl);   /* 80px */
  font-size: 1.25rem;
  background: var(--accent-gradient);
  box-shadow: var(--shadow-gold-lg);
  border-radius: var(--radius-lg);
}
```

**`.btn-primary`, `.btn-secondary`, `.btn-danger` tablet override (~line 7314)**
```css
/* old */
.btn-primary,
.btn-secondary,
.btn-danger {
  min-height: 48px;
  padding: 12px 24px;
  font-size: 0.9rem;
}
/* new */
.btn-primary,
.btn-secondary,
.btn-danger {
  min-height: var(--btn-md);   /* 56px */
  padding: 14px 28px;
  font-size: 1rem;
}
```

**`.action-btn`** — grep for `.action-btn` to find the base and tablet definitions:
```
grep -n "\.action-btn" C:/Users/super/ZAN/zanpos/src/App.css | head -10
```
Update (or add to tablet override block) to set `min-height: 60px`.

**Pay method fast-buttons** — find `.pay-method-btn` and `.pay-fast-btn` in the tablet override (~line 7299):
```css
/* old */
.pay-method-btn {
  min-height: 52px;
  padding: 10px 4px;
  font-size: 0.8rem;
}
.pay-fast-btn {
  min-height: 52px;
  padding: 14px 16px;
  font-size: 0.95rem;
}
/* new */
.pay-method-btn {
  min-height: 80px;
  padding: 12px 4px;
  font-size: 0.9rem;
}
.pay-fast-btn {
  min-height: 64px;
  padding: 16px 18px;
  font-size: 1rem;
}
```

- [ ] **Step 1: Apply all changes**

- [ ] **Step 2: Verify**

  Run: `npx vite build --mode development 2>&1 | head -20`

- [ ] **Step 3: Commit**

  ```bash
  git add src/App.css
  git commit -m "feat(ui): pay button 80px gradient, action-btn 60px, generic buttons 56px"
  ```

---

## Task 9: Modal Base Styles

**Files:**
- Modify: `src/App.css` — modal-related CSS (~lines 960–990, 2958–3005) and tablet override block

### What to change

Find the base `.modal` definition and its header/padding:
```
grep -n "\.modal {" C:/Users/super/ZAN/zanpos/src/App.css | head -5
```

**`.modal` base — increase padding**
Find the `padding` declaration inside `.modal {` and set minimum `32px` padding. The current value is typically ~`24px`. Change to `32px`.

**`.modal-header` — font size and weight** 
Find `.modal-header` or the `h2`/`h3` inside it. Add to the tablet override block:
```css
.modal-header h2, .modal-header h3, .modal h2:first-child {
  font-size: 1.4rem;
  font-weight: 800;
}
```

**`.modal-btn-primary`, `.modal-btn-secondary`, `.modal-btn-danger` tablet override (~line 7322)**
```css
/* old */
.modal-btn-primary,
.modal-btn-secondary,
.modal-btn-danger {
  min-height: 48px;
  padding: 12px 24px;
}
/* new */
.modal-btn-primary,
.modal-btn-secondary,
.modal-btn-danger {
  min-height: var(--btn-md);   /* 56px */
  padding: 14px 26px;
  font-size: 1rem;
}
```

**`.field-input`** — add to tablet override block:
```css
.field-input { min-height: var(--input-height); padding: 14px 14px; font-size: 1rem; }
```

**`.modal-btn-primary` / `.modal-btn-secondary` / `.modal-btn-danger`** in base definition (~lines 2958–3005) — add `min-height: 56px` and change `font-size: 0.85rem` → `0.95rem` on all three.

- [ ] **Step 1: Grep to find exact `.modal {` and `.modal-header` line numbers, then apply changes**

- [ ] **Step 2: Verify**

  Run: `npx vite build --mode development 2>&1 | head -20`

- [ ] **Step 3: Commit**

  ```bash
  git add src/App.css
  git commit -m "feat(ui): modal padding 32px, buttons 56px min, header 1.4rem/800"
  ```

---

## Task 10: Payment Modal + Shift/Discount/Confirm Modals

**Files:**
- Modify: `src/App.css` — payment modal and other modal-specific CSS

Find payment modal classes:
```
grep -n "\.pay-modal\|payment-modal\|quick-amt-btn\|dialpad-confirm\|discount-input\|discount-mode-tab" C:/Users/super/ZAN/zanpos/src/App.css | head -20
```

### What to change

**Quick denomination buttons tablet override (~line 7357)**
```css
/* old */
.quick-amt-btn {
  padding: 12px 18px;
  min-height: 48px;
  font-size: 0.88rem;
}
/* new */
.quick-amt-btn {
  padding: 16px 20px;
  min-height: 64px;
  font-size: 0.95rem;
}
```

**Discount modal inputs tablet override (~line 7369)**
```css
/* old */
.discount-input { font-size: 1.8rem; padding: 14px 6px; }
.discount-mode-tab { padding: 10px; min-height: 44px; font-size: 0.9rem; }
/* new */
.discount-input { font-size: 1.8rem; padding: 18px 6px; min-height: 64px; }
.discount-mode-tab { padding: 12px; min-height: 56px; font-size: 0.95rem; }
```

**Payment modal amount input** — grep for `pay-amount-input` or similar class:
```
grep -n "pay-amount\|amount-input\|payment-amount" C:/Users/super/ZAN/zanpos/src/App.css | head -10
```
If found, add to tablet override: `min-height: 72px; font-size: 1.4rem;`
If no CSS class exists (it may be using `.field-input` from task 9), confirm via grep.

**Dialpad confirm button** — find `.dialpad-confirm-btn` and add to tablet override:
```css
.dialpad-confirm-btn { min-height: 80px; font-size: 1.1rem; }
```

**Shift modal confirm button** — find `.shift-modal .modal-btn-primary` or the submit button and add to tablet override:
```css
/* In shift modal context, confirm button should be 72px */
.shift-modal .modal-btn-primary { min-height: 72px; }
.shift-modal .field-input { min-height: 64px; }
```

**Confirm action modal buttons** — add to tablet override:
```css
.confirm-action-modal .modal-btn-primary,
.confirm-action-modal .modal-btn-danger { min-height: 64px; font-size: 1rem; }
```

- [ ] **Step 1: Run all greps to find exact class names, then apply changes**

- [ ] **Step 2: Verify**

  Run: `npx vite build --mode development 2>&1 | head -20`

- [ ] **Step 3: Commit**

  ```bash
  git add src/App.css
  git commit -m "feat(ui): quick-amt 64px, discount tabs 56px, payment confirm 80px"
  ```

---

## Task 11: Back Office

**Files:**
- Modify: `src/App.css` — BO classes (~lines 4355–4680) and BO tablet overrides (~lines 7372–7393)

### What to change

**`.bo-list-pane` base (~line 4483) — width 460→480px**
```css
/* old */
  width: 460px;
/* new */
  width: 480px;
```

**`.bo-header` tablet override (~line 7373) — height 56→60px**
```css
/* old */
.bo-header { height: 56px; }
/* new */
.bo-header { height: 60px; }
```

**`.bo-tab` tablet override (~line 7374) — font size 0.85→0.94rem**
```css
/* old */
.bo-tab {
  padding: 0 18px;
  font-size: 0.85rem;
}
/* new */
.bo-tab {
  padding: 0 20px;
  font-size: 0.94rem;
}
```

**`.bo-list-row` tablet override (~line 7381) — min-height 52→72px**
```css
/* old */
.bo-list-row {
  padding: 12px 12px;
  min-height: 52px;
}
/* new */
.bo-list-row {
  padding: 16px 14px;
  min-height: 72px;
}
```

**`.bo-input` and `.bo-select` tablet override (~line 7387)**
```css
/* old */
.bo-input  { padding: 12px 14px; font-size: 0.95rem; min-height: 48px; }
.bo-select { padding: 12px 14px; font-size: 0.95rem; min-height: 48px; }
/* new */
.bo-input  { padding: 14px 16px; font-size: 1rem; min-height: 56px; }
.bo-select { padding: 14px 16px; font-size: 1rem; min-height: 56px; }
```

**`.bo-search` tablet override (~line 7389)**
```css
/* old */
.bo-search { padding: 10px 12px; font-size: 0.92rem; min-height: 44px; }
/* new */
.bo-search { padding: 14px 14px; font-size: 1rem; min-height: 56px; }
```

**`.bo-add-btn` tablet override (~line 7390)**
```css
/* old */
.bo-add-btn { min-height: 44px; padding: 10px 18px; }
/* new */
.bo-add-btn { min-height: 52px; padding: 12px 20px; font-size: 0.875rem; }
```

**`.bo-label` base (~line 4611) — font size**
```css
/* old */ font-size: 0.78rem;
/* new */ font-size: 0.875rem;  /* 14px at 16px base */
```

**`.bo-form-pane` form sections gap** — currently `margin: 12px 0 5px` on `.bo-label`. Add to tablet override:
```css
.bo-form-pane { gap: 24px; }
/* Increase the margin-top on bo-label */
.bo-label { margin-top: 20px; }
```

**`.biz-flag-row` settings row — min-height 64px**

The current `padding: 14px 16px` gives ~64px with normal content. To guarantee 64px on sparse content:
```css
/* add to tablet override */
.biz-flag-row { min-height: 64px; padding: 16px 18px; }
```

**`.biz-toggle` — 44×24 → 52×28px**
```css
/* old */
.biz-toggle {
  width: 44px;
  height: 24px;
}
.biz-toggle-track::after {
  width: 18px;
  height: 18px;
}
.biz-toggle input:checked + .biz-toggle-track::after {
  transform: translateX(22px);
}
```
Update in base definition (not tablet override — toggle is same everywhere):
```css
/* new */
.biz-toggle {
  width: 52px;
  height: 28px;
}
.biz-toggle-track::after {
  width: 22px;
  height: 22px;
}
.biz-toggle input:checked + .biz-toggle-track::after {
  transform: translateX(24px);
}
```

**`.settings-subnav-item` tablet override (~line 7396)**
```css
/* old */
.settings-subnav-item { padding: 0 16px; font-size: 0.88rem; height: 48px; }
/* new */
.settings-subnav-item { padding: 0 18px; font-size: 0.9rem; height: 52px; }
```

**BO form action buttons** — add to tablet override:
```css
.bo-form-actions .btn-primary,
.bo-form-actions .btn-secondary,
.bo-form-actions .btn-danger { min-height: 56px; }
```

- [ ] **Step 1: Apply all Back Office changes**

  Note: `.biz-toggle` changes are in the base definition (~line 11170), not the tablet override. All other BO changes are in the tablet override (~lines 7372–7393).

- [ ] **Step 2: Verify**

  Run: `npx vite build --mode development 2>&1 | head -20`

- [ ] **Step 3: Commit**

  ```bash
  git add src/App.css
  git commit -m "feat(ui): BO list rows 72px, inputs 56px, settings toggle 52×28px"
  ```

---

## Task 12: Login Screen + Lock Screen

**Files:**
- Modify: `src/App.css` — login/lock CSS (~lines 2593–2742) and tablet override (~lines 7340–7347, 7441–7442)
- Verify: `src/pages/LoginScreen.tsx` — no inline px overrides on `.pin-key`, `.pin-row`, `.user-card`, `.user-avatar`
- Verify: `src/components/LockScreen.tsx` — no inline px overrides on pin pad elements

### What to change in App.css

**`.user-card` tablet override (~line 7441)**
```css
/* old */
.user-card { padding: 28px 16px 24px; }
/* new */
.user-card { padding: 16px 16px 18px; min-height: 100px; }
```

**`.user-avatar` tablet override (~line 7442)**
```css
/* old */
.user-avatar { width: 64px; height: 64px; font-size: 1.7rem; }
/* new */
.user-avatar { width: 60px; height: 60px; font-size: 1.6rem; }
```
_(Login screen avatar spec is 60px; lock screen avatar spec is 64px — we set the base override to 60px and override in lock context if needed.)_

**`.user-name` base (~line 2632) — font size**
```css
/* old */ .user-name { font-weight: 700; font-size: 0.92rem; margin-bottom: 6px; }
/* new */ .user-name { font-weight: 700; font-size: 1.125rem; margin-bottom: 6px; }
```
_(1.125rem = 18px at 16px base)_

**`.user-grid` — gap from 12px to 12px** (already 12px — no change needed).

**`.pin-key` tablet override (~line 7340)**
```css
/* old */
.pin-key {
  padding: 22px 0;
  font-size: 1.6rem;
}
/* new */
.pin-key {
  padding: 0;
  height: 88px;
  font-size: 1.75rem;
  display: flex;
  align-items: center;
  justify-content: center;
}
```

**`.pin-key-clear` tablet override (~line 7344) — match height**
```css
/* old */
.pin-key-clear { font-size: 1.4rem; }
/* new */
.pin-key-clear { height: 88px; font-size: 1.6rem; }
```

**`.pin-pad` and `.pin-row` gap tablet override (~lines 7345–7346)**
```css
/* old */
.pin-pad { gap: 10px; }
.pin-row  { gap: 10px; }
/* new */
.pin-pad { gap: 12px; }
.pin-row  { gap: 12px; }
```

**`.pin-dot` base (~line 2684) — size 13→18px**
```css
/* old */
  width: 13px;
  height: 13px;
/* new */
  width: 18px;
  height: 18px;
```

**`.pin-display` base (~line 2682) — gap 12→16px**
```css
/* old */
.pin-display { display: flex; gap: 12px; justify-content: center; margin: 0 0 20px; }
/* new */
.pin-display { display: flex; gap: 16px; justify-content: center; margin: 0 0 20px; }
```

**Lock screen** — `.lock-title` base (~line 4901) — font size update:
```css
/* old */
.lock-title {
  font-size: 1.4rem;
  font-weight: 700;
/* new */
.lock-title {
  font-size: 1.375rem;   /* 22px at 16px base */
  font-weight: 700;
```

**Lock screen user avatar** — add to tablet override:
```css
.lock-panel .user-avatar { width: 64px; height: 64px; }
```

### Verify JSX files

- [ ] **Step 1: Check LoginScreen.tsx for inline height/width styles on pin-key, user-card**

  ```
  grep -n "style=.*height\|style=.*width" C:/Users/super/ZAN/zanpos/src/pages/LoginScreen.tsx | head -20
  ```
  Remove any inline `height`/`width` px values on those elements that would override CSS.

- [ ] **Step 2: Check LockScreen.tsx the same way**

  ```
  grep -n "style=.*height\|style=.*width" C:/Users/super/ZAN/zanpos/src/components/LockScreen.tsx | head -20
  ```

- [ ] **Step 3: Apply all App.css changes**

- [ ] **Step 4: Verify**

  Run: `npx vite build --mode development 2>&1 | head -20`

- [ ] **Step 5: Commit**

  ```bash
  git add src/App.css src/pages/LoginScreen.tsx src/components/LockScreen.tsx
  git commit -m "feat(ui): PIN keys 88×88px, dots 18px, login user cards 100px"
  ```

---

## Task 13: Setup Wizard + Final Cleanup

**Files:**
- Modify: `src/App.css` — setup wizard CSS (~lines 3473–3540) and tablet override block

### What to change

**`.setup-input` — add to tablet override block:**
```css
.setup-input { min-height: var(--input-height); padding: 14px 16px; font-size: 1rem; }
```

**`.setup-skip-btn` — add to tablet override block:**
```css
.setup-skip-btn { min-height: 64px; padding: 14px 24px; font-size: 0.95rem; }
```

**Setup wizard primary Next/submit buttons** — find the button class used in SetupWizard.tsx for the Next/Back buttons:
```
grep -n "btn-primary\|setup-next\|setup-btn\|setup-submit" C:/Users/super/ZAN/zanpos/src/components/SetupWizard.tsx | head -10
```
If they use `.btn-primary`, task 8 already covers them (min-height 56px). If a custom class, add it to the tablet override with `min-height: 64px`.

**Setup wizard primary CTA gradient** — for the final "Finish Setup" or "Complete" button that uses `.btn-primary`, add a scoped gradient rule in the tablet override:
```css
/* Make setup wizard primary CTA use the gradient */
.setup-card .btn-primary { background: var(--accent-gradient); }
```

**Step circles** — grep for step circle CSS:
```
grep -n "step-circle\|wizard-step\|mig-step-circle\|setup-step" C:/Users/super/ZAN/zanpos/src/App.css | head -10
```
If found, add to tablet override: `width: 44px; height: 44px; font-size: 1rem;`

**Comment header update** — update the comment on line 2 of App.css to reflect the new font:
```css
/* old */
/* Typeface: Inter (UI) · IBM Plex Mono (numerics) · Syne (logo only) */
/* new */
/* Typeface: DM Sans (UI) · IBM Plex Mono (numerics) · Bricolage Grotesque (display) */
```

### Verify remaining JSX files

- [ ] **Step 1: Check Dialpad.tsx for inline height overrides on key elements**

  ```
  grep -n "style=.*height\|style=.*width\|style=.*padding" C:/Users/super/ZAN/zanpos/src/components/Dialpad.tsx | head -20
  ```

- [ ] **Step 2: Check CartPanel.tsx for inline height/width overrides on qty buttons**

  ```
  grep -n "style=.*height\|style=.*width" C:/Users/super/ZAN/zanpos/src/components/CartPanel.tsx | head -20
  ```

- [ ] **Step 3: Check PaymentModal.tsx for inline height/width on method buttons or amount input**

  ```
  grep -n "style=.*height\|style=.*width" C:/Users/super/ZAN/zanpos/src/components/PaymentModal.tsx | head -20
  ```

- [ ] **Step 4: Apply setup wizard App.css changes and fix any inline style issues found**

- [ ] **Step 5: Build the full app**

  ```bash
  cd C:\Users\super\ZAN\zanpos
  npx vite build --mode development 2>&1 | tail -30
  ```
  Expected: zero errors; only optional warnings.

- [ ] **Step 6: Commit**

  ```bash
  git add src/App.css src/components/Dialpad.tsx src/components/CartPanel.tsx src/components/PaymentModal.tsx src/components/SetupWizard.tsx
  git commit -m "feat(ui): setup wizard 64px buttons, gradient CTAs, JSX inline style cleanup"
  ```

---

## Success Criteria (Spec Section 13)

After all tasks complete, verify by inspection:

1. Every interactive element has a touch target ≥ 44px
2. Charge button visually dominant at 80px with gold gradient shadow
3. Cart rows readable at 16px body font — no squinting
4. Back Office list rows 72px — tappable without zooming
5. PIN pad digits 88px — zero mis-taps
6. DM Sans renders at 16px base; Bricolage Grotesque on display headings
7. Default dark theme has warm charcoal (not blue-grey) surfaces
8. All 7 themes still render correctly (only default dark gets warm shift)
9. No regressions: all modals open, all forms submit, all keyboard shortcuts work

---

## Self-Review

**Spec coverage check:**
- Section 3 (Typography): ✅ Tasks 1
- Section 4 (Tokens — touch, button, input, spacing, radius, color, gradient): ✅ Task 1
- Section 5.1 (Top Bar): ✅ Task 2
- Section 5.2 (Sidebar): ✅ Tasks 2, 3
- Section 5.3 (Barcode Input): ✅ Task 4
- Section 5.4 (Category Pills): ✅ Task 4
- Section 5.5 (Product Cards): ✅ Task 5
- Section 5.6 (Cart Panel): ✅ Task 6
- Section 5.7 (Charge Button): ✅ Task 7
- Section 5.8 (Payment Methods): ✅ Task 7
- Section 5.9 (Dialpad Keys): ✅ Task 7
- Section 5.10 (Action Bar): ✅ Task 8
- Section 5.11 (Custom Item btn): ✅ Task 7
- Section 5.12 (Numpad Display): ✅ Task 7
- Section 5.13 (Totals rows): ✅ Task 7
- Section 6.1 (BO List Panel): ✅ Task 11
- Section 6.2 (BO Detail Panel): ✅ Task 11
- Section 6.3 (Settings): ✅ Task 11
- Section 7 (Modal Dialogs): ✅ Tasks 9, 10
- Section 8 (Login Screen): ✅ Task 12
- Section 9 (Lock Screen): ✅ Task 12
- Section 10 (Setup Wizard): ✅ Task 13
- Section 11 (JSX files): ✅ Tasks 12, 13
- Section 12 (What Does NOT Change): All non-CSS, non-JSX left untouched ✅
