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
  /** True when Supabase URL + service key are persisted. */
  supabase_configured: boolean;
  pending_events: number;
  last_successful_sync_at: string | null;
  /** Days elapsed since last successful sync. null if never synced. */
  days_since_last_sync: number | null;
  last_error: string | null;
  device_id: string;
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
  provider: string; // "anthropic" | "openai" | ""
  anthropic_key_set: boolean;
  openai_base_url: string;
  openai_key_set: boolean;
  openai_model: string;
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
}

export interface TaxRuleRow {
  tax_rule_id: string;
  name: string;
  rate_basis_points: number;
  inclusive: boolean;
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

// ─── Phase 3: Supabase Sync ───────────────────────────────────────────────────

export interface SupabaseStatus {
  configured: boolean;
}

// ─── App configuration (loaded from DB at startup) ────────────────────────────

export interface AppConfig {
  setup_complete: boolean;
  /** True when Supabase URL + service key are persisted in app_config. */
  supabase_configured: boolean;
  /** ISO timestamp when the 7-day cloud-setup grace period expires. null = Supabase configured. */
  cloud_grace_deadline: string | null;
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
  whatsapp_benefit_number: string | null;
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

// ─── Device constants — mutable, populated from DB at startup ─────────────────
// These defaults are used only as fallback; app_config_load() overwrites them.
export const DEVICE = {
  device_id:         "01JDEVICE0000000000000001",
  branch_id:         "01JBRANCH0000000000000001",
  branch_name:       "Main Branch",
  branch_code:       "MAIN",
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
  delivery_status: "pending" | "out_for_delivery" | "delivered" | "cancelled";
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

// ── WhatsApp ──────────────────────────────────────────────────────────────────

export interface WhatsAppStatus {
  connected: boolean;
  qr?: string; // base64 PNG data URL when QR is pending
}

export interface SendDeliveryInput {
  to: string;
  receipt_number: string;
  net_total_minor: number;
  currency_exponent: number;
  address_text: string;
  house_number?: string;
  area?: string;
}
