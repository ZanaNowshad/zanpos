/**
 * The dev mock's command dispatch table.
 *
 * Split from uiMockData.ts, which now holds only the fixtures. The map is
 * the part that grows fastest — every command the UI learns to call needs an
 * entry — and together the two pushed the file past the 500-line limit the
 * ship gate enforces.
 */

import {
  APP_CONFIG,
  BRANCH_ID,
  CATEGORIES,
  DEVICE_ID,
  HEALTH_REPORT,
  MOCK_SESSION,
  PRODUCTS,
  PRODUCT_PAGE,
  RANGE_SUMMARY,
  SYNC_STATUS,
  WA_CONTACTS,
  WA_MESSAGES,
  WHATSAPP_STATUS,
} from "./uiMockData";

/** command → response. Anything unlisted resolves to a safe empty value. */
export const HANDLERS: Record<string, unknown> = {
  app_config_load: APP_CONFIG,
  app_config_get_timeout: 5,
  operational_settings_load: {
    loyalty_points_per_bhd: 2, retention_days_sales: 90, retention_days_logs: 30,
    sync_interval_terminal_secs: 10, sync_interval_hub_secs: 300,
  },
  settings_get_branch: { branch_id: BRANCH_ID, name: "Amwaj AlDair", code: "AMW" },
  admin_get_provider_config: { provider: "anthropic", anthropic_model: "claude-opus-5" },
  admin_list_products: PRODUCT_PAGE,
  products_list: PRODUCTS,
  /* Duplicate groups, not an empty array. Returning [] meant the only state
     this modal could ever reach in QA was "catalog looks clean" — the populated
     list, which is the state that actually needs designing, was unreachable.
     Enough groups to force paging, because a real catalogue produces hundreds. */
  admin_find_duplicate_products: Array.from({ length: 42 }, (_, g) => {
    const base = PRODUCTS[g % PRODUCTS.length];
    const kinds = ["shared barcode", "exact name", "similar name", "shared SKU"];
    const match_type = kinds[g % kinds.length];
    /* The key is whatever actually collided, which is what the SQL returns:
       the barcode for a barcode match, the SKU for a SKU match, and the
       lower-cased name for either name match. This used to alternate between
       barcode and name irrespective of the match type, which produced
       "similar name" groups identified by a barcode — a shape the backend
       cannot return, so the header read as broken against real data only. */
    const match_key =
      match_type === "shared barcode" ? base.barcode
      : match_type === "shared SKU"   ? base.sku
      : base.name.toLowerCase().trim();
    return {
      match_type,
      match_key,
      confidence: 100 - (g % 4) * 12,
      reason: g % 3 === 0 ? "Same barcode on more than one product." : null,
      products: Array.from({ length: 2 + (g % 2) }, (_, i) => ({
        ...base,
        product_id: `${base.product_id}_dup${g}_${i}`,
        name: i === 0 ? base.name : `${base.name} (${i === 1 ? "old" : "copy"})`,
        total_stock: i === 0 ? base.stock_qty : 0,
        price_minor: base.price_minor + i * 5,
        is_active: i < 2,
      })),
    };
  }),
  admin_list_tax_rules: [],
  /* A single owner and a single role made two states unreachable in QA: the
     role picker had nothing to pick (the owner option is filtered out for a
     manager, leaving an empty list) and no inactive row existed to prove the
     muted styling. A real shop rota is a handful of cashiers, a manager, an
     accountant and someone who has left. Roles are the four the schema
     actually seeds — inventing a fifth would make the picker testable against
     a set the product does not have. */
  admin_list_roles: [
    { role_id: "role_owner", name: "owner" },
    { role_id: "role_manager", name: "manager" },
    { role_id: "role_accountant", name: "accountant" },
    { role_id: "role_cashier", name: "cashier" },
  ],
  admin_list_users_all: [
    { user_id: "usr_renihal", display_name: "Renihal", username: "renihal", role_id: "role_owner", role_name: "owner", is_active: true },
    { user_id: "usr_ahmed", display_name: "Ahmed Salman", username: "ahmed.s", role_id: "role_manager", role_name: "manager", is_active: true },
    { user_id: "usr_fatima", display_name: "Fatima Abdulla", username: "fatima.a", role_id: "role_accountant", role_name: "accountant", is_active: true },
    { user_id: "usr_rahul", display_name: "Rahul Menon", username: "rahul.m", role_id: "role_cashier", role_name: "cashier", is_active: true },
    { user_id: "usr_jomon", display_name: "Jomon Thomas", username: "jomon.t", role_id: "role_cashier", role_name: "cashier", is_active: true },
    { user_id: "usr_sameer", display_name: "Sameer Khan", username: "sameer.k", role_id: "role_cashier", role_name: "cashier", is_active: true },
    { user_id: "usr_leena", display_name: "Leena Varghese", username: "leena.v", role_id: "role_cashier", role_name: "cashier", is_active: false },
  ],
  ai_list_actions: [
    { action_id: "01KZ6M2G4BWD", session_user_id: "usr_renihal", branch_id: "br_amwaj",
      tool_name: "bulk_price_adjust", preview_text: "Raise the selling price of 412 products by 4.6% to restore margin after the supplier cost increase.",
      status: "prepared", prepared_at: new Date(Date.now() - 9 * 60_000).toISOString(),
      confirmed_at: null, executed_at: null,
      expires_at: new Date(Date.now() + 11 * 60_000).toISOString(),
      result_json: null, error_message: null },
    { action_id: "01KZ6M2G4BWE", session_user_id: "usr_renihal", branch_id: "br_amwaj",
      tool_name: "shift_close", preview_text: "Close the shift opened by Renihal on 4 Aug with 0 transactions.",
      status: "prepared", prepared_at: new Date(Date.now() - 25 * 60_000).toISOString(),
      confirmed_at: null, executed_at: null,
      expires_at: new Date(Date.now() + 3 * 60_000).toISOString(),
      result_json: null, error_message: null },
    { action_id: "01KZ6M2G4BWF", session_user_id: "usr_renihal", branch_id: "br_amwaj",
      tool_name: "product_update", preview_text: "Update the cost of Barbican Malt 330ml to BHD 0.195.",
      status: "executed", prepared_at: new Date(Date.now() - 3 * 3_600_000).toISOString(),
      confirmed_at: new Date(Date.now() - 3 * 3_600_000).toISOString(),
      executed_at: new Date(Date.now() - 3 * 3_600_000).toISOString(),
      expires_at: new Date(Date.now() - 2 * 3_600_000).toISOString(),
      result_json: '{"description":"Cost updated"}', error_message: null },
    { action_id: "01KZ6M2G4BWG", session_user_id: "usr_renihal", branch_id: "br_amwaj",
      tool_name: "create_products", preview_text: "Create 3 products from the delivery note photo.",
      status: "cancelled", prepared_at: new Date(Date.now() - 26 * 3_600_000).toISOString(),
      confirmed_at: null, executed_at: null,
      expires_at: new Date(Date.now() - 25 * 3_600_000).toISOString(),
      result_json: null, error_message: null },
    { action_id: "01KZ6M2G4BWH", session_user_id: "usr_renihal", branch_id: "br_amwaj",
      tool_name: "stock_adjust", preview_text: "Adjust stock of Lipton Yellow Label 100s to 42 units.",
      status: "expired", prepared_at: new Date(Date.now() - 50 * 3_600_000).toISOString(),
      confirmed_at: null, executed_at: null,
      expires_at: new Date(Date.now() - 49 * 3_600_000).toISOString(),
      result_json: null, error_message: "Stock level changed while the action was pending." },
  ],
  ai_undo_availability: {
    undo_id: "undo_01", action_id: "01KZ6M2G4BWF", entity_type: "product", entity_id: "prd_3",
    status: "available", available: true,
    created_at: new Date(Date.now() - 3 * 3_600_000).toISOString(), undone_at: null,
  },
  audit_log_list: [
    { audit_log_id: "au_1", event_type: "ai_action_executed", entity_type: "product",
      entity_id: "prd_3", actor_user_id: "usr_renihal",
      created_at: new Date(Date.now() - 3 * 3_600_000).toISOString() },
    { audit_log_id: "au_2", event_type: "po_received", entity_type: "purchase_order",
      entity_id: "po_41", actor_user_id: "usr_renihal",
      created_at: new Date(Date.now() - 26 * 3_600_000).toISOString() },
    { audit_log_id: "au_3", event_type: "shift_closed", entity_type: "shift",
      entity_id: "sh_88", actor_user_id: null,
      created_at: new Date(Date.now() - 30 * 3_600_000).toISOString() },
  ],
  sync_conflicts_list: [
    { conflict_id: "cf_1", conflict_type: "duplicate_barcode", table_name: "products",
      entity_id: "prd_2", severity: "warning",
      title: "Two products share barcode 6280987654321",
      detail: "This device and the hub each have a product with the same barcode.",
      status: "open", created_at: new Date(Date.now() - 5 * 3_600_000).toISOString() },
    { conflict_id: "cf_2", conflict_type: "stale_write", table_name: "stock_levels",
      entity_id: "prd_5", severity: "critical",
      title: "Stock level rejected by the hub",
      detail: "A newer value exists on the hub for Basmati Rice 5kg.",
      status: "open", created_at: new Date(Date.now() - 2 * 3_600_000).toISOString() },
  ],
  sync_stock_drift_report: [],
  hub_truth_compare: { differences: [], checked_at: new Date().toISOString() },
  ai_load_history: [],
  /* The proactive detector's output, which returned [] here — so thirteen
     detection types that run every five minutes against the real database
     were unreachable in QA, and the panel that shows them could never be
     designed against anything. These are the shapes proactive.rs emits, with
     the severities it assigns. */
  admin_get_alerts: [
    { alert_id: "alr_margin", branch_id: BRANCH_ID, alert_type: "margin_erosion", severity: "critical",
      title: "Margin fell on 14 products",
      description: "Supplier cost rose but the selling price did not follow. Worst case is now 2.1% margin.",
      detail_json: null, detected_at: new Date(Date.now() - 12 * 60_000).toISOString(),
      dismissed_at: null, dismissed_by_user_id: null, created_at: new Date(Date.now() - 12 * 60_000).toISOString() },
    { alert_id: "alr_cash", branch_id: BRANCH_ID, alert_type: "cash_discrepancy", severity: "critical",
      title: "Drawer short BHD 4.250 on yesterday's close",
      description: "Counted cash did not match expected takings for the shift closed by Ahmed Salman.",
      detail_json: null, detected_at: new Date(Date.now() - 3 * 3_600_000).toISOString(),
      dismissed_at: null, dismissed_by_user_id: null, created_at: new Date(Date.now() - 3 * 3_600_000).toISOString() },
    { alert_id: "alr_dead", branch_id: BRANCH_ID, alert_type: "dead_stock", severity: "warning",
      title: "31 products have not sold in 90 days",
      description: "BHD 412.500 of stock is sitting still. Consider clearance or delisting.",
      detail_json: null, detected_at: new Date(Date.now() - 40 * 60_000).toISOString(),
      dismissed_at: null, dismissed_by_user_id: null, created_at: new Date(Date.now() - 40 * 60_000).toISOString() },
    { alert_id: "alr_expiry", branch_id: BRANCH_ID, alert_type: "near_expiry", severity: "warning",
      title: "6 lots expire within 7 days",
      description: "Sell or discount these first — FEFO order is in Inventory.",
      detail_json: null, detected_at: new Date(Date.now() - 55 * 60_000).toISOString(),
      dismissed_at: null, dismissed_by_user_id: null, created_at: new Date(Date.now() - 55 * 60_000).toISOString() },
    /* Dismissed, so it must NOT appear on Today. Without one of these the
       dismissal filter is never exercised. */
    { alert_id: "alr_refund", branch_id: BRANCH_ID, alert_type: "refund_spike", severity: "warning",
      title: "Refunds up 3x this week",
      description: "Nine refunds against a four-week average of three.",
      detail_json: null, detected_at: new Date(Date.now() - 26 * 3_600_000).toISOString(),
      dismissed_at: new Date(Date.now() - 20 * 3_600_000).toISOString(),
      dismissed_by_user_id: "usr_renihal", created_at: new Date(Date.now() - 26 * 3_600_000).toISOString() },
  ],
  admin_get_feature_toggles: { zanshop_enabled: false },
  admin_get_ai_enabled: true,
  admin_get_ai_config: { enabled: true },
  payment_confirmations_list: [],
  whatsapp_list_messages: WA_MESSAGES,
  whatsapp_list_contacts: WA_CONTACTS,
  "plugin:window|is_maximized": false,
  "plugin:event|listen": 1,
  "plugin:event|unlisten": null,
  admin_list_categories: CATEGORIES,
  categories_list: CATEGORIES,
  admin_list_users: [
    { user_id: "usr_renihal", display_name: "Renihal", username: "renihal", role_name: "owner",   is_active: true },
    { user_id: "usr_fatima",  display_name: "Fatima",  username: "fatima",  role_name: "cashier", is_active: true },
  ],
  report_today: RANGE_SUMMARY,
  report_date_range: RANGE_SUMMARY,
  report_sales_list: { items: [], total: 0, offset: 0, limit: 50 },
  report_top_products: [],
  report_by_cashier: [],
  report_tax_by_day: [],
  report_eod_cashup: null,
  reports_config_load: { show_tax: true, show_margin: true },
  sync_status: SYNC_STATUS,
  sync_get_status: SYNC_STATUS,
  wa_status: WHATSAPP_STATUS,
  whatsapp_status: WHATSAPP_STATUS,
  whatsapp_get_status: WHATSAPP_STATUS,
  admin_run_diagnostics: { ok: true, note: "All checks passed" },
  system_health_check: HEALTH_REPORT,
  admin_system_health_report: HEALTH_REPORT,
  system_health_report: HEALTH_REPORT,
  admin_system_health: HEALTH_REPORT,
  ghost_summary: { pending: 0, found: 0, not_found: 0 },
  inventory_get_levels: PRODUCTS.map(p => ({
    product_id: p.product_id,
    product_name: p.name,
    sku: p.sku,
    category_name: p.category_name,
    quantity_on_hand: String(p.stock_qty),
    reorder_point: 10,
    is_low_stock: p.stock_qty > 0 && p.stock_qty <= 10,
    is_out_of_stock: p.stock_qty === 0,
    track_inventory: true,
  })),
  inventory_get_low_stock: [],
  // StockLevelPage — shape traced from src-tauri/src/inventory/stock_repo.rs.
  // Without this the paged Inventory screen received null and threw on `.items`.
  inventory_get_levels_paged: {
    items: PRODUCTS.map(p => ({
      product_id: p.product_id,
      product_name: p.name,
      sku: p.sku,
      category_name: p.category_name,
      quantity_on_hand: String(p.stock_qty),
      reorder_point: 10,
      is_low_stock: p.stock_qty > 0 && p.stock_qty <= 10,
      is_out_of_stock: p.stock_qty === 0,
      track_inventory: true,
    })),
    total: PRODUCTS.length,
    offset: 0,
    limit: 100,
  },
  supplier_list: [
    { supplier_id: "sup_1", name: "Al Jazira Trading", phone: "+973 1722 0000", email: "orders@aljazira.bh",
      contact_name: "Ahmed", address: "Manama", notes: null, is_active: true, product_count: 42, open_po_count: 1 },
    { supplier_id: "sup_2", name: "Bahrain Fresh Foods", phone: "+973 1755 1111", email: null,
      contact_name: "Layla", address: "Sitra", notes: null, is_active: true, product_count: 18, open_po_count: 0 },
  ],
  po_get: {
    order: {
      po_id: "po_42", supplier_id: "sup_1", supplier_name: "Al Jazira Trading",
      status: "partial", expected_date: new Date(Date.now() + 86_400_000).toISOString(),
      received_date: null, notes: "Fortnightly dairy and beverage order.",
      line_count: 3, ordered_total_minor: 1_240_500, received_total_minor: 402_000,
      updated_at: new Date().toISOString(),
    },
    lines: [
      { po_line_id: "l1", product_id: "prd_1", product_name: "Almarai Fresh Milk 1L",
        ordered_qty: 120, received_qty: 120, unit_cost_minor: 420 },
      { po_line_id: "l2", product_id: "prd_2", product_name: "Lipton Yellow Label 100s",
        ordered_qty: 48, received_qty: 20, unit_cost_minor: 1100 },
      { po_line_id: "l3", product_id: "prd_5", product_name: "Basmati Rice 5kg",
        ordered_qty: 30, received_qty: 0, unit_cost_minor: 2400 },
    ],
  },
  po_list: [
    { po_id: "po_42", po_number: "PO-0042", supplier_id: "sup_1", supplier_name: "Al Jazira Trading",
      status: "ordered", expected_date: new Date(Date.now() + 86_400_000).toISOString(),
      line_count: 42, ordered_total_minor: 1_240_500, received_total_minor: 0,
      updated_at: new Date().toISOString(), received_date: null, notes: null },
    { po_id: "po_41", po_number: "PO-0041", supplier_id: "sup_2", supplier_name: "Bahrain Fresh Foods",
      status: "received", expected_date: new Date(Date.now() - 86_400_000).toISOString(),
      line_count: 12, ordered_total_minor: 318_000, received_total_minor: 318_000,
      updated_at: new Date().toISOString(), received_date: new Date().toISOString(), notes: null },
  ],
  report_margin: {
    from_date: new Date(Date.now() - 30 * 86_400_000).toISOString().slice(0, 10),
    to_date: new Date().toISOString().slice(0, 10),
    transaction_count: 0, revenue_minor: 0, cogs_minor: 0,
    gross_margin_minor: 0, margin_basis_points: 3540, unknown_cost_line_count: 0,
  },
  report_product_margin: [],
  // An open shift, so the POS renders its selling screen instead of the
  // open-shift modal. Without this the POS was uncapturable in visual QA.
  shift_get_active: {
    shift_id: "shf_mock_01",
    branch_id: BRANCH_ID,
    device_id: DEVICE_ID,
    cashier_user_id: "usr_renihal",
    cashier_name: "Renihal",
    opened_at: new Date(Date.now() - 3 * 3600_000).toISOString(),
    closed_at: null,
    opening_cash_minor: 20_000,
    status: "open",
  },
  auth_login: MOCK_SESSION,

  // The till reads these on mount and stores whatever comes back. Falling
  // through to the null default made PosPage throw on `auto_print_receipt`
  // before it rendered, so the selling screen could never be captured.
  business_flags_load: {
    allow_negative_stock: false,
    require_discount_reason: true,
    cashier_can_discount: false,
    auto_print_receipt: false,
  },
  whatsapp_orders_get_enabled: false,
  // There is no OS keyboard to raise in a browser; reporting the touch
  // keyboard keeps the button's success path exercisable in visual QA.
  system_keyboard_open: "touch",

  // DEVICE_ID is the terminal the mock session is running on, so the first row
  // exercises the "this is the terminal you are using" case that has no remove
  // button, and the second exercises the case that does.
  device_list: [
    { device_id: DEVICE_ID, device_code: "POS01", device_name: "Front counter", is_active: true, created_at: new Date(Date.now() - 90 * 86_400_000).toISOString() },
    { device_id: "dev_mock_02", device_code: "POS02", device_name: "Counter two", is_active: true, created_at: new Date(Date.now() - 30 * 86_400_000).toISOString() },
    { device_id: "dev_mock_03", device_code: "POS03", device_name: "Old kiosk", is_active: false, created_at: new Date(Date.now() - 400 * 86_400_000).toISOString() },
  ],
  device_delete: null,

  /* Six of the ten slots filled: the till has to look right with gaps, which is
     the normal state — a shop rarely uses all ten. */
  quick_pos_load: PRODUCTS.slice(0, 10).map((p, i) => (
    i < 6
      ? { slot: i, product_id: p.product_id, name: p.name, price_minor: p.price_minor, image_path: p.image_path ?? null }
      : { slot: i, product_id: null, name: null, price_minor: null, image_path: null }
  )),
  quick_pos_save: PRODUCTS.slice(0, 10).map((p, i) => (
    i < 6
      ? { slot: i, product_id: p.product_id, name: p.name, price_minor: p.price_minor, image_path: p.image_path ?? null }
      : { slot: i, product_id: null, name: null, price_minor: null, image_path: null }
  )),

  rider_list: [
    { rider_id: "rdr_1", branch_id: BRANCH_ID, name: "Sami Al Hawaj", phone: "+97336112233", notes: "Bike 4471 · evenings", is_active: true, created_at: new Date(Date.now() - 40 * 86_400_000).toISOString() },
    { rider_id: "rdr_2", branch_id: BRANCH_ID, name: "Rashid Bu Hassan", phone: "+97336445566", notes: null, is_active: true, created_at: new Date(Date.now() - 12 * 86_400_000).toISOString() },
    { rider_id: "rdr_3", branch_id: BRANCH_ID, name: "Jassim Al Doseri", phone: "+97336778899", notes: "Left the roster", is_active: false, created_at: new Date(Date.now() - 200 * 86_400_000).toISOString() },
  ],

  // Boot path — App gates the whole UI on these.
  startup_health_check: [
    { component: "database", status: "ok", message: "Database ready" },
    { component: "sync",     status: "ok", message: "Standalone mode" },
    { component: "sidecar",  status: "ok", message: "WhatsApp sidecar idle" },
  ],
  check_for_updates: null,
  check_critical_update: null,
  log_diagnostic: null,
};
