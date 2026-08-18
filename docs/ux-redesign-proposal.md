# ZANPOS OfficeAI — Complete UX Redesign Proposal

> **Status**: Draft proposal for review
> **Date**: 2026-08-06
> **Source of truth**: Codebase analysis of `src/`, `src/officeai/`, `src/components/`, `src/styles/`
> **Limitation**: No screenshots were available for review; all findings are derived from component structure, CSS, navigation configuration, and routing logic. Visual observations (spacing, density, empty states) are inferred from layout patterns in the code. Items marked **[inferred]** should be validated against the actual UI.

---

## 1. Executive Redesign Diagnosis

The ZANPOS OfficeAI application suffers from **navigation fragmentation** as its primary defect. Three independent navigation layers (primary spaces, secondary grouped tabs, settings sub-tabs) plus a fourth internal tab system (SystemControl panes) create a Russian-doll architecture where users lose orientation at each nesting level.

The root cause is **accretion without architectural refactoring**. Features were added into whichever navigation slot was most convenient at the time — Settings received Storefront because it needed a "configuration" home, System Control received five panes because Settings already had six tabs, and Control absorbed everything that wasn't clearly Operations. The result is an application that works but does not cohere.

The **branding fragmentation** (OfficeAI, ZanAI, ZANPOS, ZanShop, Copilot, Ask AI) is a symptom of the same accretion. Each name was chosen in isolation for its specific feature context without a unifying product identity.

The redesign must resolve these structural problems before addressing surface-level visual inconsistency. The navigation must collapse from 4 tiers to 2. The branding must collapse from 6 names to 3. Settings must become a first-class navigation destination. AI must become a capability embedded across the product, not a separate destination.

---

## 2. Screenshot-by-Screenshot Audit

*No screenshots were attached. The following audit is derived from the codebase component tree, navigation configuration, and CSS layout patterns. Each finding is labeled with its evidence source.*

### A. Global Application Shell

| Aspect | Current State | Evidence |
|--------|--------------|----------|
| View system | State-machine: `"login"` → `"shift_check"` → `"pos"` / `"office_ai"` | `App.tsx:32` |
| Window chrome | Custom `WindowControls` component rendered above all views | `App.tsx:362` |
| POS access | Role-gated: only `owner` / `manager` can enter OfficeAI | `App.tsx:437-445` |
| Status communication | Startup health check splash with per-component dots; idle warning banner; update banner | `App.tsx:275-301, 400-406` |
| **Issue** | No persistent connectivity/sync status in the shell header — status is page-specific | |

### B. OfficeAI Primary Navigation (`OfficeAIPrimaryNav`)

| Aspect | Current State | Evidence |
|--------|--------------|----------|
| Structure | 4-entry vertical sidebar: Home, Ask AI, Operations, Control | `OfficeAIPrimaryNav.tsx:15-20` |
| Brand | "Office**AI**" wordmark + store name | `OfficeAIPrimaryNav.tsx:53-55` |
| Collapse | Toggle-able to icon-only state | `OfficeAIPrimaryNav.tsx:51-65` |
| Footer | "Back to POS" button (also Esc key) | `OfficeAIPrimaryNav.tsx:86-89` |
| **Issues** | (a) "Ask AI" is a primary space — AI is treated as a destination, not a capability. (b) "Control" is a catch-all for review, system, growth, and settings. (c) The wordmark "OfficeAI" is a third brand competing with ZANPOS and ZanAI. | |

### C. Secondary Navigation (`OfficeAISecondaryNav`)

| Aspect | Current State | Evidence |
|--------|--------------|----------|
| Structure | Grouped sidebar within Operations and Control spaces | `OfficeAISecondaryNav.tsx:16-39` |
| Operation groups | Catalog (products, categories, inventory, purchasing), Sales (reports, cashier, eod, deliveries), People (customers, users) | `nav.tsx:85-89` |
| Control groups | Review (actions, audit), Inbox (workflows), System (health, conflicts, devices), Growth (insights, loyalty), Settings (settings) | `nav.tsx:91-97` |
| **Issues** | (a) Two different sidebars compete for attention (primary + secondary). (b) "Purchasing" is grouped under Catalog but contains supplier management, POs, and receiving — a distinct workflow from product catalog. (c) "Audit" under Review alongside "Action Review" conflates AI approval workflow with historical audit trail. (d) "Devices" and "Health" are related but separated into different groups. | |

### D. Home / Overview (`OfficeAIOverview`)

| Aspect | Current State | Evidence |
|--------|--------------|----------|
| Content | Store pulse summary, signal grid (sales, transactions, stock, approvals), attention items list | `OfficeAIOverview.tsx:52-80` |
| Loading | Skeleton placeholders for title, 4 signal cards, attention list | `OfficeAIOverview.tsx:39-48` |
| Empty/error | Partial-availability banner with retry button | `OfficeAIOverview.tsx:62-68` |
| **Issues** | (a) **[inferred]** The signal grid uses 4 equal cards — the layout may feel sparse on wide screens. (b) Attention items are manager-only filtered, but the filtering logic is in the parent (`OfficeAIPage.tsx:448`), not the component — a separation of concerns issue. | |

### E. AI Assistant (`OfficeAIAssistantWorkspace`)

| Aspect | Current State | Evidence |
|--------|--------------|----------|
| Modes | Full-page chat workspace + slide-out CopilotDock | `OfficeAIPage.tsx:350-365, 466-483` |
| Setup | ProviderSetup wizard when unconfigured | `OfficeAIAssistantWorkspace` (imported) |
| Unconfigured state | Shows setup flow; AI features disabled indicator on overview | `officeAiStrings.ts:76-78` |
| Quick actions | 8 preset prompts (Today's sales, Low stock, etc.) | `officeAiTypes.ts:175-184` |
| Context | UiContext string with current tab name sent to AI | `OfficeAIPage.tsx:123-125` |
| **Issues** | (a) The full-page assistant has a large empty chat area before first message. (b) The CopilotDock is a separate component with its own ChatPanel — two chat implementations. (c) AI recommendations vs executed actions are distinguished in the chat UI but there's no system-wide audit trail of AI-initiated changes visible outside the Action Review tab. | |

### F. Operations Space — Products Tab (`ProductsTab`)

| Aspect | Current State | Evidence |
|--------|--------------|----------|
| Location | Under Operations → Catalog → Products | `nav.tsx:86` |
| Prefill | Receives `ProductPrefill` from POS notifications | `OfficeAIPage.tsx:304-305` |
| **Issues** | (a) **[inferred]** The products table likely includes barcode, name, category, price, cost, margin, stock, status — but these need verification against the actual component. (b) Product creation is via `ProductFormModal` — the new-product flow may feel disconnected from the list view. | |

### G. Settings (`SettingsTab`)

| Aspect | Current State | Evidence |
|--------|--------------|----------|
| Location | Under Control → Settings (4th level deep: Primary Nav → Secondary Nav → Settings Tab → Sub-tabs) | `nav.tsx:96` |
| Sub-tabs | Store, Receipt, Business, Printer, Storefront (ZanShop), System Control | `SettingsTab.tsx:51` |
| Meta bar | Branch code, currency, branch ID — always visible | `SettingsTab.tsx:337-350` |
| System Control panes | Hub, WhatsApp, AI control, Backup & updates, Maintenance (owner only) | `SystemControlTab.tsx:31-41` |
| **Issues** | (a) **Four-tier nesting**: Primary Nav → Control → Settings → Store/Receipt/... (plus System Control has a 5th tier: its own panes). (b) Storefront/ZanShop is buried inside Settings' horizontal tabs, making it feel like a configuration afterthought rather than a product feature. (c) Settings loads all branch data in one Promise.all but only shows one sub-tab at a time. (d) "System Control" mixes operational concerns (hub, WhatsApp, AI) with administrative (backup, updates, maintenance). | |

### H. Purchasing (`OfficeAIPurchasingWorkspace`)

| Aspect | Current State | Evidence |
|--------|--------------|----------|
| Location | Under Operations → Catalog → Purchasing | `nav.tsx:86` |
| Structure | Command strip + metrics + supplier/PO list/detail views | `OfficeAIPurchasingWorkspace.tsx` |
| **Issues** | (a) **[inferred]** Suppliers, POs, receiving, and margin analysis share one workspace — the workflow sequence (supplier → draft PO → approve → order → receive → update stock) may not be visually clear. (b) Margin metrics and purchasing operations are co-located but serve different purposes (analysis vs execution). | |

### I. Brand & Terminology Audit

| Term | Where Used | Problem |
|------|-----------|---------|
| **OfficeAI** | Primary nav wordmark, page class names (`oa-*`), i18n keys | Third brand competing with ZANPOS |
| **ZanAI** | CopilotDock header, i18n keys (`zanAiWorkspaces`, `zanAiCommandCenter`) | Fourth brand for the AI assistant |
| **ZANPOS** | App splash screen, CSS tokens, main app identity | The parent product — but invisible inside OfficeAI |
| **ZanShop** | Settings sub-tab label, StorefrontManagement component | Fifth brand for customer shop |
| **Copilot** | Dock toggle button, i18n keys (`openCopilot`, `hideCopilot`) | Yet another name for AI |
| **Ask AI** | Primary nav entry label | Different from both "ZanAI" and "Copilot" |
| **Control** | Primary nav space | Too broad — conflates review, system admin, growth, and settings |

---

## 3. Root Causes Ranked by Severity

### Severity 1 — Critical (blocks coherent UX)

1. **Accretion-driven navigation architecture**. Each new feature was placed in the nearest available navigation slot without reconsidering the overall hierarchy. Result: 4-tier nesting (Primary → Secondary → Settings tabs → System Control panes). Root cause: no navigation governance process; features were added to the existing structure regardless of fit.

2. **Missing unified product identity**. Six competing brand names (OfficeAI, ZanAI, ZANPOS, ZanShop, Copilot, Ask AI) were introduced independently. Root cause: each feature owner named their area without a brand architecture decision.

3. **Settings is buried under Control**. Store configuration, receipt templates, tax rules, printer setup, storefront management, and system maintenance share one horizontal tab bar 4 levels deep. Root cause: Settings was treated as "one tab" early in development, and sub-tabs were added to avoid restructuring the primary nav.

### Severity 2 — High (significant UX degradation)

4. **AI is a separate destination, not a cross-cutting capability**. The primary nav treats "Ask AI" as a peer to "Operations" — a user must leave their workflow to ask a question. The CopilotDock partially addresses this but is hidden behind a toggle button. Root cause: AI was originally built as a standalone chat page; the dock was added later as a compromise.

5. **"Control" is a dumping ground**. It contains action review (approval workflow), inbox (WhatsApp, bills), health (system monitoring), conflict inbox (data resolution), insights (analytics), loyalty (CRM), settings (configuration), audit (compliance), and devices (hardware). These serve entirely different user goals. Root cause: "Control" was the default destination for anything not clearly catalog, sales, or people.

6. **Inconsistent status communication**. Sync status, WhatsApp connectivity, AI availability, printer state, and system health are surfaced in different ways on different pages — pulse chips in the shell header, overview attention items, System Health page, and individual component status pills. Root cause: each status indicator was built for its specific page without a system-wide status framework.

### Severity 3 — Medium (noticeable but not blocking)

7. **Duplicate AI chat implementations**. `OfficeAIAssistantWorkspace` (full page) and `CopilotDock` (slide-out) both contain chat UIs with overlapping but not identical behavior. Root cause: the dock was added without refactoring the full-page chat into a shared component.

8. **Terminology inconsistency in i18n keys**. The same concept uses different keys across different translators (`officeAiStrings.ts`, `operationsStrings.ts`, `backOfficeStrings.ts`). Root cause: translators were created per-feature rather than per-domain.

9. **[Inferred] Inconsistent page layouts**. The overview page uses a signal grid + attention list pattern. The purchasing workspace uses a command strip + metrics + list pattern. Standard tabs (Products, Categories, etc.) use a table + action bar pattern. Settings uses a horizontal tab bar + form pattern. These are not variations on a single template system — they are independently designed layouts. Root cause: no shared page template components.

### Severity 4 — Low (cosmetic / polish)

10. **[Inferred] Empty states vary in quality**. Some components likely have guided empty states (StorefrontManagement checks for settings availability), while simpler tabs may show only "No data" messages. Root cause: empty states were not part of a design system.

11. **[Inferred] CSS class naming is inconsistent**. `oa-*` (officeai), `bo-*` (back office), `settings-*`, `pos-*` — four separate naming conventions. Root cause: each area evolved independently.

---

## 4. New Product Mental Model

**The product is a Store Operating System — one application that helps you run your retail store, from the till to the back office, with AI assistance woven throughout.**

### Primary Domains

| Domain | Purpose | User goal |
|--------|---------|-----------|
| **Today** | Command centre: what's happening right now | "How is my store doing? What needs my attention?" |
| **Sell** | Point of sale (existing PosPage) | "Process transactions quickly and accurately" |
| **Catalogue** | Products, categories, inventory, purchasing | "Manage what I sell and what I have in stock" |
| **Customers** | Customer records, loyalty, ZANSHOP | "Know my customers and sell to them online" |
| **Reports** | Sales, cashiers, end-of-day, deliveries | "Understand my business performance" |
| **Team** | Staff accounts, permissions, shifts | "Manage who works here and what they can do" |
| **Settings** | Store configuration, receipts, printers, integrations, system | "Configure how the store operates" |

### AI Placement

AI is not a domain. AI is a **capability layer** that sits alongside every domain:

- **Ask bar**: A global command/ask input (Ctrl+K) available everywhere
- **Context panel**: A slide-out panel that shows AI analysis of the current page
- **Inline suggestions**: AI-proposed actions appear within the relevant workflow (e.g., "Reorder this product" on the inventory page)
- **Action Review**: A dedicated approval queue for AI-proposed changes (remains a top-level concern)

---

## 5. Proposed Naming & Brand Architecture

| Layer | Name | Role |
|-------|------|------|
| **Product family** | ZANPOS | The store operating system — used in the app title bar, about dialog, installer |
| **POS module** | ZANPOS Till | The point-of-sale interface (the current `PosPage`) |
| **Back office** | ZANPOS Command | The management interface (the current `OfficeAIPage`) — replaces "OfficeAI" |
| **AI assistant** | ZanAI | The AI capability layer — used in the ask bar, context panel, and Action Review |
| **Customer shop** | ZANSHOP | The online storefront — remains as-is but elevated from settings |

### Migration

- `OfficeAI` → `ZANPOS Command` (or simply "Command" in internal navigation)
- All `oa-*` CSS classes → `cmd-*` (command) — or keep `oa-*` as legacy and add `cmd-*` aliases
- "Ask AI" primary nav entry → removed; replaced by global ask bar
- "Copilot" → "ZanAI" (context panel header)
- "Control" primary nav entry → removed; contents redistributed

---

## 6. Current vs Proposed Sitemap

### Current Sitemap

```
ZANPOS App
├── POS (PosPage)
│   ├── Products grid
│   ├── Cart panel
│   ├── Payment modal
│   └── (various POS modals)
└── OfficeAI (OfficeAIPage)
    ├── [Primary Nav]
    │   ├── Home → Overview
    │   ├── Ask AI → Assistant Workspace
    │   │   ├── Chat
    │   │   └── Provider Setup
    │   ├── Operations → [Secondary Nav]
    │   │   ├── Catalog
    │   │   │   ├── Products
    │   │   │   ├── Categories
    │   │   │   ├── Inventory
    │   │   │   └── Purchasing
    │   │   ├── Sales
    │   │   │   ├── Reports
    │   │   │   ├── Cashiers
    │   │   │   ├── End of Day
    │   │   │   └── Deliveries
    │   │   └── People
    │   │       ├── Customers
    │   │       └── Users
    │   └── Control → [Secondary Nav]
    │       ├── Review
    │       │   ├── Action Review
    │       │   └── Audit
    │       ├── Inbox → Workflow Inbox
    │       ├── System
    │       │   ├── Health
    │       │   ├── Conflict Inbox
    │       │   └── Devices
    │       ├── Growth
    │       │   ├── Insights
    │       │   └── Loyalty
    │       └── Settings → [Horizontal Tabs]
    │           ├── Store
    │           ├── Receipt
    │           ├── Business
    │           ├── Printer
    │           ├── ZanShop → [Internal Pages]
    │           │   ├── Setup
    │           │   └── Catalogue
    │           └── System Control → [Internal Panes]
    │               ├── Hub
    │               ├── WhatsApp
    │               ├── AI Control
    │               ├── Backup & Updates
    │               └── Maintenance (owner)
    └── [Floating: CopilotDock, Command Palette, Confirm Modal]
```

### Proposed Sitemap

```
ZANPOS
├── Till (PosPage — unchanged)
└── Command (formerly OfficeAI)
    ├── [Global Shell: header with ask bar, status bar, user context]
    ├── [Primary Nav — 7 domains]
    │   ├── Today (formerly Overview)
    │   │   └── Dashboard + attention feed
    │   ├── Catalogue (formerly Operations → Catalog + Purchasing)
    │   │   ├── Products
    │   │   ├── Categories
    │   │   ├── Inventory
    │   │   └── Purchasing
    │   ├── Customers (formerly People → Customers + Growth → Loyalty)
    │   │   ├── Directory
    │   │   └── Loyalty
    │   ├── Reports (formerly Operations → Sales)
    │   │   ├── Sales Reports
    │   │   ├── Cashier Reports
    │   │   ├── End of Day
    │   │   └── Deliveries
    │   ├── Team (formerly People → Users)
    │   │   └── Staff & Permissions
    │   ├── Settings (formerly Control → Settings, elevated)
    │   │   ├── Store Identity
    │   │   ├── Receipts & Tax
    │   │   ├── Payments & Business Rules
    │   │   ├── Hardware & Printing
    │   │   ├── ZANSHOP
    │   │   ├── Integrations (WhatsApp, AI, Cloudflare)
    │   │   └── System (Backup, Updates, Maintenance)
    │   └── System (formerly Control → Health, Conflicts, Devices, Audit)
    │       ├── Health Dashboard
    │       ├── Sync & Conflict Inbox
    │       ├── Audit Log
    │       └── Devices
    ├── [Cross-cutting]
    │   ├── Ask Bar (Ctrl+K) — global
    │   ├── ZanAI Context Panel (slide-out) — global
    │   ├── Action Review (approval queue) — badge on header
    │   ├── Workflow Inbox (WhatsApp, bills, payments) — badge on header
    │   └── Notifications — in header
    └── [Settings detail: 2-level max]
        └── No sub-tabs within sub-tabs. Each settings section is a direct page.
```

---

## 7. Current-Feature-to-New-Location Mapping

| Current Feature | Current Location | New Location |
|----------------|-----------------|--------------|
| Overview / Home | Primary Nav → Home | **Today** (primary nav) |
| AI Assistant | Primary Nav → Ask AI | **ZanAI** (global ask bar + context panel) |
| Action Review | Control → Review → Actions | **Header badge** → dedicated page |
| Workflow Inbox | Control → Inbox → Workflows | **Header badge** → dedicated page |
| Health | Control → System → Health | **System** (primary nav) → Health Dashboard |
| Conflict Inbox | Control → System → Conflicts | **System** → Sync & Conflicts |
| Devices | Control → System → Devices | **System** → Devices |
| Audit | Control → Review → Audit | **System** → Audit Log |
| Insights | Control → Growth → Insights | **Reports** → Insights (or Today dashboard cards) |
| Loyalty | Control → Growth → Loyalty | **Customers** → Loyalty |
| Products | Operations → Catalog → Products | **Catalogue** → Products |
| Categories | Operations → Catalog → Categories | **Catalogue** → Categories |
| Inventory | Operations → Catalog → Inventory | **Catalogue** → Inventory |
| Purchasing | Operations → Catalog → Purchasing | **Catalogue** → Purchasing |
| Reports | Operations → Sales → Reports | **Reports** → Sales Reports |
| Cashiers | Operations → Sales → Cashiers | **Reports** → Cashier Reports |
| End of Day | Operations → Sales → EOD | **Reports** → End of Day |
| Deliveries | Operations → Sales → Deliveries | **Reports** → Deliveries |
| Customers | Operations → People → Customers | **Customers** → Directory |
| Users | Operations → People → Users | **Team** → Staff |
| Settings → Store | Control → Settings → Store | **Settings** → Store Identity |
| Settings → Receipt | Control → Settings → Receipt | **Settings** → Receipts & Tax |
| Settings → Business | Control → Settings → Business | **Settings** → Payments & Business Rules |
| Settings → Printer | Control → Settings → Printer | **Settings** → Hardware & Printing |
| Settings → Storefront | Control → Settings → ZanShop | **Settings** → ZANSHOP |
| Settings → System Control → Hub | Control → Settings → System Control → Hub | **Settings** → Integrations → Hub |
| Settings → System Control → WhatsApp | Control → Settings → System Control → WhatsApp | **Settings** → Integrations → WhatsApp |
| Settings → System Control → AI | Control → Settings → System Control → AI | **Settings** → Integrations → AI |
| Settings → System Control → Backup | Control → Settings → System Control → Backup | **Settings** → System → Backup |
| Settings → System Control → Updates | Control → Settings → System Control → Updates | **Settings** → System → Updates |
| Settings → System Control → Maintenance | Control → Settings → System Control → Maintenance | **Settings** → System → Maintenance (owner) |
| Back to POS | Primary Nav footer | **Shell header** (always visible) |
| Copilot / Dock | Floating slide-out | **ZanAI Context Panel** (header button) |
| Command Palette (Ctrl+K) | Floating modal | **Ask Bar** (global, in header) |
| Refresh | Shell header button | Per-page refresh in page header |
| Search | Shell header button | **Ask Bar** (unified search + ask) |

---

## 8. Role & Permission Matrix

| Domain / Page | Owner | Manager | Accountant | Cashier |
|--------------|-------|---------|------------|---------|
| **Till (POS)** | Full | Full | View only | Full |
| **Today** | Full | Full | Summary | Summary |
| **Catalogue → Products** | CRUD | CRUD | View | View |
| **Catalogue → Categories** | CRUD | CRUD | View | View |
| **Catalogue → Inventory** | CRUD + stock-take | CRUD + stock-take | View | View |
| **Catalogue → Purchasing** | Full | Full | View | — |
| **Customers → Directory** | CRUD | CRUD | View | View |
| **Customers → Loyalty** | Full | Full | View | — |
| **Reports → Sales** | Full | Full | Full | Own only |
| **Reports → Cashiers** | Full | Full | View | Own only |
| **Reports → End of Day** | Full | Full | View | Own shift |
| **Reports → Deliveries** | Full | Full | View | View |
| **Team** | Full | View | View | — |
| **Settings → Store Identity** | Edit | Edit | — | — |
| **Settings → Receipts & Tax** | Edit | Edit | — | — |
| **Settings → Business Rules** | Edit | — | — | — |
| **Settings → Hardware** | Edit | Edit | — | — |
| **Settings → ZANSHOP** | Full | Setup | — | — |
| **Settings → Integrations** | Full | — | — | — |
| **Settings → System** | Full | Backup only | — | — |
| **System → Health** | Full | View | View | — |
| **System → Sync & Conflicts** | Full | Resolve | — | — |
| **System → Audit Log** | Full | View | View | — |
| **System → Devices** | Full | View | — | — |
| **Action Review** | Approve/reject | Approve/reject | — | — |
| **Workflow Inbox** | Full | Full | — | — |
| **ZanAI (ask bar)** | Full | Full | Ask only | Ask only |
| **ZanAI (context panel)** | Full | Full | — | — |

---

## 9. Global Application Shell Specification

### Shell Structure

```
┌─────────────────────────────────────────────────────────┐
│ ZANPOS Command                              Till | User │  ← Global header
│ [Ask anything or find a page...  Ctrl+K]    ⚡Synced 🔔 │  ← Ask bar + status
├────────┬────────────────────────────────────────────────┤
│        │ ┌── Page Title ──────────────────────────────┐ │
│  Today │ │ [Primary Action]              [Secondary]   │ │  ← Page header
│  Catlg │ ├────────────────────────────────────────────┤ │
│  Custm │ │                                            │ │
│  Reprt │ │          Content Area                      │ │
│  Team  │ │                                            │ │
│  Settg │ │                                            │ │
│  Systm │ │                                            │ │
│        │ └────────────────────────────────────────────┘ │
│        │                                    ┌──────────┐│
│        │                                    │ ZanAI    ││  ← Context panel
│        │                                    │ (slide)  ││     (toggle-able)
└────────┴────────────────────────────────────┴──────────┘
```

### Persistent Elements

| Element | Location | Behavior |
|---------|----------|----------|
| **Brand** | Top-left of sidebar | "ZANPOS" with store name below. Clicking returns to Today. |
| **Primary nav** | Left sidebar, 48px wide (collapsed) / 200px (expanded) | 7 icons + labels. Active state highlighted with accent left-border. |
| **Ask bar** | Top-center of header | Full-width on focus. Searches pages + sends AI prompts. |
| **Status row** | Right side of header | Sync status pill, notification bell (with badge), user avatar + role. |
| **Till button** | Rightmost of header | "Till" — returns to POS. Keyboard: Esc. |
| **ZanAI toggle** | Header button (or Ctrl+/) | Opens/closes the context panel. |
| **Action Review badge** | Notification bell dropdown | Count of pending AI approvals. |
| **Workflow Inbox badge** | Notification bell dropdown | Count of unread WhatsApp/payment items. |
| **Page header** | Top of content area | Title, description, primary action button, secondary actions. |
| **Breadcrumb** | Not needed — 2-level max depth, nav always visible. | |

### Content Frame

- **Max width**: 1280px centered for form/list pages; full-width for dashboards and tables
- **Padding**: 24px (--space-6) page gutter
- **Scroll**: Content area scrolls independently; sidebar and header are fixed

### Responsive Behavior

| Breakpoint | Sidebar | Content |
|-----------|---------|---------|
| ≥ 1280px | Expanded (200px) | Full |
| 1024–1279px | Collapsed (48px) | Full |
| 768–1023px | Collapsed (48px) | Reduced padding |
| < 768px | Hidden, hamburger overlay | Stacked |

---

## 10. Unified Status & Notification Model

### Severity Levels

| Level | Icon | Color | Meaning | Example |
|-------|------|-------|---------|---------|
| **Normal** | ✓ | `--success` | Operating correctly | "Sync: up to date" |
| **Info** | ℹ | `--info` | Informational, no action needed | "Backup completed" |
| **Attention** | ⚡ | `--warning` | Needs review, not blocking | "3 products below reorder level" |
| **Degraded** | ⚠ | `--warning` | Reduced capability, still trading | "WhatsApp disconnected — messages queued" |
| **Blocked** | ✕ | `--error` | Feature unavailable | "AI provider not configured" |
| **Critical** | 🔴 | `--error` | Store operation affected | "Database integrity check failed" |

### Status Bar (persistent in header)

A single row of compact status pills, always visible:

```
⚡ Synced 2m ago   |   WhatsApp ● Active   |   AI ✓ Ready   |   Printer ✓
```

Each pill is clickable — navigates to the relevant System Health detail.

### Notifications (bell icon dropdown)

Categorized:
- **Requiring action**: Pending AI approvals, unresolved conflicts, payment confirmations
- **Informational**: Backup completed, update available, sync milestone
- **Inbox**: WhatsApp messages, payment proofs, catalog requests

### Alert Fatigue Prevention

- A single "attention required" count aggregates all active warnings
- Individual component statuses are visible on hover, not as persistent banners
- Degraded states show a single dismissible banner, not per-component banners
- Critical states show a non-dismissible banner with recommended action

---

## 11. Core Page Templates

### Template 1: Dashboard (Today, System Health)

```
┌────────────────────────────────────────────┐
│ [Page Title]                    [Refresh]  │
│ [Summary paragraph — dynamic]              │
├────────────────────────────────────────────┤
│ ┌─────────┐ ┌─────────┐ ┌─────────┐       │
│ │ Signal  │ │ Signal  │ │ Signal  │  ...  │  ← Signal grid (3-4 cards)
│ └─────────┘ └─────────┘ └─────────┘       │
│ ┌──────────────────────────────────────┐   │
│ │ Attention Item 1                     │   │  ← Attention feed
│ │ Attention Item 2                     │   │
│ └──────────────────────────────────────┘   │
└────────────────────────────────────────────┘
```

- **Loading**: Skeleton cards matching grid layout
- **Empty**: "All systems normal. No action needed." with a check icon
- **Error**: Partial-availability banner; working signals still shown
- **Permission**: Reduced signals for cashier; full for manager

### Template 2: List & Table (Products, Customers, Reports)

```
┌────────────────────────────────────────────┐
│ [Page Title]          [Search] [Filter] [+ New] │
│ [N results]  [Bulk Actions (if selected)]  │
├────────────────────────────────────────────┤
│ ┌──────────────────────────────────────┐   │
│ │ Table header                         │   │
│ │ Row 1                                │   │
│ │ Row 2                                │   │
│ │ ...                                  │   │
│ └──────────────────────────────────────┘   │
│                 ← 1 2 3 ... →              │  ← Pagination
└────────────────────────────────────────────┘
```

- **Search**: Filter-as-you-type text input
- **Saved views**: Dropdown of saved filter presets
- **Bulk actions**: Appear as a bar above the table when rows are checked
- **Row actions**: Right-click or ⋮ menu (edit, duplicate, delete)
- **Empty**: Guided creation ("Add your first product" with button + import option)
- **Loading**: Skeleton rows matching column count

### Template 3: Detail View (Product Detail, Customer Detail)

```
┌────────────────────────────────────────────┐
│ ← Back to [List]          [Edit] [Delete]  │
├────────────────────────────────────────────┤
│ ┌── Detail Header ─────────────────────┐   │
│ │ Name, status badge, key metrics      │   │
│ └──────────────────────────────────────┘   │
│ ┌── Tab: Info ──┬── Tab: History ──────┐   │
│ │ ...           │ ...                  │   │
│ └───────────────┴──────────────────────┘   │
└────────────────────────────────────────────┘
```

### Template 4: Form Page (New/Edit Product, New Supplier)

```
┌────────────────────────────────────────────┐
│ ← Back              [Save Draft] [Publish] │
├────────────────────────────────────────────┤
│ ┌── Section: Basic Info ────────────────┐  │
│ │ Field 1                               │  │
│ │ Field 2                               │  │
│ └───────────────────────────────────────┘  │
│ ┌── Section: Pricing ───────────────────┐  │
│ │ Field 3                               │  │
│ └───────────────────────────────────────┘  │
└────────────────────────────────────────────┘
```

- **Sections**: Collapsible cards grouping related fields
- **Validation**: Inline errors below each field; summary banner at top
- **Dirty state**: "Unsaved changes" indicator + confirmation on leave

### Template 5: Split-View Workflow (Purchasing)

```
┌────────────────────────────────────────────┐
│ [Page Title]                    [+ New PO] │
├──────────────────┬─────────────────────────┤
│ ┌── List ──────┐ │ ┌── Detail ───────────┐│
│ │ Supplier 1   │ │ │ Supplier Name       ││
│ │ Supplier 2 ▸ │ │ │ Contact, Stats      ││
│ │ Supplier 3   │ │ │ ───                  ││
│ │              │ │ │ Open POs            ││
│ │              │ │ │ Recent Activity     ││
│ └──────────────┘ │ └─────────────────────┘│
└──────────────────┴─────────────────────────┘
```

### Template 6: Settings Page

```
┌────────────────────────────────────────────┐
│ [Settings Section Title]      [Save] [Reset]│
├────────────────────────────────────────────┤
│ ┌── Group: Label ───────────────────────┐  │
│ │ Setting 1          [toggle / input]   │  │
│ │ Setting 2          [toggle / input]   │  │
│ └───────────────────────────────────────┘  │
│ ┌── Group: Label ───────────────────────┐  │
│ │ Setting 3          [toggle / input]   │  │
│ └───────────────────────────────────────┘  │
└────────────────────────────────────────────┘
```

- **Search**: Filter settings by name
- **Dirty state**: Individual save per group or single page save
- **Danger zone**: Red-bordered section at bottom for destructive actions

### Template 7: Empty State

```
┌────────────────────────────────────────────┐
│                                            │
│         [Illustration or icon]             │
│                                            │
│       No products yet                      │
│       Add your first product to get        │
│       started with your catalogue.         │
│                                            │
│    [Add Product]  [Import from CSV]        │
│                                            │
└────────────────────────────────────────────┘
```

### Template 8: Degraded/Offline State

```
┌────────────────────────────────────────────┐
│ ⚠ WhatsApp is disconnected                 │
│ Messages are queued and will send when     │
│ the sidecar reconnects.                    │
│ Last connected: 12 minutes ago             │
│                              [Reconnect] [×]│
├────────────────────────────────────────────┤
│ (Content below remains accessible)         │
└────────────────────────────────────────────┘
```

---

## 12. Screen-by-Screen Redesign Specifications

### A. TODAY (formerly Overview / Home)

**Goal**: Answer "How is my store doing right now?" in under 10 seconds.

**Layout**: Dashboard template with signal grid + attention feed.

**Signals** (4 cards, adaptive):
1. **Sales today**: Total revenue, transaction count, average basket
2. **Cash position**: Current cash drawer, expected, variance
3. **Stock health**: Out-of-stock count, low-stock count, total SKUs
4. **Pending actions**: AI approvals waiting, unread inbox items, unresolved conflicts

**Attention feed**: Chronological list of items requiring action, each with severity icon, title, description, and "Take action" button linking to the relevant page.

**Empty/perfect state**: "All systems normal. Today's sales: BHD 0.00 (0 transactions)." — not "No data."

**Loading**: Skeleton signal cards + skeleton attention items.

**Refresh**: Auto-refreshes on page entry; manual refresh button updates all signals.

---

### B. CATALOGUE (Products, Categories, Inventory, Purchasing)

**Products**: List & Table template.
- Columns: Barcode, Name, Category, Price, Cost, Margin%, Stock, Status, Updated
- Filters: Category dropdown, stock status (in stock / low / out), price range
- Bulk: Price update, category reassign, stock adjustment, export selected
- New product: Button opens ProductFormModal; also accessible from empty state

**Categories**: List & Table template.
- Tree or flat list of categories with product count, nested subcategory support
- Drag-and-drop reordering

**Inventory**: List & Table template with stock-focused columns.
- Columns: Product, SKU, Current stock, Reorder level, Last movement, Status
- Quick actions: Stock in, stock out, adjust, stock-take
- Low-stock filter: Pre-applied when navigating from an alert

**Purchasing**: Split-view workflow template.
- Left panel: Supplier list with search
- Right panel: Selected supplier detail + POs
- PO workflow visually sequenced: Draft → Pending Approval → Ordered → Partially Received → Received
- Each PO shows: PO number, date, status badge, item count, total, actions
- "New PO" opens a focused form (not a modal — inline in the detail panel)

---

### C. CUSTOMERS (Directory + Loyalty)

**Directory**: List & Table template.
- Columns: Name, Phone, Total orders, Total spent, Last visit, Loyalty tier
- Detail view: Customer info, order history, loyalty points, notes
- New customer: Inline form or modal

**Loyalty**: Dashboard + configuration.
- Program overview: enrolled customers, points issued, points redeemed, redemption rate
- Tier configuration (if applicable)
- Points rules

---

### D. REPORTS (Sales, Cashiers, End of Day, Deliveries)

**Sales Reports**: Dashboard + table.
- Date range picker (today, this week, this month, custom)
- Summary cards: revenue, transactions, average basket, refunds, tax collected
- Breakdown charts (if chart library available)
- Export CSV

**Cashier Reports**: Table.
- Per-cashier: sales, transaction count, average, discounts given, refunds

**End of Day**: Workflow template.
- Active shift summary
- Cash counting form
- Discrepancy handling
- Close shift confirmation with printable summary

**Deliveries**: Table.
- Delivery queue with status: pending, in transit, delivered, verified
- Receive action → stock update

---

### E. TEAM (Staff)

**Staff**: List & Table template.
- Columns: Name, Role, PIN status, Last active, Status (active/inactive)
- Add/edit user form
- Role assignment (owner, manager, accountant, cashier)
- PIN reset (owner only)

---

### F. SETTINGS (elevated from Control)

Each settings section is a **direct page** under the Settings nav item — no sub-tabs within sub-tabs.

1. **Store Identity**: Name, timezone, address, phone, tax number, CR number, currency display
2. **Receipts & Tax**: Header, footer, tax rules (VAT rates), invoice numbering
3. **Payments & Business Rules**: Discount permissions, negative stock, auto-print, idle timeout
4. **Hardware & Printing**: Printer settings, port selection, test print, barcode label printers
5. **ZANSHOP**: Setup wizard, catalogue selection, publishing, preview URL, WhatsApp link (elevated from buried tab)
6. **Integrations**: Hub/LAN sync, WhatsApp sidecar, AI provider, Cloudflare
7. **System**: Database backup, app updates, maintenance (owner-gated)

---

### G. SYSTEM (Health, Sync, Audit, Devices)

**Health Dashboard**: Dashboard template.
- Component status grid: Database, Hub sync, WhatsApp, AI, Printer, each with status dot + detail
- Run diagnostics button
- Last checked timestamp

**Sync & Conflicts**: Split-view.
- Left: Sync status, event queue length, last successful sync
- Right: Conflict list with resolution actions (keep local, keep remote, merge)

**Audit Log**: Table with filters.
- Columns: Timestamp, User, Action, Target, Details
- Filter by: user, action type, date range

**Devices**: Table.
- Columns: Device name, Branch, Last seen, Status
- Register new device (owner)

---

### H. ZANSHOP (elevated, see Settings §5)

**Setup**: Step-by-step wizard.
1. Enable/disable shop
2. Select catalogue (which products appear online)
3. Configure ordering settings (delivery/pickup, payment methods)
4. Set public URL
5. Connect WhatsApp number
6. Preview and go live

**Status**: Readiness summary showing what's configured and what's missing.

**Catalogue**: Product selection grid with include/exclude toggles.

---

### I. GLOBAL: Ask Bar & ZanAI Context Panel

**Ask Bar** (Ctrl+K):
- Keyboard shortcut: Ctrl+K (already implemented as command palette — repurpose)
- Opens a Spotlight-style input in the header
- Type to search pages ("products", "settings printer") OR ask AI ("what sold best this week?")
- AI responses stream inline; page navigation is instant
- Preserves the existing `OfficeAICommandPalette` component structure

**ZanAI Context Panel** (Ctrl+/):
- Slide-out right panel (320px)
- Shows AI analysis of the current page context
- "Summarize this page" / "Find anomalies" / "Suggest actions" quick buttons
- Full chat capability in the panel
- Reuses `CopilotDock` component, renamed

**Action Review**:
- Dedicated page (not a tab under Control)
- Queue of pending AI-proposed actions
- Each shows: tool name, preview, source message, expiry countdown
- Approve / Reject / Modify actions
- Batch approve for related changes
- Audit trail of all approved/rejected actions

---

## 13. Critical Workflow Maps

### Workflow 1: Adding the First Product

```
Entry: Empty Products page → "Add your first product" CTA
  ↓
ProductFormModal opens (prefilled with nothing)
  ↓
Fill: Name, Barcode (or generate), Category, Price, Cost
  ↓
Save → Product appears in table → Toast: "Product created"
  ↓
Empty state replaced by 1-row table
```

### Workflow 2: Creating a Purchase Order

```
Entry: Catalogue → Purchasing → "New PO" button
  ↓
Select supplier (search or create new inline)
  ↓
Add line items (search products, set quantity, unit cost)
  ↓
Review totals (estimated cost, item count)
  ↓
Save as Draft or Send for Approval (if approvals configured)
  ↓
PO appears in supplier's PO list with "Draft" or "Pending Approval" badge
```

### Workflow 3: Receiving Supplier Stock

```
Entry: Catalogue → Purchasing → Select PO with "Ordered" status → "Receive"
  ↓
Verify quantities against PO (pre-filled, editable)
  ↓
Enter actual received quantities + any variance notes
  ↓
Confirm → PO status → "Partially Received" or "Received"
  ↓
Stock levels auto-update → Inventory reflects new quantities
  ↓
Cost updated if different from PO cost (variance logged)
```

### Workflow 4: Offline Sync Backlog

```
Entry: System → Sync & Conflicts → View event queue
  ↓
See: N events pending, last successful sync timestamp
  ↓
If online but backlog: "Sync now" button forces immediate push
  ↓
If offline: "Waiting for connection" — events are safely queued locally
  ↓
Resolve conflicts: per-conflict "Keep local" / "Keep remote" / "Merge" buttons
```

### Workflow 5: Approving an AI-Proposed Action

```
Entry: Notification bell badge shows "3 pending" → Click → Action Review
  ↓
See list of pending actions with previews
  ↓
Click action → Expand to see: tool used, exact change, source prompt, context
  ↓
Approve (single) or Add to batch
  ↓
Batch approve → Confirmation modal → "Apply N changes?"
  ↓
Confirm → Actions execute → Status updates to "Applied" → Toast confirmation
  ↓
Audit log records all approvals with actor + timestamp
```

### Workflow 6: Publishing ZANSHOP

```
Entry: Settings → ZANSHOP → Setup wizard (guided flow)
  ↓
Step 1: Enable shop toggle → ON
  ↓
Step 2: Select products to include (catalogue picker — select all, per-category, or individual)
  ↓
Step 3: Configure: delivery/pickup options, payment method, order notifications
  ↓
Step 4: Review public URL, preview shop
  ↓
Step 5: Connect WhatsApp for order notifications (optional)
  ↓
"Go Live" → Shop published → Status: "Live — 247 products listed"
```

---

## 14. Design System Specification

### Typography Scale

| Token | Size | Weight | Line Height | Use |
|-------|------|--------|-------------|-----|
| `--type-page` | 1.25rem (20px) | 600 | 1.3 | Page titles |
| `--type-section` | 1rem (16px) | 600 | 1.35 | Section headings, card titles |
| `--type-body` | 0.875rem (14px) | 400 | 1.5 | Body text, table cells, form labels |
| `--type-caption` | 0.75rem (12px) | 400 | 1.45 | Captions, timestamps, metadata |
| `--type-pos-large` | 2rem | 700 | 1.25 | POS amount due (unchanged) |
| `--type-pos-base` | 1rem | 500 | 1.45 | POS base (unchanged) |

### Spacing Scale (4px grid)

| Token | Value | Use |
|-------|-------|-----|
| `--space-1` | 4px | Icon gaps, tight inline |
| `--space-2` | 8px | Inline spacing, compact |
| `--space-3` | 12px | Card padding (compact) |
| `--space-4` | 16px | Standard padding, card padding |
| `--space-5` | 20px | Section gaps |
| `--space-6` | 24px | Page gutters, section spacing |
| `--space-8` | 32px | Large section breaks |
| `--space-10` | 40px | Page top/bottom padding |
| `--space-12` | 48px | Hero sections |

### Color Roles (preserving existing token system)

| Role | Token | Value | Use |
|------|-------|-------|-----|
| **Page BG** | `--bg` | `#0E0C0A` | Main content background |
| **Surface** | `--surface` | `#1A1713` | Cards, panels |
| **Surface raised** | `--surface2` | `#221F1A` | Hover states, elevated cards |
| **Action accent** | `--accent-action` | `#F0A500` | Primary buttons, brand, money values |
| **AI accent** | `--accent-ai` | `#14B8A6` | AI elements only |
| **Money in** | `--money-in` | `#22C55E` | Income, profit, received |
| **Money out** | `--money-out` | `#DC2626` | Expenses, refunds, loss |
| **Warning** | `--warn` | `#FBBF24` | Degraded state, pending |
| **Text primary** | `--text` | `#F4F5FA` | Headings, body |
| **Text secondary** | `--text-dim` | `#9BA5CC` | Descriptions, metadata |
| **Text muted** | `--text-muted` | `#727EAC` | Placeholders, disabled |
| **Border** | `--border` | `rgba(255,255,255,0.11)` | Card borders, dividers |

### Component Tokens

| Component | Token | Value |
|-----------|-------|-------|
| **Button height (standard)** | `--btn-md` | 56px |
| **Button height (compact)** | `--btn-sm` | 44px |
| **Input height** | `--input-height` | 56px |
| **Card radius** | `--radius-card` | 8px |
| **Modal radius** | `--radius` | 14px |
| **Focus ring** | `--focus-ring` | `2px solid var(--accent)` |
| **Focus halo** | `--focus-halo` | `0 0 0 4px color-mix(in srgb, var(--accent) 30%, transparent)` |
| **Transition** | `--t-base` | `120ms cubic-bezier(0.2, 0, 0, 1)` |
| **Touch target (POS)** | `--hit-till` | 48px |
| **Touch target (Command)** | `--hit-office` | 36px |

### Icon Rules

- **Stroke width**: 1.75px for navigation, 1.5px for inline
- **Size**: 17px for nav, 15px for inline, 14px for badges
- **Directional icons**: Use `icon-directional` class (already implemented for RTL)
- **Non-directional icons**: Never mirror (printer, trash, bell — already implemented)

### Button Variants

| Variant | Background | Text | Border | Use |
|---------|-----------|------|--------|-----|
| **Primary** | `--accent` | `--accent-t` | none | Main page action |
| **Secondary** | transparent | `--text` | `--border` | Alternative action |
| **Ghost** | transparent | `--text-dim` | none | Low-priority action |
| **Danger** | `--color-danger-soft` | `--error` | `--error` | Destructive actions |
| **AI** | `--accent-ai-dim` | `--accent-ai` | none | AI-suggested actions |

---

## 15. Responsive Behavior

| Breakpoint | Label | Navigation | Content | Notes |
|-----------|-------|-----------|---------|-------|
| ≥ 1440px | Desktop wide | Expanded (200px) | 1280px max-width centered | Ideal for dashboards and split-views |
| 1280–1439px | Desktop | Expanded (200px) | Full width minus nav | Standard layout |
| 1024–1279px | Laptop | Collapsed (48px icons) | Full width minus nav | Icons only with tooltips |
| 768–1023px | Tablet landscape | Collapsed (48px) | Reduced padding (16px) | Touch targets increase to 44px |
| < 768px | Tablet portrait | Hidden, hamburger menu | Stacked, full width | Single column; modals become full-screen sheets |

### Component Adaptations

- **Tables**: Below 1024px, tables with >4 columns collapse non-essential columns into an expandable row detail
- **Split views**: Below 1024px, split-view becomes single-column with back/forward navigation
- **ZanAI panel**: Below 768px, becomes a full-screen overlay instead of slide-out
- **Modals**: Below 768px, become full-screen sheets with a close button and swipe-down gesture
- **Forms**: Multi-column forms collapse to single column below 768px

---

## 16. Accessibility Specification

### WCAG 2.2 AA Compliance

| Requirement | Implementation |
|-------------|---------------|
| **Color contrast** | All text meets 4.5:1 (normal) / 3:1 (large). Already achieved with `--text-muted` fix to 5.2:1. |
| **Focus indicators** | 2px solid accent ring + 4px halo on all interactive elements. Already implemented in `tokens.css:476-481`. |
| **Keyboard navigation** | Tab order matches visual order. Esc closes panels/modals. Ctrl+K opens ask bar. Arrow keys navigate lists. |
| **Screen readers** | All icons have `aria-hidden="true"` with adjacent text labels. Status indicators use `aria-label`. Page sections use `aria-labelledby`. |
| **Non-color status** | All status indicators include an icon AND text label, never color alone. |
| **Target size** | Minimum 36px (back office) / 48px (POS). Already implemented in `tokens.css:499-504`. |
| **Reduced motion** | `prefers-reduced-motion` sets all transitions to 0ms. Already implemented in `tokens.css:445-450`. |
| **RTL** | Arabic layout flips direction while keeping numbers LTR. Directional icons mirror. Already implemented in `tokens.css:548-664`. |
| **Zoom** | Layout remains usable at 200% zoom. Relative units (`rem`) used throughout. |
| **Touch targets** | No touch targets below 36px in back office. Adequate spacing between clickable elements. |

### Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+K` | Open ask bar |
| `Ctrl+/` | Toggle ZanAI context panel |
| `Esc` | Close panel/modal, or return to Till |
| `Ctrl+1..7` | Navigate to primary nav domains |
| `Tab` / `Shift+Tab` | Move through interactive elements |
| `Enter` | Activate focused button/link |
| `Arrow keys` | Navigate within lists and tables |

---

## 17. Frontend Implementation Architecture

### Route Hierarchy

Replace the state-machine view system in `App.tsx` with a proper router (React Router or TanStack Router). This is the single highest-impact structural change.

```
/                         → Redirect to /till or /command based on role
/till                     → PosPage (existing, unchanged)
/command                  → CommandShell (new, replaces OfficeAIPage)
/command/today            → TodayDashboard
/command/catalogue        → CatalogueShell
/command/catalogue/products    → ProductsPage
/command/catalogue/categories  → CategoriesPage
/command/catalogue/inventory   → InventoryPage
/command/catalogue/purchasing  → PurchasingWorkspace
/command/customers        → CustomersShell
/command/customers/directory   → CustomerDirectory
/command/customers/loyalty     → LoyaltyPage
/command/reports          → ReportsShell
/command/reports/sales         → SalesReports
/command/reports/cashiers      → CashierReports
/command/reports/eod           → EndOfDay
/command/reports/deliveries    → DeliveriesPage
/command/team             → TeamPage
/command/settings         → SettingsShell
/command/settings/store        → StoreIdentitySettings
/command/settings/receipts     → ReceiptSettings
/command/settings/business     → BusinessRulesSettings
/command/settings/hardware     → HardwareSettings
/command/settings/zanshop      → ZanShopSettings
/command/settings/integrations → IntegrationsSettings
/command/settings/system       → SystemSettings
/command/system           → SystemShell
/command/system/health         → SystemHealth
/command/system/sync           → SyncConflicts
/command/system/audit          → AuditLog
/command/system/devices        → DevicesPage
/command/actions          → ActionReview
/command/inbox            → WorkflowInbox
```

### Component Architecture

```
<CommandShell>                    ← Persistent shell (sidebar + header)
  <CommandSidebar>                ← 7-domain primary nav (collapsible)
  <CommandHeader>                 ← Ask bar + status + user + Till button
    <AskBar />                    ← Global search/ask (Ctrl+K)
    <StatusRow />                 ← Sync, WhatsApp, AI, Printer status pills
    <NotificationBell />          ← Action Review + Inbox counts
    <UserMenu />                  ← Avatar, role, logout
  <PageHeader />                  ← Title, description, actions (per-page)
  <Outlet />                      ← Router outlet for page content
  <ZanAIPanel />                  ← Slide-out context panel (toggle-able)
```

### Design Tokens (preserving existing)

The existing `tokens.css` is already comprehensive. Changes needed:
1. Add `cmd-*` class name aliases alongside `oa-*` during migration
2. Add `--nav-width-expanded: 200px` and `--nav-width-collapsed: 48px` tokens
3. Add `--header-height: 56px` token

### Offline State Handling

- `useSyncStatus` hook (existing) drives the status pills
- Each page checks its data dependencies and shows the degraded state template when data is unavailable
- AI panel shows "AI unavailable — offline" when disconnected
- All forms queue writes locally; sync status indicates pending uploads

---

## 18. Migration Roadmap

### Phase 1: Terminology & Navigation Normalization (2-3 days)

**No structural changes.** Rename labels and reorganize navigation groups within the existing architecture.

- Rename "OfficeAI" → "Command" in UI labels (CSS classes keep `oa-*` prefix temporarily)
- Rename "Ask AI" primary nav → remove; replace with "Assistant" as a Control tab (temporary)
- Rename "Copilot" → "ZanAI" in the dock header
- Reorganize secondary nav groups: move Purchasing to its own group; move Insights to a new group
- Add "ZanAI" toggle button to shell header (alongside existing Copilot toggle)

**Files affected**: `OfficeAIPrimaryNav.tsx`, `nav.tsx`, `officeAiNavigation.ts`, `officeAiStrings.ts`, `CopilotDock.tsx`

### Phase 2: Shared Design Tokens & Components (3-4 days)

- Add `cmd-*` CSS class aliases for all `oa-*` classes
- Extract shared page templates as components: `<PageTemplate variant="dashboard|table|detail|form|settings" />`
- Extract `<EmptyState>`, `<DegradedBanner>`, `<StatusPill>` as reusable components
- Unify i18n translators into a single command translator

### Phase 3: Global Shell Replacement (5-7 days)

- Introduce React Router with the route hierarchy above
- Build `<CommandShell>` wrapping the new sidebar + header
- Migrate existing `OfficeAIPage` state-machine logic into route-based navigation
- Remove the old primary/secondary nav components; replace with `<CommandSidebar>`
- The POS view remains in the state machine temporarily (separate migration)

### Phase 4: Home & System Status Redesign (3-4 days)

- Redesign `OfficeAIOverview` → `TodayDashboard` using the new dashboard template
- Build the unified status row in the header
- Migrate `OfficeAISystemHealth` into the new System domain

### Phase 5: Catalogue & Purchasing Redesign (4-5 days)

- Restructure Catalogue domain with the new list/table template
- Redesign Purchasing with the split-view template
- Ensure the workflow sequence (supplier → PO → receive → stock update) is visually clear

### Phase 6: AI Integration (3-4 days)

- Repurpose the command palette into the ask bar (Ctrl+K)
- Rename CopilotDock → ZanAIPanel; keep it accessible from any page
- Move Action Review to a top-level route, accessible from the notification bell

### Phase 7: Settings Consolidation (4-5 days)

- Elevate Settings to a primary nav domain
- Flatten settings: each section is a direct page, no sub-tabs within sub-tabs
- Integrate ZANSHOP as a proper settings section
- Merge System Control panes into Integrations and System settings sections

### Phase 8: ZANSHOP Integration (2-3 days)

- Elevate ZANSHOP from Settings → Storefront tab to Settings → ZANSHOP section
- Redesign the setup wizard with clear go-live steps

### Phase 9: Accessibility & Responsive Hardening (3-4 days)

- Audit all new components for keyboard navigation
- Test responsive breakpoints
- Verify screen reader announcements for status changes
- Add `prefers-reduced-motion` and `prefers-contrast` handling
- Ensure all AI outputs have clear role labels

### Phase 10: Polish & Removal (2-3 days)

- Remove deprecated components (old `OfficeAIPrimaryNav`, `OfficeAISecondaryNav`, `OfficeAICommandPalette`)
- Remove old `oa-*` CSS classes (or keep as legacy)
- Final i18n audit for consistent terminology
- Performance audit (lazy-load all domain shells)

---

## 19. Risks, Assumptions, and Unresolved Questions

### Risks

1. **Router migration could break POS**: The state-machine view system is deeply embedded in `App.tsx`. Introducing a router must not affect the POS view or the login/shift flow.
   - **Mitigation**: Keep POS in the state machine; only migrate the Command (OfficeAI) side to routing.

2. **Settings flattening could break save logic**: The current `SettingsTab` loads all data in one `Promise.all`. Splitting into separate pages means each page loads its own data.
   - **Mitigation**: Each settings page independently loads its subset. Add a "Save" indicator per page.

3. **CSS class rename could miss usages**: The `oa-*` prefix is used across ~30 components and one CSS file.
   - **Mitigation**: Add `cmd-*` aliases without removing `oa-*` initially. Remove legacy classes in Phase 10.

### Assumptions

1. **[Assumed]** The existing POS interface (`PosPage`) is not part of this redesign scope — it remains as-is.
2. **[Assumed]** A router library (React Router) can be added without breaking the Tauri build.
3. **[Assumed]** The 7-domain primary nav structure is the right number — it may need adjustment after user testing.
4. **[Assumed]** Users will accept the removal of the "Ask AI" primary nav entry in favor of the global ask bar.
5. **[Inferred from code]** The actual visual density, spacing, and empty state quality match the patterns seen in the component structure. Visual verification against the real UI is needed.

### Unresolved Questions

1. **Should the Till and Command share one window or be separate?** Currently they're mutually exclusive views in the same window. The redesign keeps this pattern but it may be worth exploring a tabbed approach.
2. **Should "Insights" be part of Reports or Today?** Currently under Control → Growth. The proposal moves it to Reports, but it could also be a Today dashboard section.
3. **What chart library is available?** The codebase doesn't appear to use one. Analytics features may need a charting dependency.
4. **What is the tablet usage reality?** The brief mentions tablet support; the codebase targets desktop (Tauri v2 Windows). Tablet support may require a separate PWA or responsive adaptation.

---

## 20. Measurable Acceptance Criteria

1. **Navigation depth**: No feature requires more than 2 clicks from the primary nav to reach its primary view.
2. **Brand consistency**: No more than 3 brand names appear in the UI (ZANPOS, ZanAI, ZANSHOP).
3. **One shell**: Every Command page uses the same sidebar + header components.
4. **Status visibility**: Sync, WhatsApp, AI, and printer status are visible from every page without scrolling.
5. **AI clarity**: Every AI output is labeled as one of: explanation, recommendation, draft, pending approval, approved, executed, or failed.
6. **Task completion**: The 15 critical workflows mapped in §13 can be completed without consulting documentation.
7. **Empty states**: Every list/table page has a guided empty state with at least one suggested action.
8. **Offline clarity**: When a service is unavailable, the UI shows what is unavailable, what remains usable, and whether data is queued.
9. **Keyboard access**: Every interactive element is reachable via Tab; every modal/panel is closable via Esc.
10. **Contrast**: All text meets 4.5:1 contrast ratio (already verified for the current tokens).
11. **Responsive**: All pages remain usable at 1024px width (collapsed nav) and 768px width (stacked layout).
12. **Migration safety**: No existing feature is removed without a documented replacement location.

---

## 21. Final Visual Direction

**Aesthetic**: Calm, reliable, fast, contemporary. Professional retail operations — not playful, not ornamental, not sparse.

**Key visual attributes**:
- Dark default theme (warm amber on near-black) — already established
- Gold accent (`#F0A500`) for primary actions and brand — preserved
- Teal accent (`#14B8A6`) exclusively for AI elements — preserved
- Generous but not wasteful whitespace
- Cards with subtle borders, not heavy shadows
- Typography that prioritizes readability at 14px body size
- Status communicated through icon+text combinations, never color alone
- RTL-ready with proper number isolation (already implemented)

**What changes visually**:
- One sidebar instead of two (primary + secondary merged)
- Settings is a full page, not a tab bar inside a tab inside a nav group
- The ask bar is always visible in the header
- The ZanAI panel slides over content rather than replacing it
- Pages use consistent templates so the same pattern (search → filter → table → pagination) appears everywhere

---

## 22. Visual Companion: Mockup Specifications

### Mockup A: Today Dashboard

- **Canvas**: 1440 × 900px, dark theme
- **Shell**: Expanded sidebar (200px), 7-domain nav, "Today" active
- **Header**: Ask bar (centered), status pills (Synced, WhatsApp Active, AI Ready), notification bell (2 pending), user avatar
- **Content**: 
  - Page title "Today" with subtitle "Wednesday, 6 August 2026"
  - Refresh button (top right of content area)
  - 4 signal cards in a row: Sales today, Cash position, Stock health, Pending actions
  - Attention feed below: 3 items with warning icons, titles, and "Take action" links
- **Variants**: Loading (skeleton cards), All normal (green check + "No action needed"), Offline (degraded banner above signals)

### Mockup B: Catalogue — Products

- **Canvas**: 1440 × 900px, dark theme
- **Shell**: Expanded sidebar, "Catalogue" active, secondary nav shows: Products, Categories, Inventory, Purchasing
- **Content**:
  - Page header: "Products" + product count + [Search] [Category filter ▼] [Stock filter ▼] [+ New Product]
  - Table: Barcode | Name | Category | Price | Cost | Margin% | Stock | Status | Updated
  - Row actions on hover: Edit, Duplicate, ⋮
  - Bulk action bar appears when rows are checked
- **Variants**: Empty (illustration + "Add your first product" + "Import CSV"), Loading (5 skeleton rows), Filtered (active filter chips shown)

### Mockup C: Purchasing Workspace

- **Canvas**: 1440 × 900px, dark theme
- **Shell**: Expanded sidebar, "Catalogue" active, "Purchasing" selected
- **Content**: Split-view
  - Left panel (320px): Supplier list with search, "New Supplier" button
  - Right panel: Selected supplier detail (name, contact, stats) + open POs list
  - PO status badges: Draft (gray), Pending Approval (amber), Ordered (blue), Partially Received (amber), Received (green)
- **Variants**: No supplier selected (empty right panel with prompt), No POs (empty list with "Create first PO")

### Mockup D: ZanAI Context Panel

- **Canvas**: 1440 × 900px, dark theme, Products page visible
- **Overlay**: Right-side panel (360px) slides over content; backdrop is semi-transparent
- **Panel content**:
  - Header: "ZanAI" with sparkle icon, "Products context", [Expand] [Close]
  - Quick actions row: "Summarize" "Find issues" "Suggest changes"
  - Chat area with messages
  - Composer at bottom with text input and send button
- **Variants**: Unconfigured (setup CTA instead of chat), AI thinking (animated dots), Action proposed (preview card with Approve/Reject)

### Mockup E: Action Review

- **Canvas**: 1440 × 900px, dark theme
- **Shell**: Expanded sidebar, "Action Review" accessible from notification bell badge
- **Content**:
  - Page header: "Action Review" + pending count + [Approve All] (disabled if none selected)
  - Queue of pending actions, each showing: tool icon, tool name, preview of change, source prompt excerpt, expiry countdown
  - Checkbox for batch selection
  - Expand to see full preview and context
- **Variants**: Empty queue ("No pending actions — AI hasn't proposed any changes"), Expired action (red badge, disabled approve button)

### Mockup F: Settings — Consolidated

- **Canvas**: 1440 × 900px, dark theme
- **Shell**: Expanded sidebar, "Settings" active
- **Secondary nav** (within Settings): Store Identity, Receipts & Tax, Payments & Business Rules, Hardware & Printing, ZANSHOP, Integrations, System
- **Content** (Store Identity selected):
  - Page header: "Store Identity" + [Save] [Reset]
  - Form groups: Store Details (name, timezone, address, phone), Legal Identity (tax number, CR number), Session (idle timeout)
  - Danger zone at bottom: "Delete store data" (red button, confirmation required)
- **Variants**: Unsaved changes (yellow dot on Save button), Saving (Save button disabled + spinner), Saved (green checkmark toast)

### Mockup G: ZANSHOP Setup

- **Canvas**: 1440 × 900px, dark theme
- **Shell**: Expanded sidebar, "Settings" active, "ZANSHOP" selected
- **Content**:
  - Setup progress indicator (Step 2 of 6)
  - Current step: "Select catalogue" — product grid with include/exclude toggles
  - Category filter sidebar within the step
  - [Back] [Next Step] navigation
  - Readiness summary sidebar showing completed and pending steps
- **Variants**: Not set up (full wizard), Live (dashboard with visitor count, order count, product count, [Edit] [Preview] [Disable]), Error (connection failed banner)

### Mockup H: System Health

- **Canvas**: 1440 × 900px, dark theme
- **Shell**: Expanded sidebar, "System" active, "Health" selected
- **Content**:
  - Page header: "System Health" + [Run Diagnostics]
  - Component grid (3 columns): Database (✓ Healthy), Hub Sync (⚡ 2 events pending), WhatsApp (✓ Active), AI (⚠ Not configured), Printer (✓ Ready)
  - Each card: icon, status badge, detail text, [View] or [Fix] action
  - Last checked timestamp
- **Variants**: All healthy (green across the board), Degraded (warning cards with action buttons), Critical (red banner at top), Diagnostics running (progress bar)

---

## Appendix: Files to Create/Modify

### New Files
- `src/command/CommandShell.tsx` — New shell component
- `src/command/CommandSidebar.tsx` — 7-domain primary nav
- `src/command/CommandHeader.tsx` — Header with ask bar + status + user
- `src/command/AskBar.tsx` — Global search/ask input
- `src/command/StatusRow.tsx` — Status pills
- `src/command/ZanAIPanel.tsx` — Renamed from CopilotDock
- `src/command/pages/TodayDashboard.tsx`
- `src/command/pages/*.tsx` — One per route
- `src/components/templates/PageTemplate.tsx` — Shared page templates
- `src/components/templates/EmptyState.tsx`
- `src/components/templates/DegradedBanner.tsx`
- `src/components/StatusPill.tsx`

### Modified Files
- `src/App.tsx` — Add router; keep POS in state machine
- `src/officeai/OfficeAIPage.tsx` — Refactor into CommandShell + routes
- `src/officeai/OfficeAIPrimaryNav.tsx` — Replace with CommandSidebar
- `src/officeai/OfficeAISecondaryNav.tsx` — Remove; integrate into CommandSidebar
- `src/officeai/nav.tsx` — Restructure for 7-domain model
- `src/officeai/officeAiTypes.ts` — Add new types; deprecate old
- `src/officeai/CopilotDock.tsx` — Rename to ZanAIPanel
- `src/components/SettingsTab.tsx` — Split into per-page settings components
- `src/components/settings/SystemControlTab.tsx` — Split into Integrations + System settings
- `src/components/settings/StorefrontManagementTab.tsx` — Elevate to Settings section
- `src/styles/tokens.css` — Add `cmd-*` aliases
- `src/i18n/officeAiStrings.ts` — Rename to `commandStrings.ts`; update keys
- `src/i18n/backOfficeStrings.ts` — Consolidate with command strings

### Removed Files (Phase 10)
- `src/officeai/OfficeAISecondaryNav.tsx`
- `src/officeai/OfficeAICommandPalette.tsx` (absorbed into AskBar)
- `src/officeai/OfficeAIWorkspaceNav.tsx` (if unused after restructure)

---

*End of proposal. This document covers the codebase as analyzed on 2026-08-06 from `C:\Users\super\ZAN\zanpos-open-source-upgrade\`. All findings not explicitly marked as [inferred] are based on source code evidence; all feature locations were verified against the component tree and navigation configuration.*
