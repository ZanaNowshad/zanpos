// ─── Domain types mirroring Rust structs ──────────────────────────────────────

export interface Product {
  product_id: string;
  category_id: string;
  name: string;
  sku: string | null;
  barcode: string | null;
  description: string | null;
  track_inventory: boolean;
  allow_decimal_quantity: boolean;
  is_active: boolean;
  tax_rule_id: string | null;
  cost_minor: number | null;
  currency: string;
  version: number;
}

export interface ProductWithPrice {
  product_id: string;
  category_id: string;
  name: string;
  sku: string | null;
  barcode: string | null;
  description: string | null;
  track_inventory: boolean;
  allow_decimal_quantity: boolean;
  is_active: boolean;
  tax_rule_id: string | null;
  cost_minor: number | null;
  currency: string;
  version: number;
  reorder_point: number;
  image_path: string | null;
  price_minor: number;
  tax_rate_basis_points: number;
  tax_inclusive: boolean;
  category_name: string;
  quantity_on_hand: string | null;
}

// ── Inventory types ───────────────────────────────────────────────────────────

export interface LowStockAlert {
  product_id: string;
  product_name: string;
  quantity_on_hand: string;
  reorder_point: number;
}

export interface StockLevel {
  product_id: string;
  product_name: string;
  sku: string | null;
  category_name: string;
  quantity_on_hand: string;
  reorder_point: number;
  is_low_stock: boolean;
  is_out_of_stock: boolean;
  track_inventory: boolean;
}

export interface StockMovementRow {
  movement_id: string;
  movement_type: string;
  quantity_delta: string;
  quantity_after: string;
  reference_type: string | null;
  reference_id: string | null;
  notes: string | null;
  created_at: string;
}

export interface CartLine {
  cart_line_id: string;
  product_id: string | null;
  product_name: string;
  sku: string | null;
  barcode: string | null;
  quantity: string;
  unit_price_minor: number;
  line_discount_minor: number;
  line_discount_reason: string | null;
  tax_rule_id: string;
  tax_rate_basis_points: number;
  tax_inclusive: boolean;
  tax_amount_minor: number;
  line_total_minor: number;
  note: string | null;
  voided: boolean;
}

export interface Cart {
  cart_id: string;
  branch_id: string;
  device_id: string;
  shift_id: string;
  cashier_user_id: string;
  lines: CartLine[];
  bill_discount_minor: number;
  bill_discount_reason: string | null;
}

export interface PaymentInput {
  method: "cash" | "card" | "wallet" | "other";
  amount_minor: number;
  tendered_minor?: number;
  external_reference?: string;
}

export interface PaymentSummary {
  method: string;
  amount_minor: number;
  change_minor: number | null;
}

export interface SaleItemSummary {
  product_name: string;
  quantity: string;
  unit_price_minor: number;
  line_total_minor: number;
  tax_amount_minor: number;
}

export interface SaleResult {
  sale_id: string;
  receipt_number: string;
  net_total_minor: number;
  tax_total_minor: number;
  discount_total_minor: number;
  currency: string;
  payments: PaymentSummary[];
  items: SaleItemSummary[];
  cashier_name: string;
  branch_name: string;
  sold_at: string;
  business_date: string;
  created_offline: boolean;
  low_stock_alerts: LowStockAlert[];
  delivery?: DeliveryRow;
}

export interface SyncStatus {
  online: boolean;
  /** True when hub is configured (hub mode or terminal with hub_url + token). */
  hub_configured: boolean;
  mode: string;
  hub_url: string | null;
  pending_events: number;
  last_successful_sync_at: string | null;
  /** Days elapsed since last successful sync. null if never synced. */
  days_since_last_sync: number | null;
  last_error: string | null;
  device_id: string;
  /** Count of consecutive sync cycles ending in error. Resets on success. */
  consecutive_failure_count?: number;
}

// ─── Phase 1: Auth / Shift / Refund / Report types ───────────────────────────

export interface UserSummary {
  user_id: string;
  display_name: string;
  username: string;
  role_name: string;
}

export interface SessionUser {
  user_id: string;
  display_name: string;
  username: string;
  role_id: string;
  role_name: string;
}

export interface Shift {
  shift_id: string;
  branch_id: string;
  device_id: string;
  cashier_user_id: string;
  cashier_name: string;
  opened_at: string;
  closed_at: string | null;
  opening_cash_minor: number;
  status: string;
}

export interface SaleItemForRefund {
  sale_item_id: string;
  product_name_snapshot: string;
  quantity: string;
  unit_price_minor: number;
  line_total_minor: number;
}

export interface SaleForRefund {
  sale_id: string;
  receipt_number: string;
  net_total_minor: number;
  currency: string;
  sold_at: string;
  cashier_name: string;
  status: string;
  origin_device_id: string;
  items: SaleItemForRefund[];
}

export interface RefundItemInput {
  sale_item_id: string;
  product_name_snapshot: string;
  quantity: string;
  unit_price_minor: number;
  refund_amount_minor: number;
}

export interface RefundResult {
  refund_id: string;
  refund_receipt_number: string;
  refund_total_minor: number;
  currency: string;
  created_at: string;
}

export interface HeldCartSummary {
  held_cart_id: string;
  note: string | null;
  held_at: string;
  line_count: number;
  estimated_total_minor: number;
}

export interface TodaySummary {
  business_date: string;
  transaction_count: number;
  gross_total_minor: number;
  discount_total_minor: number;
  tax_total_minor: number;
  net_total_minor: number;
  cash_total_minor: number;
  card_total_minor: number;
  refund_count: number;
  refund_total_minor: number;
  pending_delivery_count: number;
  pending_delivery_minor: number;
}

// ─── Phase 2: AI Provider config ─────────────────────────────────────────────

export interface ProviderConfig {
  provider: string; // "anthropic" | "openai" | "gemini" | ""
  anthropic_key_set: boolean;
  openai_base_url: string;
  openai_key_set: boolean;
  openai_model: string;
  gemini_key_set: boolean;
  gemini_model: string;
}

export interface ModelInfo {
  id: string;
}

export interface ValidateProviderResult {
  success: boolean;
  models: ModelInfo[];
  error: string | null;
}

// ─── Phase 2: AI Admin types ──────────────────────────────────────────────────

export interface ChatMessage {
  role: "user" | "assistant";
  content: string;
}

export interface ToolPreviewField {
  label: string;
  value: string;
}

export interface ToolPreview {
  tool_name: string;
  description: string;
  fields: ToolPreviewField[];
}

export interface AiChatInput {
  history: ChatMessage[];
  message: string;
  user_id: string;
  branch_id: string;
  currency_exponent: number;
}

export type AiChatResponse =
  | { type: "message"; content: string }
  | {
      type: "pending_action";
      action_id: string;
      tool_name: string;
      preview: ToolPreview;
      expires_at: string;
      assistant_text: string;
    }
  | { type: "no_api_key" };

export interface ExecuteActionInput {
  action_id: string;
  user_id: string;
  history: ChatMessage[];
  assistant_text: string;
  currency_exponent: number;
}

export interface ExecuteActionResult {
  action_id: string;
  undo_id: string | null;
  followup: string;
}

export interface UndoActionResult {
  undo_id: string;
  followup: string;
}

// ─── AI streaming & history ───────────────────────────────────────────────────

export interface AiChatMessage {
  id: number;
  session_id: string;
  branch_id: string;
  user_id: string;
  role: string;
  content: string;
  message_type: string;
  created_at: string;
}

export type StreamEvent =
  | { type: "token"; text: string }
  | { type: "tool_start"; name: string }
  | { type: "tool_done"; name: string }
  | {
      type: "mutation_pending";
      action_id: string;
      tool_name: string;
      preview: ToolPreview;
      expires_at: string;
      assistant_text: string;
    }
  | { type: "done" }
  | { type: "error"; message: string };

// ─── Phase 7: Enhanced report types ──────────────────────────────────────────

export interface RangeSummary {
  from_date: string;
  to_date: string;
  transaction_count: number;
  gross_total_minor: number;
  discount_total_minor: number;
  tax_total_minor: number;
  net_total_minor: number;
  cash_total_minor: number;
  card_total_minor: number;
  refund_count: number;
  refund_total_minor: number;
  pending_delivery_count: number;
  pending_delivery_minor: number;
}

export interface TopProduct {
  product_name: string;
  total_quantity: string;
  revenue_minor: number;
  transaction_count: number;
}

export interface SaleListRow {
  sale_id: string;
  receipt_number: string;
  sold_at: string;
  cashier_name: string;
  net_total_minor: number;
  discount_total_minor: number;
  status: string;
  payment_methods: string;
}

/** Paginated wrapper returned by report_sales_list (F-BIZ-002 / F-INT-001).
 *  `total` is the full count of matching rows ignoring limit/offset,
 *  allowing callers to detect truncation and implement pagination. */
export interface SaleListPage {
  items: SaleListRow[];
  total: number;
  offset: number;
  limit: number;
}

// ─── Phase 6: Back-office admin types ────────────────────────────────────────

export interface AdminProduct {
  product_id: string;
  category_id: string;
  category_name: string;
  name: string;
  sku: string | null;
  barcode: string | null;
  track_inventory: boolean;
  allow_decimal_quantity: boolean;
  is_active: boolean;
  tax_rule_id: string | null;
  tax_rule_name: string | null;
  price_minor: number;
  reorder_point: number;
  image_path: string | null;
  barcodes?: string[];
}

// ─── Cash events (Paid-In / Paid-Out) ────────────────────────────────────────

export interface CashEventRow {
  cash_event_id:      string;
  shift_id:           string;
  event_type:         "paid_in" | "paid_out" | "safe_drop";
  amount_minor:       number;
  note:               string | null;
  created_by_user_id: string;
  created_at:         string;
}

export interface NoSaleRow {
  no_sale_id:     string;
  shift_id:       string;
  actor_user_id:  string;
  note:           string | null;
  created_at:     string;
}

export interface CashDrawerSummary {
  opening_minor:     number;
  cash_sales_minor:  number;
  cash_refunds_minor: number;
  paid_in_minor:     number;
  paid_out_minor:    number;
  safe_drop_minor:   number;
  expected_minor:    number;
  counted_minor:     number | null;
  variance_minor:    number | null;
  events:            CashEventRow[];
  pending_delivery_cash_minor: number;
}

// ─── Product barcodes ─────────────────────────────────────────────────────────

export interface ProductBarcodeRow {
  barcode_id:  string;
  product_id:  string;
  barcode:     string;
  created_at:  string;
}

// ─── Phase 10b: Customers ─────────────────────────────────────────────────────

export interface CustomerRow {
  customer_id:    string;
  branch_id:      string;
  name:           string;
  phone:          string | null;
  email:          string | null;
  loyalty_points: number;
  created_at:     string;
  notes:          string | null;
}

// ─── Phase 10b: Devices ───────────────────────────────────────────────────────

export interface DeviceRow {
  device_id:   string;
  device_code: string;
  device_name: string;
  is_active:   boolean;
  created_at:  string;
}

// ─── Phase 10b: Thermal config ────────────────────────────────────────────────

export interface ThermalConfig {
  enabled: boolean;
  port:    string;
  baud:    string;
}

export interface CategoryRow {
  category_id: string;
  name: string;
  sort_order: number;
  is_active: boolean;
  parent_category_id?: string;
}

export interface TaxRuleRow {
  tax_rule_id: string;
  name: string;
  rate_basis_points: number;
  inclusive: boolean;
  is_active: boolean;
}

export interface AdminUserRow {
  user_id: string;
  display_name: string;
  username: string;
  role_id: string;
  role_name: string;
  is_active: boolean;
  last_login_at: string | null;
}

export interface RoleRow {
  role_id: string;
  name: string;
}

// ─── Hub (LAN sync) ────────────────────────────────────────────────────────────

export interface HubStatus {
  mode: "hub" | "terminal" | "standalone";
  running: boolean;
  port: number;
  lan_ips: string[];
  token: string | null;
  hub_url: string | null;
  last_error: string | null;
  terminals: HubTerminalSeen[];
}

export interface HubTerminalSeen { device_id: string; ip: string; last_seen: string }

export interface HubTestResult { ok: boolean; store_name: string | null; error: string | null }

// ─── App configuration (loaded from DB at startup) ────────────────────────────

export interface AppConfig {
  setup_complete: boolean;
  /** "hub" | "terminal" | "standalone" — drives Settings/Hub UI + SyncChip. */
  hub_mode: "hub" | "terminal" | "standalone";
  hub_url: string | null;
  branch_id: string;
  device_id: string;
  branch_name: string;
  branch_code: string;
  currency: string;
  currency_exponent: number;
  address: string | null;
  phone: string | null;
  receipt_header: string | null;
  receipt_footer: string | null;
  tax_number: string | null;
  cr_number: string | null;
  whatsapp_benefit_number: string | null;
  owner_user_id: string | null;
}

export interface BranchSettings {
  branch_id: string;
  name: string;
  branch_code: string;
  currency: string;
  timezone: string;
  address: string | null;
  phone: string | null;
  receipt_header: string | null;
  receipt_footer: string | null;
  tax_number: string | null;
  cr_number: string | null;
}

export interface BusinessFlags {
  /** Allow a sale to finalize even when stock quantity would go below zero. */
  allow_negative_stock: boolean;
  /** When true, a non-empty reason is required for every discount applied. */
  require_discount_reason: boolean;
  /** When true, cashiers (not just managers/owners) may apply discounts. */
  cashier_can_discount: boolean;
  /** When true, the thermal receipt prints automatically after every sale. */
  auto_print_receipt: boolean;
}

// ─── Cashier report ───────────────────────────────────────────────────────────

export interface CashierSummaryRow {
  cashier_user_id: string;
  cashier_name: string;
  transaction_count: number;
  net_total_minor: number;
  cash_total_minor: number;
  card_total_minor: number;
  discount_total_minor: number;
  refund_count: number;
  refund_total_minor: number;
}

// ─── EOD cash-up report ───────────────────────────────────────────────────────

export interface EodShiftRow {
  shift_id: string;
  cashier_name: string;
  opened_at: string;
  closed_at: string | null;
  opening_minor: number;
  cash_sales_minor: number;
  safe_drop_minor: number;
  paid_in_minor: number;
  paid_out_minor: number;
  expected_minor: number;
  counted_minor: number | null;
  variance_minor: number | null;
  net_sales_minor: number;
}

export interface EodCashupReport {
  date: string;
  shifts: EodShiftRow[];
  total_net_minor: number;
  total_cash_minor: number;
  total_counted_minor: number | null;
  total_variance_minor: number | null;
}

// ─── Bulk stock-take ──────────────────────────────────────────────────────────

export interface BulkStockTakeResult {
  updated: number;
  errors: string[];
}

// ─── Sync queue ───────────────────────────────────────────────────────────────

export interface SyncQueueItem {
  sync_event_id: string;
  entity_type: string;
  entity_id: string;
  operation: string;
  status: string;
  attempt_count: number;
  last_attempt_at: string | null;
  last_error: string | null;
  created_at: string;
}

export interface SyncTableStats {
  table: string;
  pending: number;
  failed: number;
  max_attempts: number;
  attempts_dist: string;
}

export interface SyncDiagTable {
  table: string;
  pending: number;
  stuck: number;
  max_attempts: number;
  avg_attempts: number;
}

export interface SyncDiagnostics {
  hub_configured: boolean;
  pending_events: number;
  stuck_events: number;
  last_sync_at: string | null;
  last_error: string | null;
  online: boolean;
  tables: SyncDiagTable[];
}

// ─── Device constants — populated from DB at startup before any UI renders ─────
// F-CRIT-03: Default to empty string ("") instead of seed IDs so any pre-init
// use fails FK validation at the DB layer rather than silently writing fake IDs.
// App.tsx calls init() before any authenticated UI is shown; if init() hasn't run,
// DB calls will return "no active branch/device" errors instead of corrupting data.
export const DEVICE = {
  device_id:         "",   // empty = not initialized; seed ID removed
  branch_id:         "",   // empty = not initialized; seed ID removed
  branch_name:       "Loading…",
  branch_code:       "",
  currency:          "BHD",
  currency_exponent: 3,

  /** Called once by App.tsx after loading config from DB. */
  init(cfg: AppConfig) {
    this.device_id         = cfg.device_id;
    this.branch_id         = cfg.branch_id;
    this.branch_name       = cfg.branch_name;
    this.branch_code       = cfg.branch_code;
    this.currency          = cfg.currency;
    this.currency_exponent = cfg.currency_exponent;
  },
};

// ─── Delivery module types ─────────────────────────────────────────────────────

export interface DeliveryInput {
  customer_id?: string;
  customer_name?: string;
  contact_number: string;       // E.164: +97333050666
  house_number?: string;
  area?: string;
  address_text: string;         // required
  delivery_note?: string;
  delivery_staff_name?: string;
  expected_payment_method: string; // cash | card | wallet
}

export interface DeliveryRow {
  delivery_id: string;
  sale_id: string;
  receipt_number: string;
  customer_id?: string;
  customer_name?: string;
  contact_number: string;
  house_number?: string;
  area?: string;
  address_text: string;
  delivery_note?: string;
  delivery_staff_name?: string;
  expected_payment_method: string;
  payment_status: "unpaid" | "paid" | "cancelled";
  delivery_status: "pending" | "dispatched" | "out_for_delivery" | "delivered" | "cancelled";
  amount_minor: number;
  currency: string;
  paid_confirmed_by_user_id?: string;
  paid_confirmed_at?: string;
  payment_reference?: string;
  payment_note?: string;
  created_by_user_id: string;
  branch_id: string;
  device_id: string;
  created_at: string;
  updated_at: string;
}

export interface DeliveryListFilter {
  branch_id?: string;
  payment_status?: string;
  delivery_status?: string;
  date_from?: string;
  date_to?: string;
  staff_name?: string;
  contact_search?: string;
  limit?: number;
  offset?: number;
}

export interface ConfirmDeliveryPaymentInput {
  delivery_id: string;
  confirmed_by_user_id: string;
  payment_reference?: string;
  payment_note?: string;
}

export interface UpdateDeliveryStatusInput {
  delivery_id: string;
  delivery_status: string;
  actor_user_id: string;
}

export interface CancelDeliveryInput {
  delivery_id: string;
  actor_user_id: string;
}

export interface RevertPaymentInput {
  delivery_id: string;
  actor_user_id: string;
  reason?: string;
}

// ── WhatsApp ──────────────────────────────────────────────────────────────────

export interface WhatsAppStatus {
  connected: boolean;
  qr?: string; // base64 PNG data URL when QR is pending
}

export interface ImportContactsResult {
  imported: number;
  skipped: number;
  total: number;
}

export interface SendDeliveryInput {
  to: string;
  receipt_number: string;
  net_total_minor: number;
  currency_exponent: number;
  address_text: string;
  house_number?: string;
  area?: string;
  /** Pre-built message from the frontend template editor. Overrides Rust builder. */
  message_override?: string;
}

// ── Migration Agent ───────────────────────────────────────────────────────────

export interface ColumnSchema {
  name: string;
  samples: string[];
}

export interface SheetSchema {
  name: string;
  columns: ColumnSchema[];
  row_count: number;
}

export interface FileSchema {
  file_path: string;
  file_type: string;
  sheets: SheetSchema[];
}

export interface ColumnMapping {
  source_col: string;
  target_col: string;
  transform: string;
  notes?: string;
}

export interface SheetMapping {
  source_sheet: string;
  target_table: string;
  column_mappings: ColumnMapping[];
}

export interface MappingConfig {
  sheet_mappings: SheetMapping[];
}

export type MigrationProgress =
  | { type: "started"; total_sheets: number }
  | { type: "sheet_start"; sheet: string; target: string; total_rows: number }
  | { type: "sheet_progress"; sheet: string; done: number; total: number }
  | { type: "sheet_done"; sheet: string; target: string; inserted: number; skipped: number }
  | { type: "done"; message: string }
  | { type: "error"; message: string };

// ── Migration extended tools ──────────────────────────────────────────────────

export interface ConnectTestResult {
  success: boolean;
  message: string;
  server_version: string | null;
  db_type: string;
}

export interface RemoteTableInfo {
  name: string;
  row_count: number;
  columns: string[];
}

export interface QueryResult {
  columns: string[];
  rows: string[][];
  row_count: number;
  truncated: boolean;
}

export interface ProcessInfo {
  name: string;
  pid: string;
  memory_kb: string;
}

export interface DbFileInfo {
  path: string;
  size_bytes: number;
  file_type: string;
  modified: string;
}

export interface DecompressResult {
  extracted_files: string[];
  db_files: string[];
  dest_dir: string;
  error: string | null;
}

export interface ZanposStats {
  products: number;
  categories: number;
  customers: number;
  sales: number;
  sale_items: number;
  stock_levels: number;
}

export interface RollbackResult {
  deleted_counts: [string, number][];
  total_deleted: number;
}

// ── Ghost Barcode types ───────────────────────────────────────────────────────

export interface GhostBarcode {
  id: string;
  barcode: string;
  scan_count: number;
  first_seen_at: number;
  last_seen_at: number;
  status: 'pending' | 'found' | 'not_found' | 'dismissed';
  product_name: string | null;
  brand: string | null;
  category: string | null;
  image_url: string | null;
}

export interface GhostSummary {
  pending: number;
  found: number;
  not_found: number;
}

export interface ProductPrefill {
  name: string;
  barcode: string;
  brand: string | null;
  category: string | null;
  image_url: string | null;
}

export interface ResolveResult {
  resolved: number;
  not_found: number;
}
