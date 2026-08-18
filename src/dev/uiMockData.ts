/**
 * Canned data for the dev-only IPC mock.
 *
 * Split from uiMock.ts, which holds the dispatcher that serves these. The
 * fixtures are the part that grows — every command the UI learns to call needs
 * a shape here — and they were pushing the file past the 500-line rule.
 *
 * Shapes are traced from the Rust commands they stand in for. Where a real
 * command filters on its arguments the dispatcher honours that too, so search,
 * empty and no-results states stay reachable in QA rather than simulated.
 */

export const BRANCH_ID = "br_amwaj";
export const DEVICE_ID = "dev_mock_01";

export const MOCK_SESSION = {
  user_id: "usr_renihal",
  branch_id: BRANCH_ID,
  display_name: "Renihal",
  username: "renihal",
  role_id: "role_owner",
  role_name: "owner",
  session_token: "mock-token",
  session_expires_at: new Date(Date.now() + 86_400_000).toISOString(),
};

export const APP_CONFIG = {
  setup_complete: true,
  database_path: "mock.db",
  hub_mode: "standalone",
  hub_url: null,
  branch_id: BRANCH_ID,
  device_id: DEVICE_ID,
  branch_name: "Amwaj AlDair",
  branch_code: "AMW",
  currency: "BHD",
  currency_exponent: 3,
  address: "Building 210, Road 2803, Al Dair",
  phone: "+973 3305 0666",
  receipt_header: "Amwaj AlDair",
  receipt_footer: "Thank you",
  tax_number: null,
};

/* Names here are as long as the ones in the real catalogue, and the list is as
   long as a real page. Both matter for visual QA: short names let every column
   fit at any width, so a table that pushes its action buttons off the screen
   under real data looked perfectly fine against this mock. */
export const PRODUCTS = [
  { barcode: "6280123456781", name: "Almarai Fresh Milk 1L",   category: "Dairy",     cost: 420,  price: 650,  stock: 48 },
  { barcode: "6280987654321", name: "Lipton Yellow Label 100s", category: "Beverages", cost: 1100, price: 1650, stock: 6 },
  { barcode: "6280555123451", name: "Barbican Malt 330ml",      category: "Beverages", cost: 180,  price: 300,  stock: 0 },
  { barcode: "6280444333221", name: "Al Ain Water 1.5L",        category: "Beverages", cost: 90,   price: 150,  stock: 240 },
  { barcode: "6280777888991", name: "Basmati Rice 5kg",         category: "Grocery",   cost: 2400, price: 3250, stock: 32 },
  { barcode: "6280222111334", name: "Nadec Laban 1L",           category: "Dairy",     cost: 380,  price: 550,  stock: 12 },
  { barcode: "6280310045512", name: "Green foods Green Peas SR2 Salted (50gm)(Mixto)", category: "Food", cost: 150, price: 150, stock: 0 },
  { barcode: "6280310045529", name: "Green foods Chilly Corn Fryms (21gm)(Mixto)",     category: "Food", cost: 75,  price: 75,  stock: 0 },
  { barcode: "6280310045536", name: "Green foods Bombay Mixture (275Gm)(Mixto)",       category: "Food", cost: 450, price: 450, stock: 0 },
  { barcode: "6280310045543", name: "Green foods Dal Masala SR2 (50gm)(Mixto)",        category: "Food", cost: 150, price: 150, stock: 0 },
  { barcode: "6280310045550", name: "Green foods Gulab Jamun (82gm)(Mixto)",           category: "Food", cost: 153, price: 170, stock: 0 },
  { barcode: "6280310045567", name: "Green foods Dry Fig -Pak 200gm",                  category: "Food", cost: 450, price: 450, stock: 0 },
  { barcode: "6280310045574", name: "Green foods Almond 130gm",                        category: "Food", cost: 1200, price: 1200, stock: 0 },
  { barcode: "6280310045581", name: "Green foods Cashew 130gm",                        category: "Food", cost: 800, price: 800, stock: 0 },
  { barcode: "6280310045598", name: "Al Karamah Sunflower Cooking Oil 1.8 Litre Bottle", category: "Grocery", cost: 1450, price: 1750, stock: 24 },
  { barcode: "6280310045604", name: "Bahrain Fresh Farms Free Range Large Eggs (30 pack)", category: "Dairy", cost: 1800, price: 2100, stock: 9 },
].map((p, i) => ({
  product_id: `prd_${i + 1}`,
  category_id: `cat_${p.category.toLowerCase()}`,
  category_name: p.category,
  name: p.name,
  sku: p.barcode,
  barcode: p.barcode,
  barcodes: [p.barcode],
  track_inventory: true,
  allow_decimal_quantity: false,
  is_active: true,
  tax_rule_id: null,
  tax_rule_name: null,
  price_minor: p.price,
  reorder_point: 10,
  image_path: null,
  // Extra fields consumed by list/table views.
  cost_minor: p.cost,
  stock_qty: p.stock,
  updated_at: new Date(Date.now() - i * 3_600_000).toISOString(),
}));

/** adminListProducts returns a page, not a bare array. */
export const PRODUCT_PAGE = { items: PRODUCTS, total: PRODUCTS.length };

export interface MockCartLine { cart_line_id: string; quantity: string; unit_price_minor: number; line_total_minor: number; voided: boolean; }
export interface MockCart { lines: MockCartLine[]; [key: string]: unknown; }

/** One cart line, shaped like the CartLine the Rust command returns. */
export const cartLine = (product: (typeof PRODUCTS)[number], quantity: number, index: number) => ({
  cart_line_id: `cln_mock_${index + 1}`,
  product_id: product.product_id,
  product_name: product.name,
  sku: product.sku,
  barcode: product.barcode,
  image_path: product.image_path,
  quantity: String(quantity),
  unit_price_minor: product.price_minor,
  line_discount_minor: 0,
  line_discount_reason: null,
  tax_rule_id: "tax_none",
  tax_rate_basis_points: 0,
  tax_inclusive: true,
  tax_amount_minor: 0,
  line_total_minor: product.price_minor * quantity,
  note: null,
  voided: false,
});

export const CATEGORIES = ["Dairy", "Beverages", "Grocery", "Bakery", "Household"].map((name, i) => ({
  category_id: `cat_${name.toLowerCase()}`,
  name,
  product_count: i === 1 ? 3 : 1,
  sort_order: i,
}));

export const TODAY = new Date().toISOString().slice(0, 10);

export const RANGE_SUMMARY = {
  from_date: TODAY, to_date: TODAY,
  transaction_count: 0, gross_total_minor: 0, discount_total_minor: 0,
  tax_total_minor: 0, net_total_minor: 0, cash_total_minor: 0, card_total_minor: 0,
  refund_count: 0, refund_total_minor: 0,
  pending_delivery_count: 0, pending_delivery_minor: 0,
};

export const SYNC_STATUS = {
  online: true,
  hub_configured: true,
  mode: "hub",
  hub_url: "https://hub.local",
  pending_events: 11,
  last_successful_sync_at: new Date(Date.now() - 25 * 60_000).toISOString(),
  days_since_last_sync: 0,
  last_error: null,
  device_id: DEVICE_ID,
  consecutive_errors: 0,
};

export const WHATSAPP_STATUS = { connected: false };

/** Loyalty balances span held / zero / no-phone so every directory and
    programme state is reachable in QA. Names are fictional. */
export const CUSTOMERS = [
  { name: "Fatima Al Sayed",  phone: "+973 3600 1122", email: "fatima@example.test", pts: 340, notes: "Prefers evening delivery." },
  { name: "Hassan Al Mannai", phone: "+973 3600 4455", email: null,                  pts: 185, notes: null },
  { name: "Layla Bu Ali",     phone: null,             email: "layla@example.test",  pts: 120, notes: "Bulk buyer — rice, oil." },
  { name: "Omar Al Dosari",   phone: "+973 3600 7788", email: null,                  pts: 40,  notes: null },
  { name: "Noor Al Khalifa",  phone: "+973 3600 9911", email: "noor@example.test",   pts: 0,   notes: null },
  { name: "Yusuf Al Ansari",  phone: "+973 3600 2233", email: null,                  pts: 0,   notes: "Account opened at the till." },
].map((c, i) => ({
  customer_id: `cus_${i + 1}`,
  branch_id: BRANCH_ID,
  name: c.name,
  phone: c.phone,
  email: c.email,
  loyalty_points: c.pts,
  created_at: new Date(Date.now() - (i + 1) * 26 * 864e5).toISOString(),
  notes: c.notes,
}));

export const HEALTH_REPORT = {
  summary: {
    ok: true, db_integrity: "ok", migration_count: 42,
    pending_sync_rows: 11, stuck_sync_rows: 0, device_count: 2,
    hub_mode: "hub", checked_at: new Date().toISOString(),
  },
  findings: [
    { code: "wa_disconnected", severity: "warning", area: "whatsapp",
      title: "WhatsApp is disconnected", detail: "14 messages are queued on this device.",
      fix_action: null },
  ],
  devices: [
    { device_id: DEVICE_ID, label: "Front till", role: "terminal", status: "online",
      ip: "192.168.1.20", last_seen: new Date().toISOString() },
  ],
  tables: [],
};

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
  admin_find_duplicate_products: [],
  admin_list_tax_rules: [],
  admin_list_roles: [{ role_id: "role_owner", name: "owner" }],
  admin_list_users_all: [
    { user_id: "usr_renihal", display_name: "Renihal", username: "renihal", role_name: "owner", is_active: true },
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
  admin_get_alerts: [],
  admin_get_feature_toggles: { zanshop_enabled: false },
  admin_get_ai_enabled: true,
  admin_get_ai_config: { enabled: true },
  payment_confirmations_list: [],
  whatsapp_list_messages: [],
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
