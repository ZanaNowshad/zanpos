/**
 * The workflows ZanAI already knows, as things a person can ask for.
 *
 * `src-tauri/src/ai/workflows.rs` holds 21 written business procedures —
 * VAT filing, dead-stock clearance, margin erosion, cash-flow forecast — each
 * one a step-by-step guide the model loads through the `load_workflow` tool.
 * Until now the only way to reach any of them was for the model to decide to,
 * which meant the shopkeeper had to already know the procedure existed in
 * order to phrase a question that triggered it.
 *
 * These are the front door instead. On a till with no keyboard, a blank chat
 * box is the wrong opening move when the answers are already written down.
 *
 * `id` must match `WORKFLOW_REGISTRY` in workflows.rs exactly — the prompt
 * names the workflow so the model loads the right guide. `workflowCatalogue`
 * test asserts the two lists agree, because a typo here produces a chat reply
 * that silently ignores the guide rather than an error.
 */

export interface WorkflowEntry {
  /** Registry key in workflows.rs. */
  id: string;
  label: string;
  /** What the operator gets, in their words rather than the guide's. */
  blurb: string;
  /** Grouping for the launcher. */
  group: "money" | "stock" | "day" | "customers" | "system";
  /** Sent to the assistant when pressed. */
  prompt: string;
}

export const WORKFLOW_CATALOGUE: WorkflowEntry[] = [
  // ── Money ────────────────────────────────────────────────────────────────
  { id: "margin_erosion", group: "money", label: "Margin erosion review",
    blurb: "Products whose cost rose but price did not.",
    prompt: "Run the margin_erosion workflow and show me which products lost margin and what to reprice." },
  { id: "cash_flow_forecast", group: "money", label: "Cash-flow forecast",
    blurb: "What is owed to you and what you owe, short term.",
    prompt: "Run the cash_flow_forecast workflow for a short-term view of my cash position." },
  { id: "vat_filing", group: "money", label: "VAT filing preparation",
    blurb: "Assemble the Bahrain VAT return figures.",
    prompt: "Run the vat_filing workflow and prepare my Bahrain VAT figures for the current period." },
  { id: "cash_discrepancy", group: "money", label: "Cash discrepancy",
    blurb: "Investigate a drawer that did not balance.",
    prompt: "Run the cash_discrepancy workflow and investigate the shifts that did not balance." },
  { id: "supplier_invoice", group: "money", label: "Supplier invoice",
    blurb: "Process an invoice against what was received.",
    prompt: "Run the supplier_invoice workflow to process a supplier invoice against goods received." },

  // ── Stock ────────────────────────────────────────────────────────────────
  { id: "low_stock_restock", group: "stock", label: "Restock pipeline",
    blurb: "Turn low stock into a purchase order.",
    prompt: "Run the low_stock_restock workflow and prepare what I need to reorder." },
  { id: "dead_stock_clearance", group: "stock", label: "Dead stock",
    blurb: "Stock that has not moved, and what to do with it.",
    prompt: "Run the dead_stock_clearance workflow and show me aging inventory worth clearing." },
  { id: "expiry_management", group: "stock", label: "Expiry and shelf life",
    blurb: "Nearest expiry first, before it is written off.",
    prompt: "Run the expiry_management workflow and list what expires soonest." },
  { id: "seasonal_demand", group: "stock", label: "Seasonal demand",
    blurb: "Prepare stock for a season using past years.",
    prompt: "Run the seasonal_demand workflow and tell me what to stock up on." },
  { id: "bulk_operations", group: "stock", label: "Bulk changes",
    blurb: "Change many products at once, safely.",
    prompt: "Run the bulk_operations workflow — I want to change many products at once." },
  { id: "ghost_barcode", group: "stock", label: "Unknown barcode",
    blurb: "Resolve a barcode the till did not recognise.",
    prompt: "Run the ghost_barcode workflow to resolve barcodes the till could not find." },
  { id: "basket_placement", group: "stock", label: "Basket and shelf placement",
    blurb: "What sells together, and where to put it.",
    prompt: "Run the basket_placement workflow and tell me what sells together." },

  // ── The day ──────────────────────────────────────────────────────────────
  { id: "daily_briefing", group: "day", label: "Daily briefing",
    blurb: "What happened, and what needs you.",
    prompt: "Run the daily_briefing workflow and brief me on the store." },
  { id: "eod_reconciliation", group: "day", label: "End-of-day reconciliation",
    blurb: "Close the day and account for the cash.",
    prompt: "Run the eod_reconciliation workflow and walk me through closing today." },
  { id: "proactive_alerts", group: "day", label: "Work through alerts",
    blurb: "Deal with what the store flagged on its own.",
    prompt: "Run the proactive_alerts workflow and help me work through the open alerts." },
  { id: "delivery_lifecycle", group: "day", label: "Deliveries",
    blurb: "Track a delivery from order to collected cash.",
    prompt: "Run the delivery_lifecycle workflow and show me where my deliveries stand." },

  // ── Customers ────────────────────────────────────────────────────────────
  { id: "customer_winback", group: "customers", label: "Win back lapsed customers",
    blurb: "Who stopped coming, and what to send them.",
    prompt: "Run the customer_winback workflow and find customers who have stopped coming in." },
  { id: "customer_message", group: "customers", label: "Customer message",
    blurb: "Handle an incoming WhatsApp from a customer.",
    prompt: "Run the customer_message workflow to handle customer WhatsApp messages." },
  { id: "whatsapp_message", group: "customers", label: "WhatsApp to action",
    blurb: "Turn a message or photo into something actionable.",
    prompt: "Run the whatsapp_message workflow on the latest WhatsApp message." },

  // ── System ───────────────────────────────────────────────────────────────
  { id: "sync_recovery", group: "system", label: "Sync recovery",
    blurb: "Get a stuck terminal talking to the hub again.",
    prompt: "Run the sync_recovery workflow — sync looks stuck." },
  { id: "db_maintenance", group: "system", label: "Database health",
    blurb: "Check and maintain the local database.",
    prompt: "Run the db_maintenance workflow and check the database." },
];

export const WORKFLOW_GROUPS: { id: WorkflowEntry["group"]; label: string }[] = [
  { id: "day", label: "The day" },
  { id: "money", label: "Money" },
  { id: "stock", label: "Stock" },
  { id: "customers", label: "Customers" },
  { id: "system", label: "System" },
];
