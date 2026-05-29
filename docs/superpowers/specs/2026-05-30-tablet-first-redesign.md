# ZANPOS Tablet-First Redesign Spec
**Date:** 2026-05-30  
**Target:** 15"+ fixed-mount POS touchscreen (landscape, Windows)  
**Aesthetic:** "Warm Command" — refined dark retail terminal

---

## 1. Problem Statement

ZANPOS was designed for mouse/keyboard and adapted to touch. The result:
- Touch targets too small (many elements < 40px — below 44px minimum)
- Base font 14px — hard to read from standing distance on 15" screen
- Information density optimised for desktop, overwhelming on touch
- Back Office list rows cramped — require precise tapping
- Payment flow buttons borderline for large fingers
- Login PIN pad small relative to screen real estate

---

## 2. Design Principles

1. **56px minimum for any interactive element** (44px absolute floor, 72px for primary CTAs, 80px for the Charge button)
2. **16px base font** — readable at arm's length from a 15" terminal
3. **Every screen has one dominant action** — visually unmistakable
4. **No information hidden behind hover** — everything readable without interaction
5. **Warm, not clinical** — surfaces shift from cool blue-grey to warm charcoal (amber-tinted)

---

## 3. Typography Change

| Role | Before | After |
|---|---|---|
| UI body | Inter | DM Sans |
| Display/headings | Syne | Bricolage Grotesque |
| Money/numbers | IBM Plex Mono | IBM Plex Mono (keep) |
| Base size | 14px | 16px |

Google Fonts import line becomes:
```
DM Sans (300;400;500;600;700;800), Bricolage Grotesque (400;500;600;700;800), IBM Plex Mono (400;500;600;700), JetBrains Mono (400;500) [boot screen only], Syncopate (700) [boot screen only]
```

CSS variables:
```css
--font:         'DM Sans', 'Segoe UI Variable Text', system-ui, sans-serif;
--font-display: 'Bricolage Grotesque', 'Syne', system-ui, sans-serif;
--font-mono:    'IBM Plex Mono', 'Cascadia Code', monospace;
font-size: 16px;  /* was 14px */
```

---

## 4. New Token Set

### 4.1 Touch Targets (NEW)
```css
--touch-sm: 44px;   /* minimum — tertiary/icon buttons */
--touch-md: 56px;   /* standard — inputs, most buttons, list rows */
--touch-lg: 72px;   /* prominent — cart rows, category pills, action bar */
--touch-xl: 80px;   /* hero — Charge button, primary payment CTA */
```

### 4.2 Button Heights (NEW)
```css
--btn-sm: 44px;
--btn-md: 56px;
--btn-lg: 64px;
--btn-xl: 80px;
```

### 4.3 Input Height (NEW)
```css
--input-height: 56px;
```

### 4.4 Spacing (additions)
```css
--space-10: 40px;
--space-12: 48px;
--space-16: 64px;
```

### 4.5 Radius (bumped)
```css
--radius-sm: 10px;   /* was 8px */
--radius:    14px;   /* was 12px */
--radius-md: 14px;   /* was 12px */
--radius-lg: 20px;   /* was 16px */
--radius-xl: 28px;   /* was 20px */
```

### 4.6 Color Evolution (default dark theme only)
The warm charcoal shift: surfaces lose cool blue-grey tint, gain warm amber tint.
```css
/* Default dark — warm charcoal */
--bg:       #0E0C0A;   /* was #0C0E13 — amber-warmed black */
--surface:  #1A1713;   /* was #181B27 — warm charcoal */
--surface2: #221F1A;   /* was #1F2335 */
--surface3: #2C2822;   /* was #262B42 */
--surface4: #36322A;   /* was #2E3452 */
```
All other themes keep their existing values — only the default dark gets the warm shift.

### 4.7 Accent Gradient (NEW — for primary CTAs)
```css
--accent-gradient: linear-gradient(135deg, #F5AD0A 0%, #D49200 100%);
--shadow-gold-lg: 0 8px 32px rgba(240,165,0,0.35);
```

---

## 5. Component-by-Component Changes

### 5.1 Top Bar
| Property | Before | After |
|---|---|---|
| Height | 62px | 72px |
| Logo font-size | ~16px | 20px |
| Time font-size | ~16px | 22px |
| Status pills height | 26px | 34px |
| Cashier chip height | ~28px | 38px |
| Padding | 0 20px | 0 24px |

### 5.2 POS Sidebar
| Property | Before | After |
|---|---|---|
| Width | 88px | 108px |
| Button width×height | 72×58px | 88×72px |
| Icon size | 20px (implied) | 26px |
| Label font | 10px | 12px |
| Active indicator | 3px | 4px |

`.pos-main` grid: `88px minmax(0,1fr) 380px` → `108px minmax(0,1fr) 400px`

### 5.3 Barcode Input
| Property | Before | After |
|---|---|---|
| Height | 52px | 64px |
| Font size | 1.05rem | 1.1rem |
| Icon size | implied ~20px | 24px |
| Dropdown item height | ~40px | 56px |
| Dropdown font | 0.92rem | 1rem |

### 5.4 Category Pills (toolbar)
Should be reviewed in context — typical pill: 
- Height: 44px (from implied ~36px)
- Font: 0.9rem weight 600 (from ~0.8rem)
- Padding: 0 20px (from ~0 14px)
- Gap between pills: 8px (from ~5px)

### 5.5 Product Cards
| Property | Before | After |
|---|---|---|
| min-height | 130px | 140px |
| Name font | 0.88rem / weight 700 | 0.95rem / weight 700 |
| Price font | 1.1rem | 1.25rem |
| SKU font | 0.68rem | 0.72rem |
| Padding | 12×12×8 | 14×14×10 |
| Grid min column | 178px | 190px |
| Stock badge height | implied ~22px | 28px |
| Stock badge font | 0.66rem | 0.72rem |

### 5.6 Cart Panel
| Property | Before | After |
|---|---|---|
| Width (in grid) | 380px | 400px |
| Header min-height | 56px | 68px |
| Header title | 1.1rem | 1.2rem |
| Cart row min-height | 80px | 92px |
| Qty buttons | 44×44px | 52×52px |
| Qty value font | 1.1rem | 1.2rem |
| Cart line name | 1rem | 1.05rem |
| Cart line total | 1rem | 1.1rem |
| Grand total | 1.8rem | 2.2rem |
| Del button | 44×44px | 52×52px |
| Lines padding | 8px 8px | 10px 10px |
| Lines gap | 6px | 8px |

### 5.7 Charge Button (np-fast-cash-btn)
| Property | Before | After |
|---|---|---|
| Height | 60px | 80px |
| Font size | 1.1rem | 1.25rem |
| Background | var(--accent) flat | var(--accent-gradient) |
| Shadow | none | var(--shadow-gold-lg) |
| Border-radius | var(--radius-md) | var(--radius-lg) |

### 5.8 Payment Method Buttons (np-method-btn)
| Property | Before | After |
|---|---|---|
| Height | 52px | 64px |
| Font | 0.92rem weight 700 | 1rem weight 700 |
| Border-radius | var(--radius-sm) | var(--radius-md) |

### 5.9 Dialpad Keys
| Property | Before | After |
|---|---|---|
| Height | 48px | 60px |
| Font | 1.2rem weight 700 | 1.35rem weight 700 |

### 5.10 Action Bar Buttons (pos-sidebar-item)
Already covered in 5.2. The action bar at the bottom of the sidebar:
- Any standalone action buttons in `.action-bar` or similar: height 60px minimum

### 5.11 Custom Item / Quick-Add Button (numpad-custom-btn)
| Property | Before | After |
|---|---|---|
| Height | 44px | 56px |
| Font | 0.88rem | 0.95rem |

### 5.12 Numpad Display
| Property | Before | After |
|---|---|---|
| min-height | 58px | 68px |
| Multiplier font | 2rem | 2.25rem |

### 5.13 Totals rows (np-total-row)
| Property | Before | After |
|---|---|---|
| Font | 0.92rem | 1rem |
| Padding per row | 3px 0 | 5px 0 |

---

## 6. Back Office Modal

### 6.1 List panel (bo-list-pane / bo-list-header)
| Property | Before | After |
|---|---|---|
| Panel width | 460px | 480px |
| List row min-height | implied ~52px | 72px |
| Search input height | ~40px | 56px |
| Search font | implied ~14px | 16px |
| Action button height (Import/Add) | ~40px | 52px |
| Button font | implied ~13px | 14px |
| Tab/section font | implied ~13px | 15px |

### 6.2 Detail/edit panel
| Property | Before | After |
|---|---|---|
| All text inputs | implied ~40px | 56px |
| Input font | implied ~14px | 16px |
| Select height | implied ~40px | 56px |
| Button height (Save/Cancel) | implied ~40px | 56px |
| Label font | implied ~13px | 15px |
| Section gap | implied ~16px | 24px |

### 6.3 Reports, Settings sub-tabs
| Property | Before | After |
|---|---|---|
| Sub-tab button height | implied ~40px | 52px |
| Toggle switch size | implied 22×44px | 28×52px |
| Settings row min-height | implied ~52px | 64px |

---

## 7. Modal Dialogs (general)

All modals get:
- `min-padding: 32px` (was implied ~24px)  
- Buttons: `min-height: 56px`
- Inputs: `height: 56px`
- Header font: `1.4rem weight 800` (was implied ~1.1rem)

### 7.1 Payment Modal (PaymentModal.tsx)
| Element | Before | After |
|---|---|---|
| Method buttons | implied ~56px | 80px tall |
| Amount input | implied ~48px | 72px |
| Numpad keys | ~48px | 60px |
| Quick denomination buttons | implied ~48px | 64px |
| Confirm button | implied ~52px | 80px |

### 7.2 Shift Modal
| Element | Before | After |
|---|---|---|
| Amount inputs | implied ~44px | 64px |
| Confirm button | implied ~48px | 72px |

### 7.3 Discount Modal  
| Element | Before | After |
|---|---|---|
| Amount input | implied ~44px | 64px |
| Percent/amount toggle buttons | implied ~44px | 56px |
| Reason input | implied ~44px | 56px |
| Apply button | implied ~48px | 72px |

### 7.4 Confirm Action Modal
| Element | Before | After |
|---|---|---|
| Confirm/Cancel buttons | implied ~44px | 64px |
| Button font | implied ~14px | 16px |

---

## 8. Login Screen

### 8.1 User Cards (staff selection)
| Property | Before | After |
|---|---|---|
| Card height | implied ~80px | 100px |
| Avatar circle | implied ~44px | 60px |
| Name font | implied ~16px | 18px weight 700 |
| Role badge height | implied ~24px | 30px |
| Card padding | implied ~12px | 16px |
| Grid gap | implied ~8px | 12px |

### 8.2 PIN Pad (on login screen)
| Property | Before | After |
|---|---|---|
| Digit buttons | implied ~72×72px | 88×88px |
| Digit font | implied ~24px | 28px weight 600 |
| Grid gap | implied ~8px | 12px |
| PIN dot size | implied ~12px | 18px |
| PIN dots gap | implied ~10px | 16px |
| Backspace/OK buttons | implied ~72×72px | 88×88px |

---

## 9. Lock Screen

Same PIN pad sizing as login screen (Section 8.2). User avatar: 64px. User name: 22px weight 700.

---

## 10. Setup Wizard

| Element | Before | After |
|---|---|---|
| Step circle | implied ~32px | 44px |
| Step label | implied ~12px | 14px |
| All inputs | implied ~44px | 56px |
| Next/Back buttons | implied ~48px | 64px |
| Primary CTA background | flat accent | var(--accent-gradient) |

---

## 11. Files to Modify

### Primary
| File | Change scope |
|---|---|
| `src/App.css` | All token changes + all component CSS changes (90% of work) |

### Secondary (structural JSX changes needed)
| File | Change |
|---|---|
| `src/pages/LoginScreen.tsx` | PIN pad button sizes (inline style or className only) |
| `src/components/LockScreen.tsx` | PIN pad sizing consistent with login |
| `src/components/Dialpad.tsx` | Dialpad key height via CSS class; verify no hardcoded px |
| `src/components/CartPanel.tsx` | Verify qty button sizing picks up new CSS; no inline px overrides |
| `src/components/PaymentModal.tsx` | Method button grid layout + amount input sizing |

---

## 12. What Does NOT Change

- The multi-theme system (all 7 themes preserved; only default dark gets warm surface shift)
- The overall 3-column POS layout (sidebar | product area | cart+numpad)
- All Rust backend, Tauri commands, DB schema — zero backend changes
- All existing CSS class names — new values only, no renames
- The keyboard shortcut system (still works for power users with keyboards)
- All component logic and state — pure visual changes

---

## 13. Success Criteria

After implementation:
1. Every interactive element has a touch target ≥ 44px (verify by inspection)
2. Charge button is visually dominant and reaches 80px height
3. Cart rows are comfortably readable at 16px without squinting
4. Back Office list rows 72px — tappable without zooming in
5. PIN pad digits 88px — zero mis-taps for normal adult fingers
6. DM Sans renders at 16px base, Bricolage Grotesque on display headings
7. Default dark theme has warm charcoal (not blue-grey) surfaces
8. All existing themes still render correctly
9. No regressions in functionality (all modals open, all forms submit)
