import { invoke, Channel } from "@tauri-apps/api/core";
import type {
  AdminProduct,
  AdminUserRow,
  AiChatInput,
  AiChatResponse,
  AppConfig,
  BranchSettings,
  BusinessFlags,
  BulkStockTakeResult,
  Cart,
  CashDrawerSummary,
  CashEventRow,
  CashierSummaryRow,
  CategoryRow,
  CustomerRow,
  DeviceRow,
  EodCashupReport,
  ExecuteActionInput,
  ExecuteActionResult,
  HeldCartSummary,
  PaymentInput,
  ProductBarcodeRow,
  ProductWithPrice,
  ProviderConfig,
  RangeSummary,
  RefundItemInput,
  RefundResult,
  RoleRow,
  SaleForRefund,
  SaleListPage,
  SaleResult,
  SessionUser,
  Shift,
  StockLevel,
  StockMovementRow,
  SyncQueueItem,
  SyncDiagnostics,
  SyncTableStats,
  SyncStatus,
  TaxRuleRow,
  ThermalConfig,
  TodaySummary,
  TopProduct,
  UndoActionResult,
  AiChatMessage,
  StreamEvent,
  UserSummary,
  ValidateProviderResult,
  NoSaleRow,
  DeliveryRow,
  DeliveryListFilter,
  ConfirmDeliveryPaymentInput,
  UpdateDeliveryStatusInput,
  CancelDeliveryInput,
  RevertPaymentInput,
  WhatsAppStatus,
  ImportContactsResult,
  SendDeliveryInput,
  FileSchema,
  GhostBarcode,
  GhostSummary,
  MappingConfig,
  MigrationProgress,
  ConnectTestResult,
  RemoteTableInfo,
  QueryResult,
  ProcessInfo,
  DbFileInfo,
  DecompressResult,
  ZanposStats,
  RollbackResult,
  ChatMessage,
  ProductPrefill,
  ResolveResult,
  HubStatus,
  HubTestResult,
} from "../types";

// ─── Setup & Settings commands ────────────────────────────────────────────────

export const appConfigLoad = (): Promise<AppConfig> =>
  invoke("app_config_load");

export const setupWizardComplete = (input: {
  store_name: string;
  store_address?: string;
  store_phone?: string;
  receipt_header?: string;
  receipt_footer?: string;
  tax_number?: string;
  cr_number?: string;
  currency: string;
  timezone: string;
  owner_display_name: string;
  owner_username: string;
  owner_pin: string;
}): Promise<AppConfig> =>
  invoke("setup_wizard_complete", { input });

export interface PullSummary {
  ok: boolean;
  rows_pulled: number;
  error: string | null;
}

/** Blocking initial catalog pull called after joinStore succeeds. */
export const setupPullCatalog = (): Promise<PullSummary> =>
  invoke("setup_pull_catalog");

export const settingsGetBranch = (actorUserId: string): Promise<BranchSettings> =>
  invoke("settings_get_branch", { actorUserId });

export const settingsUpdateBranch = (input: {
  name: string;
  address?: string;
  phone?: string;
  receipt_header?: string;
  receipt_footer?: string;
  tax_number?: string;
  cr_number?: string;
  timezone: string;
  actor_user_id: string;
}): Promise<BranchSettings> =>
  invoke("settings_update_branch", { input });

// ─── Business flags ────────────────────────────────────────────────────────────

export const businessFlagsLoad = (): Promise<BusinessFlags> =>
  invoke("business_flags_load");

export const businessFlagsSave = (
  flags: BusinessFlags,
  actor_user_id: string,
): Promise<void> =>
  invoke("business_flags_save", { input: { flags, actor_user_id } });

// ─── Auth commands ────────────────────────────────────────────────────────────

export const authListUsers = (actorUserId?: string): Promise<UserSummary[]> =>
  invoke("auth_list_users", { actorUserId });

export const authLoginPin = (username: string, pin: string): Promise<SessionUser> =>
  invoke("auth_login_pin", { input: { username, pin } });

// ─── Shift commands ───────────────────────────────────────────────────────────

export const shiftGetActive = (device_id: string, actorUserId = ""): Promise<Shift | null> =>
  invoke("shift_get_active", { deviceId: device_id, actorUserId });

export const shiftOpen = (
  branch_id: string,
  device_id: string,
  cashier_user_id: string,
  opening_cash_minor: number
): Promise<Shift> =>
  invoke("shift_open", { input: { branch_id, device_id, cashier_user_id, opening_cash_minor } });

export const shiftClose = (
  shift_id: string,
  actor_user_id: string,
  counted_cash_minor?: number,
  notes?: string
): Promise<Shift> =>
  invoke("shift_close", { input: { shift_id, actor_user_id, counted_cash_minor, notes } });

// ─── Product commands ─────────────────────────────────────────────────────────

export const productSearch = (actorUserId: string, query: string): Promise<ProductWithPrice[]> =>
  invoke("product_search", { actorUserId, query });

export const productGetByBarcode = (actorUserId: string, barcode: string): Promise<ProductWithPrice | null> =>
  invoke("product_get_by_barcode", { actorUserId, barcode });

export const productListAll = (
  actorUserId: string,
  afterId?: string | null,
  pageSize?: number,
): Promise<ProductWithPrice[]> =>
  invoke("product_list_all", {
    actorUserId,
    afterId: afterId ?? null,
    pageSize: pageSize ?? 50,
  });

// ─── POS commands ─────────────────────────────────────────────────────────────

export const posStartCart = (
  branch_id: string,
  device_id: string,
  shift_id: string,
  cashier_user_id: string
): Promise<{ cart: Cart }> =>
  invoke("pos_start_cart", { input: { branch_id, device_id, shift_id, cashier_user_id } });

export const posAddItem = (cart: Cart, product_id: string, quantity?: string): Promise<Cart> =>
  invoke("pos_add_item", { input: { cart, product_id, quantity } });

export const posAddItemByBarcode = (cart: Cart, barcode: string): Promise<Cart> =>
  invoke("pos_add_item_by_barcode", { input: { cart, barcode } });

export const posUpdateQuantity = (cart: Cart, cart_line_id: string, quantity: string): Promise<Cart> =>
  invoke("pos_update_quantity", { input: { cart, cart_line_id, quantity } });

export const posSetLinePrice = (cart: Cart, cart_line_id: string, price_minor: number, authorized_by_user_id: string): Promise<Cart> =>
  invoke("pos_set_line_price", { input: { cart, cart_line_id, price_minor, authorized_by_user_id } });

export const posRemoveLine = (cart: Cart, cart_line_id: string): Promise<Cart> =>
  invoke("pos_remove_line", { input: { cart, cart_line_id } });

export const posFinalizeSale = (
  cart: Cart,
  payments: PaymentInput[],
  idempotency_key?: string,
  customer_id?: string,
  delivery?: import("../types").DeliveryInput,
): Promise<SaleResult> =>
  invoke("pos_finalize_sale", { input: { cart, payments, idempotency_key, customer_id, delivery } });

export const posApplyBillDiscount = (cart: Cart, discount_minor: number, reason: string, authorized_by_user_id: string): Promise<Cart> =>
  invoke("pos_apply_bill_discount", { input: { cart, discount_minor, reason, authorized_by_user_id } });

export const posApplyLineDiscount = (cart: Cart, cart_line_id: string, discount_minor: number, reason: string, authorized_by_user_id: string): Promise<Cart> =>
  invoke("pos_apply_line_discount", { input: { cart, cart_line_id, discount_minor, reason, authorized_by_user_id } });

export const posSetLineNote = (cart: Cart, cart_line_id: string, note: string | null): Promise<Cart> =>
  invoke("pos_set_line_note", { input: { cart, cart_line_id, note } });

export const posAddCustomItem = (
  cart: Cart,
  name: string,
  price_minor: number,
  quantity: string,
): Promise<Cart> =>
  invoke("pos_add_custom_item", { input: { cart, name, price_minor, quantity } });

/** Load a completed sale back into a Cart for editing.
 *  Returns a pre-populated Cart with each line item as a custom item
 *  preserving original prices, quantities, and discounts. */
export const posLoadSaleForEdit = (
  receipt_number: string,
  branch_id: string,
  device_id: string,
  shift_id: string,
  cashier_user_id: string,
): Promise<Cart> =>
  invoke("pos_load_sale_for_edit", {
    input: { receipt_number, branch_id, device_id, shift_id, cashier_user_id },
  });

export interface VoidSaleResult {
  voided: boolean;
  stock_warning: string | null;
}

export const posVoidSale = (sale_id: string, voided_by_user_id: string): Promise<VoidSaleResult> =>
  invoke("pos_void_sale", { saleId: sale_id, voidedByUserId: voided_by_user_id });

export const posRecordVoid = (
  cart_id: string,
  device_id: string,
  cashier_user_id: string,
  line_count: number,
  net_total_minor: number,
): Promise<void> =>
  invoke("pos_record_void", { cartId: cart_id, deviceId: device_id, cashierUserId: cashier_user_id, lineCount: line_count, netTotalMinor: net_total_minor });

export const posCartSummary = (cart: Cart): Promise<{
  gross_total_minor: number;
  tax_total_minor: number;
  discount_total_minor: number;
  net_total_minor: number;
  line_count: number;
}> =>
  invoke("pos_cart_summary", { cart });

// ─── Held cart commands ───────────────────────────────────────────────────────

export const heldCartSave = (actorUserId: string, cart: Cart, note?: string): Promise<HeldCartSummary> =>
  invoke("held_cart_save", { input: { cart, note, actor_user_id: actorUserId } });

export const heldCartList = (actorUserId: string, device_id: string): Promise<HeldCartSummary[]> =>
  invoke("held_cart_list", { actorUserId, deviceId: device_id });

export const heldCartResume = (actorUserId: string, held_cart_id: string, shift_id: string): Promise<Cart> =>
  invoke("held_cart_resume", { input: { held_cart_id, shift_id, actor_user_id: actorUserId } });

export const heldCartDelete = (actorUserId: string, held_cart_id: string): Promise<void> =>
  invoke("held_cart_delete", { actorUserId, heldCartId: held_cart_id });

// ─── Refund commands ──────────────────────────────────────────────────────────

export const refundGetSale = (receipt_number: string, requesting_user_id: string): Promise<SaleForRefund> =>
  invoke("refund_get_sale", { receiptNumber: receipt_number, requestingUserId: requesting_user_id });

export const refundCreate = (
  original_sale_id: string,
  items: RefundItemInput[],
  reason: string,
  created_by_user_id: string,
  return_reason_code?: string,
  manager_override_token?: string,
): Promise<RefundResult> =>
  invoke("refund_create", {
    input: {
      original_sale_id,
      items,
      reason,
      return_reason_code: return_reason_code ?? null,
      created_by_user_id,
      manager_override_token: manager_override_token ?? null,
    },
  });

export const receiptReprint = (receipt_number: string, requesting_user_id: string): Promise<SaleResult> =>
  invoke("receipt_reprint", { receiptNumber: receipt_number, requestingUserId: requesting_user_id });

// ─── Report commands ──────────────────────────────────────────────────────────

export const reportToday = (actor_user_id: string, branch_id: string, business_date: string): Promise<TodaySummary> =>
  invoke("report_today", { actorUserId: actor_user_id, branchId: branch_id, businessDate: business_date });

export const reportDateRange = (actor_user_id: string, branch_id: string, from_date: string, to_date: string): Promise<RangeSummary> =>
  invoke("report_date_range", { actorUserId: actor_user_id, branchId: branch_id, fromDate: from_date, toDate: to_date });

export const reportTopProducts = (actor_user_id: string, branch_id: string, from_date: string, to_date: string): Promise<TopProduct[]> =>
  invoke("report_top_products", { actorUserId: actor_user_id, branchId: branch_id, fromDate: from_date, toDate: to_date });

/** Fetch paginated sales list. Returns total count alongside items so callers
 *  can detect truncation and implement paging (F-BIZ-002 / F-INT-001).
 *  Defaults: limit=200, offset=0 (backwards-compatible). */
export const reportSalesList = (
  actor_user_id: string,
  branch_id: string,
  from_date: string,
  to_date: string,
  offset?: number,
  limit?: number,
): Promise<SaleListPage> =>
  invoke("report_sales_list", {
    actorUserId: actor_user_id,
    branchId: branch_id,
    fromDate: from_date,
    toDate: to_date,
    offset: offset ?? 0,
    limit: limit ?? 200,
  });

export const dbIntegrityCheck = (actorUserId: string): Promise<string> =>
  invoke("db_integrity_check", { actorUserId });

// ─── Sync commands ────────────────────────────────────────────────────────────

export const syncStatus = (actorUserId: string): Promise<SyncStatus> => invoke("sync_status", { actorUserId });
export const syncTriggerNow = (actorUserId: string): Promise<string> => invoke("sync_trigger_now", { actorUserId });

/// Recovery: re-enqueue the full catalog and push immediately. For terminals whose
/// data never reached the cloud (outbox looks empty but cloud is empty).
export const syncForceFullResync = (actorUserId: string): Promise<string> =>
  invoke("sync_force_full_resync", { actorUserId });

// ─── Hub (LAN sync) ───────────────────────────────────────────────────────────

export const hubStatus = (actorUserId: string): Promise<HubStatus> =>
  invoke("hub_status", { actorUserId });
export const hubEnable = (actorUserId: string, port?: number): Promise<HubStatus> =>
  invoke("hub_enable", { actorUserId, port: port ?? null });
export const hubRegenerateToken = (actorUserId: string): Promise<HubStatus> =>
  invoke("hub_regenerate_token", { actorUserId });
export const hubTestConnection = (url: string, token: string): Promise<HubTestResult> =>
  invoke("hub_test_connection", { url, token });
export const hubJoin = (input: { hub_url: string; token: string; device_name: string; device_code: string })
  : Promise<AppConfig> => invoke("hub_join", { input });
export const hubConnectExisting = (actorUserId: string, hubUrl: string, token: string): Promise<HubStatus> =>
  invoke("hub_connect_existing", { actorUserId, hubUrl, token });
export const hubSetUrl = (actorUserId: string, hubUrl: string): Promise<HubStatus> =>
  invoke("hub_set_url", { actorUserId, hubUrl });

// ─── AI Admin — provider management ──────────────────────────────────────────

export const adminGetProviderConfig = (actorUserId: string): Promise<ProviderConfig> =>
  invoke("admin_get_provider_config", { actorUserId });

export const adminSetAnthropic = (actorUserId: string, apiKey: string): Promise<void> =>
  invoke("admin_set_anthropic", { actorUserId, apiKey });

export const adminValidateOpenai = (
  actorUserId: string,
  baseUrl: string,
  apiKey: string
): Promise<ValidateProviderResult> =>
  invoke("admin_validate_openai", { actorUserId, baseUrl, apiKey });

export const adminSetOpenai = (
  actorUserId: string,
  baseUrl: string,
  apiKey: string,
  model: string
): Promise<void> =>
  invoke("admin_set_openai", { actorUserId, baseUrl, apiKey, model });

// Google Gemini (OpenAI-compatible endpoint; base URL is fixed server-side)
export const adminValidateGemini = (actorUserId: string, apiKey: string): Promise<ValidateProviderResult> =>
  invoke("admin_validate_gemini", { actorUserId, apiKey });

export const adminSetGemini = (
  actorUserId: string,
  apiKey: string,
  model: string
): Promise<void> =>
  invoke("admin_set_gemini", { actorUserId, apiKey, model });

// Legacy — kept for compat
export const adminGetApiKeySet = (actorUserId: string): Promise<boolean> =>
  invoke("admin_get_api_key_set", { actorUserId });

export const adminSetApiKey = (actorUserId: string, key: string): Promise<void> =>
  invoke("admin_set_api_key", { actorUserId, key });

export const aiChat = (input: AiChatInput): Promise<AiChatResponse> =>
  invoke("ai_chat", { input });

export const aiExecuteAction = (input: ExecuteActionInput): Promise<ExecuteActionResult> =>
  invoke("ai_execute_action", { input });

export const aiCancelAction = (actionId: string, actorUserId: string): Promise<void> =>
  invoke("ai_cancel_action", { actionId, actorUserId });

export const aiUndoAction = (
  undoId: string,
  userId: string,
  currencyExponent: number,
  actorUserId: string
): Promise<UndoActionResult> =>
  invoke("ai_undo_action", { undoId, userId, currencyExponent, actorUserId });

export const aiChatStream = (
  input: AiChatInput,
  onEvent: Channel<StreamEvent>
): Promise<void> => invoke("ai_chat_stream", { input, onEvent });

export const aiSaveMessage = (
  sessionId: string,
  branchId: string,
  userId: string,
  role: string,
  content: string,
  messageType: string
): Promise<number> =>
  invoke("ai_save_message", { sessionId, branchId, userId, role, content, messageType });

export const aiLoadHistory = (
  branchId: string,
  userId: string
): Promise<AiChatMessage[]> =>
  invoke("ai_load_history", { branchId, userId });

export const aiClearHistory = (
  branchId: string,
  userId: string
): Promise<void> => invoke("ai_clear_history", { branchId, userId });

// ─── Back-office admin commands ───────────────────────────────────────────────

export interface AdminProductPage {
  items: AdminProduct[];
  total: number;
  offset: number;
  limit: number;
}

export const adminListProducts = (
  actor_user_id: string,
  opts?: { search?: string; categoryId?: string; offset?: number; limit?: number }
): Promise<AdminProductPage> =>
  invoke("admin_list_products", {
    actorUserId: actor_user_id,
    search: opts?.search ?? null,
    categoryId: opts?.categoryId ?? null,
    offset: opts?.offset ?? 0,
    limit: opts?.limit ?? 100,
  });

export const adminCreateProduct = (input: {
  category_id: string; name: string; sku?: string; barcode?: string;
  tax_rule_id?: string; price_minor: number;
  track_inventory: boolean; allow_decimal_quantity: boolean;
  reorder_point: number; created_by_user_id: string;
  image_path?: string;
}): Promise<AdminProduct> =>
  invoke("admin_create_product", { input });

export const adminUpdateProduct = (input: {
  product_id: string; category_id: string; name: string; sku?: string; barcode?: string;
  tax_rule_id?: string; price_minor: number;
  track_inventory: boolean; allow_decimal_quantity: boolean;
  reorder_point: number; is_active: boolean; updated_by_user_id: string;
  image_path?: string;
}): Promise<AdminProduct> =>
  invoke("admin_update_product", { input });

export const adminListCategories = (actor_user_id: string): Promise<CategoryRow[]> =>
  invoke("admin_list_categories", { actorUserId: actor_user_id });

export const adminListTaxRules = (actor_user_id: string): Promise<TaxRuleRow[]> =>
  invoke("admin_list_tax_rules", { actorUserId: actor_user_id });

export const adminSaveTaxRule = (input: {
  tax_rule_id?: string;
  name: string;
  rate_basis_points: number;
  inclusive: boolean;
  is_active: boolean;
  actor_user_id: string;
}): Promise<TaxRuleRow> =>
  invoke("admin_save_tax_rule", { input });

export const adminDeleteTaxRule = (tax_rule_id: string, actor_user_id: string): Promise<void> =>
  invoke("admin_delete_tax_rule", { input: { tax_rule_id, actor_user_id } });

export const adminSaveCategory = (input: {
  category_id?: string; name: string; sort_order: number; is_active: boolean; parent_category_id?: string; actor_user_id: string;
}): Promise<CategoryRow> =>
  invoke("admin_save_category", { input });

// Bulk CSV import
export interface BulkCategoryRow { name: string; sort_order?: number; }
export interface BulkProductRow {
  name: string; category_name: string; price: string;
  sku?: string; barcode?: string;
  /** Pipe-separated barcodes, e.g. "12345|67890". Takes priority over `barcode`. */
  barcodes?: string;
  track_inventory?: boolean; tax_rule_name?: string;
}
export interface BulkRowError { row: number; name: string; reason: string; }
export interface BulkImportResult { inserted: number; skipped: number; errors: BulkRowError[]; }

export const adminBulkImportCategories = (rows: BulkCategoryRow[], actorUserId: string): Promise<BulkImportResult> =>
  invoke("admin_bulk_import_categories", { rows, actorUserId });

export const adminBulkImportProducts = (rows: BulkProductRow[], actorUserId: string): Promise<BulkImportResult> =>
  invoke("admin_bulk_import_products", { rows, actorUserId });

export const adminListUsersAll = (actor_user_id: string): Promise<AdminUserRow[]> =>
  invoke("admin_list_users_all", { actorUserId: actor_user_id });

export const adminListRoles = (actor_user_id: string): Promise<RoleRow[]> =>
  invoke("admin_list_roles", { actorUserId: actor_user_id });

export const adminCreateUser = (input: {
  display_name: string; username: string; pin: string; role_id: string; actor_user_id: string;
}): Promise<AdminUserRow> =>
  invoke("admin_create_user", { input });

export const adminUpdateUser = (input: {
  user_id: string; display_name: string; pin?: string; role_id: string; is_active: boolean; actor_user_id: string;
}): Promise<AdminUserRow> =>
  invoke("admin_update_user", { input });

// ─── Inventory commands ───────────────────────────────────────────────────────

export interface StockLevelPage {
  items: StockLevel[];
  total: number;
  offset: number;
  limit: number;
}

export const inventoryGetLevels = (actor_user_id: string): Promise<StockLevel[]> =>
  invoke("inventory_get_levels", { actorUserId: actor_user_id });

export const inventoryGetLevelsPaged = (
  actor_user_id: string,
  search: string,
  offset: number,
  limit: number,
): Promise<StockLevelPage> =>
  invoke("inventory_get_levels_paged", { actorUserId: actor_user_id, search: search || null, offset, limit });

export const inventoryGetLowStock = (actor_user_id: string): Promise<StockLevel[]> =>
  invoke("inventory_get_low_stock", { actorUserId: actor_user_id });

export const inventoryGetMovements = (actor_user_id: string, productId: string): Promise<StockMovementRow[]> =>
  invoke("inventory_get_movements", { actorUserId: actor_user_id, productId });

export const inventoryReceiveStock = (
  product_id: string,
  quantity: string,
  notes: string | undefined,
  received_by_user_id: string,
): Promise<StockLevel> =>
  invoke("inventory_receive_stock", {
    input: { product_id, quantity, notes, received_by_user_id },
  });

// ─── Phase 10a commands ───────────────────────────────────────────────────────

export const appConfigGetTimeout = (): Promise<number> =>
  invoke("app_config_get_timeout");

export const appConfigSetTimeout = (minutes: number, actorUserId: string): Promise<void> =>
  invoke("app_config_set_timeout", { minutes, actorUserId });

// ─── Reports device-scope (Phase E) ───────────────────────────────────────────
export type ReportsConfig = {
  device_scope: "origin" | "all";
  device_count: number;
  local_device_id: string;
};

export const reportsConfigLoad = (actorUserId: string): Promise<ReportsConfig> =>
  invoke("reports_config_load", { actorUserId });

export const reportsConfigSave = (
  device_scope: "origin" | "all",
  actor_user_id: string
): Promise<void> =>
  invoke("reports_config_save", { input: { device_scope, actor_user_id } });

export const dbBackup = (destPath: string, actorUserId: string): Promise<string> =>
  invoke("db_backup", { destPath, actorUserId });

export const reportTaxByDay = (
  actor_user_id: string,
  branch_id: string,
  from_date: string,
  to_date: string,
): Promise<Array<{ day: string; transaction_count: number; tax_minor: number; cumulative_minor: number }>> =>
  invoke("report_tax_by_day", { actorUserId: actor_user_id, branchId: branch_id, fromDate: from_date, toDate: to_date });

export const auditLogList = (
  from: string,
  to: string,
  page: number,
  actorUserId: string,
): Promise<Array<{ audit_log_id: string; event_type: string; entity_type: string; entity_id: string | null; actor_user_id: string | null; created_at: string }>> =>
  invoke("audit_log_list", { from, to, page, actorUserId });

export const auditVerifyChain = (actorUserId: string): Promise<{
  total_rows: number; legacy_rows: number; verified: number;
  broken_hash: number; broken_link: number; ok: boolean;
}> => invoke("audit_verify_chain", { actorUserId });

export const inventoryAdjustStock = (
  product_id: string,
  new_quantity: string,
  notes: string | undefined,
  adjusted_by_user_id: string,
): Promise<StockLevel> =>
  invoke("inventory_adjust_stock", {
    input: { product_id, new_quantity, notes, adjusted_by_user_id },
  });

// ─── Phase 10b — Customers ────────────────────────────────────────────────────

export const customerList = (actorUserId: string, search: string): Promise<CustomerRow[]> =>
  invoke("customer_list", { actorUserId, search });

export const customerCreate = (input: {
  name: string; phone?: string; email?: string; notes?: string; actor_user_id: string;
}): Promise<CustomerRow> =>
  invoke("customer_create", { input });

export const customerUpdate = (input: {
  customer_id: string; name: string; phone?: string; email?: string; notes?: string; actor_user_id: string;
}): Promise<CustomerRow> =>
  invoke("customer_update", { input });

export const customerGet = (actorUserId: string, customerId: string): Promise<CustomerRow> =>
  invoke("customer_get", { actorUserId, customerId });

export const customerAddLoyalty = (actorUserId: string, customerId: string, points: number): Promise<number> =>
  invoke("customer_add_loyalty", { actorUserId, customerId, points });

// ─── Phase 10b — Devices ──────────────────────────────────────────────────────

export const deviceList = (actorUserId: string): Promise<DeviceRow[]> =>
  invoke("device_list", { actorUserId });

export const deviceCreate = (actorUserId: string, input: {
  device_code: string; device_name: string;
}): Promise<DeviceRow> =>
  invoke("device_create", { input, actorUserId });

export const deviceToggleActive = (actorUserId: string, deviceId: string, isActive: boolean): Promise<void> =>
  invoke("device_toggle_active", { deviceId, isActive, actorUserId });

// ─── Phase 10b — Product image picker ────────────────────────────────────────

export const productPickImage = (): Promise<string | null> =>
  invoke("product_pick_image");

// ─── Phase 10b — Auto-updater ─────────────────────────────────────────────────

export const checkForUpdates = (): Promise<string | null> =>
  invoke("check_for_updates");

/// Download + install the available update, then restart. Resolves false if none.
/// On success the app restarts, so the promise typically does not resolve.
export const downloadAndInstallUpdate = (actorUserId: string): Promise<boolean> =>
  invoke("download_and_install_update", { actorUserId });

// ─── Maintenance page gate — verify an owner PIN ──────────────────────────────
export const authVerifyOwnerPin = (pin: string): Promise<boolean> =>
  invoke("auth_verify_owner_pin", { pin });

// ─── Cross-device refund — validate a manager PIN, get override token ──────────
export const authValidateManagerPin = (pin: string): Promise<string> =>
  invoke("auth_validate_manager_pin", { pin });

// ─── Phase 10b — Thermal printer ─────────────────────────────────────────────

export interface PortEntry {
  port: string;
  label: string;
  /** True if this is the OS-default printer (Windows only; always false for serial ports) */
  is_default: boolean;
}

export const thermalListPorts = (): Promise<PortEntry[]> =>
  invoke("thermal_list_ports");

export const thermalGetConfig = (actorUserId: string): Promise<ThermalConfig> =>
  invoke("thermal_get_config", { actorUserId });

export const thermalSetConfig = (actorUserId: string, input: ThermalConfig): Promise<void> =>
  invoke("thermal_set_config", { input, actorUserId });

export const thermalPrintTest = (actorUserId: string): Promise<string> =>
  invoke("thermal_print_test", { actorUserId });

export const printReceiptRaw = (actorUserId: string, storeName: string, lines: string[]): Promise<string> =>
  invoke("print_receipt_raw", { actorUserId, storeName, lines });

/** Open the cash drawer connected to the ESC/POS printer's RJ-11 port.
 *  Returns "opened" on success, "no_printer" if thermal printing is disabled. */
export const openCashDrawer = (actorUserId: string): Promise<string> =>
  invoke("open_cash_drawer", { actorUserId });

// ─── Cash events ──────────────────────────────────────────────────────────────

export const cashEventCreate = (
  shift_id: string,
  event_type: "paid_in" | "paid_out" | "safe_drop",
  amount_minor: number,
  note: string | undefined,
  created_by_user_id: string,
): Promise<CashEventRow> =>
  invoke("cash_event_create", { shiftId: shift_id, eventType: event_type, amountMinor: amount_minor, note, createdByUserId: created_by_user_id });

export const cashEventsList = (actor_user_id: string, shift_id: string): Promise<CashEventRow[]> =>
  invoke("cash_events_list", { actorUserId: actor_user_id, shiftId: shift_id });

export const cashDrawerSummary = (actor_user_id: string, shift_id: string): Promise<CashDrawerSummary> =>
  invoke("cash_drawer_summary", { actorUserId: actor_user_id, shiftId: shift_id });

export const cashXReport = (shift_id: string, actor_user_id: string): Promise<CashDrawerSummary> =>
  invoke("cash_x_report", { shiftId: shift_id, actorUserId: actor_user_id });

export const cashNoSale = (
  shift_id:      string,
  actor_user_id: string,
  note?:         string,
): Promise<NoSaleRow> =>
  invoke("cash_no_sale", { shiftId: shift_id, actorUserId: actor_user_id, note: note ?? null });

// ─── Product barcodes ─────────────────────────────────────────────────────────

export const productBarcodeAdd = (actorUserId: string, product_id: string, barcode: string): Promise<ProductBarcodeRow> =>
  invoke("product_barcode_add", { actorUserId, productId: product_id, barcode });

export const productBarcodeRemove = (actorUserId: string, barcode_id: string): Promise<void> =>
  invoke("product_barcode_remove", { actorUserId, barcodeId: barcode_id });

export const productBarcodesList = (actor_user_id: string, product_id: string): Promise<ProductBarcodeRow[]> =>
  invoke("product_barcodes_list", { actorUserId: actor_user_id, productId: product_id });

// ─── New pilot-hardening commands ─────────────────────────────────────────────

export const reportByCashier = (
  actor_user_id: string,
  branch_id: string,
  from_date: string,
  to_date: string,
): Promise<CashierSummaryRow[]> =>
  invoke("report_by_cashier", { actorUserId: actor_user_id, branchId: branch_id, fromDate: from_date, toDate: to_date });

export const reportEodCashup = (
  actor_user_id: string,
  branch_id: string,
  business_date: string,
): Promise<EodCashupReport> =>
  invoke("report_eod_cashup", { actorUserId: actor_user_id, branchId: branch_id, date: business_date });

export const inventoryBulkStockTake = (
  // FIX: string, not number — JS floats corrupt Decimal arithmetic in Rust backend
  entries: Array<{ product_id: string; new_quantity: string; notes?: string }>,
  actor_user_id: string,
): Promise<BulkStockTakeResult> =>
  invoke("inventory_bulk_stock_take", { entries, actorUserId: actor_user_id });

export const syncQueueList = (actorUserId: string): Promise<SyncQueueItem[]> =>
  invoke("sync_queue_list", { actorUserId });

export const syncQueueRetry = (actorUserId: string, syncEventId: string): Promise<void> =>
  invoke("sync_queue_retry", { id: syncEventId, actorUserId });

export const syncQueueDismiss = (actorUserId: string, syncEventId: string): Promise<void> =>
  invoke("sync_queue_dismiss", { id: syncEventId, actorUserId });

export const syncQueueStats = (actorUserId: string): Promise<SyncTableStats[]> =>
  invoke("sync_queue_stats", { actorUserId });

export const syncDiagnostics = (actorUserId: string): Promise<SyncDiagnostics> =>
  invoke("sync_diagnostics", { actorUserId });

export const syncResetStuck = (actorUserId: string): Promise<string> =>
  invoke("sync_reset_stuck", { actorUserId });

export const syncBulkInitial = (actorUserId: string): Promise<string> =>
  invoke("sync_bulk_initial", { actorUserId });

// ─── Delivery commands ────────────────────────────────────────────────────────

export const deliveryList = (
  filter: DeliveryListFilter,
  actor_user_id: string,
): Promise<DeliveryRow[]> =>
  invoke("delivery_list", { filter, actorUserId: actor_user_id });

export const deliveryGet = (
  delivery_id: string,
  actor_user_id: string,
): Promise<DeliveryRow> =>
  invoke("delivery_get", { deliveryId: delivery_id, actorUserId: actor_user_id });

export const deliveryUpdateStatus = (
  input: UpdateDeliveryStatusInput,
): Promise<DeliveryRow> =>
  invoke("delivery_update_status", { input });

export const deliveryConfirmPayment = (
  input: ConfirmDeliveryPaymentInput,
): Promise<DeliveryRow> =>
  invoke("delivery_confirm_payment", { input });

export const deliveryCancel = (
  input: CancelDeliveryInput,
): Promise<DeliveryRow> =>
  invoke("delivery_cancel", { input });

export const deliveryRevertPayment = (
  input: RevertPaymentInput,
): Promise<DeliveryRow> =>
  invoke("delivery_revert_payment", { input });

export const deliveryRiderSuggestions = (
  branch_id: string,
  actor_user_id: string,
): Promise<string[]> =>
  invoke("delivery_rider_suggestions", { branchId: branch_id, actorUserId: actor_user_id });

// ─── WhatsApp ─────────────────────────────────────────────────────────────────

export function whatsappStatus(actorUserId: string): Promise<WhatsAppStatus> {
  return invoke("whatsapp_status", { actorUserId });
}

export function whatsappSendDelivery(actorUserId: string, input: SendDeliveryInput): Promise<boolean> {
  return invoke("whatsapp_send_delivery", { input, actorUserId });
}

export function whatsappNotifyArrival(actorUserId: string, to: string, receiptNumber: string): Promise<boolean> {
  return invoke("whatsapp_notify_arrival", {
    input: { to, receipt_number: receiptNumber },
    actorUserId,
  });
}

export function whatsappPaymentReminder(
  actorUserId: string,
  to: string,
  receiptNumber: string,
  amountMinor: number,
  currencyExponent: number,
  currency: string,
): Promise<boolean> {
  return invoke("whatsapp_payment_reminder", {
    input: {
      to,
      receipt_number: receiptNumber,
      amount_minor: amountMinor,
      currency_exponent: currencyExponent,
      currency,
    },
    actorUserId,
  });
}

export function whatsappDisconnect(actorUserId: string): Promise<boolean> {
  return invoke("whatsapp_disconnect", { actorUserId });
}

export function whatsappSaveConfig(benefitNumber: string, actorUserId: string): Promise<void> {
  return invoke("whatsapp_save_config", { benefitNumber, actorUserId });
}

export function whatsappImportContacts(actorUserId: string): Promise<ImportContactsResult> {
  return invoke("whatsapp_import_contacts", { actorUserId });
}

export interface ReceiptItemForPdf {
  product_name: string;
  quantity: string;
  unit_price_minor: number;
  line_total_minor: number;
}

export interface PaymentForPdf {
  method: string;
  amount_minor: number;
  /** Change returned to customer for cash payments (mirrors PaymentSummary.change_minor). */
  change_minor?: number | null;
}

export interface WhatsAppReceiptPdfInput {
  to: string;
  receipt_number: string;
  branch_name: string;
  branch_phone?: string;
  cashier_name: string;
  sold_at: string;
  currency: string;
  currency_exponent: number;
  items: ReceiptItemForPdf[];
  net_total_minor: number;
  tax_total_minor: number;
  discount_total_minor: number;
  payments: PaymentForPdf[];
  caption?: string;
  address_text?: string;
  house_number?: string;
  area?: string;
}

/** Send a PDF receipt + caption text as a single WhatsApp document message.
 *  Returns true on confirmed delivery, false if sidecar rejects (caller can fall back). */
export function whatsappSendReceiptPdf(
  actorUserId: string,
  input: WhatsAppReceiptPdfInput,
): Promise<boolean> {
  return invoke("whatsapp_send_receipt_pdf", { input, actorUserId });
}

export function setupSaveBenefitNumber(benefitNumber: string): Promise<void> {
  return invoke("setup_save_benefit_number", { benefitNumber });
}

// ── Migration Agent ───────────────────────────────────────────────────────────

export const migrationInspectFile = (path: string): Promise<FileSchema> =>
  invoke("migration_inspect_file", { path });

export const migrationAiMap = (
  schema: FileSchema,
  currencyExponent: number,
): Promise<MappingConfig> =>
  invoke("migration_ai_map", { schema, currencyExponent });

export const migrationExecute = (
  path: string,
  mapping: MappingConfig,
  currencyExponent: number,
  actorUserId: string,
  onEvent: Channel<MigrationProgress>,
): Promise<void> =>
  invoke("migration_execute", { path, mapping, currencyExponent, actorUserId, onEvent });

// ── Migration extended tools ──────────────────────────────────────────────────

export const migrationConnectTest = (
  dbType: string,
  connStr: string,
): Promise<ConnectTestResult> =>
  invoke("migration_connect_test", { dbType, connStr });

export const migrationListTables = (
  dbType: string,
  connStr: string,
): Promise<RemoteTableInfo[]> =>
  invoke("migration_list_tables", { dbType, connStr });

export const migrationQueryRemote = (
  dbType: string,
  connStr: string,
  query: string,
  maxRows?: number,
): Promise<QueryResult> =>
  invoke("migration_query_remote", { dbType, connStr, query, maxRows });

export const migrationListProcesses = (
  filter?: string,
): Promise<ProcessInfo[]> =>
  invoke("migration_list_processes", { filter });

export const migrationFindDbFiles = (
  extraPaths?: string[],
): Promise<DbFileInfo[]> =>
  invoke("migration_find_db_files", { extraPaths });

export const migrationReadFile = (
  path: string,
  maxChars?: number,
): Promise<string> =>
  invoke("migration_read_file", { path, maxChars });

export const migrationDecompress = (
  archivePath: string,
  destDir?: string,
): Promise<DecompressResult> =>
  invoke("migration_decompress", { archivePath, destDir });

export const migrationZanposStats = (): Promise<ZanposStats> =>
  invoke("migration_zanpos_stats");

export const migrationRollback = (
  sinceIso: string,
  user_id: string,
): Promise<RollbackResult> =>
  invoke("migration_rollback", { sinceIso, userId: user_id });

export const migrationAgentChat = (
  history: ChatMessage[],
  message: string,
): Promise<string> =>
  invoke("migration_agent_chat", { input: { history, message } });

// ── Ghost barcode lookup ──────────────────────────────────────────────────────

/** Record a failed barcode scan. Fire-and-forget — never throws. */
export const ghostRecord = (actorUserId: string, barcode: string): Promise<void> =>
  invoke<void>("ghost_record", { barcode, actorUserId }).catch(() => {});

/** Get counts of pending/found/not_found ghost barcodes. Manager+ only. */
export const ghostSummary = (actorUserId: string): Promise<GhostSummary> =>
  invoke("ghost_summary", { actorUserId });

/** Full list of non-dismissed ghost barcodes. Manager+ only. */
export const ghostList = (actorUserId: string): Promise<GhostBarcode[]> =>
  invoke("ghost_list", { actorUserId });

/** Run HTTP lookup chain for all pending barcodes. Manager+ only. */
export const ghostResolve = (actorUserId: string): Promise<ResolveResult> =>
  invoke("ghost_resolve", { actorUserId });

/** Dismiss a ghost barcode (removes from panel). Manager+ only. */
export const ghostDismiss = (id: string, actorUserId: string): Promise<void> =>
  invoke("ghost_dismiss", { id, actorUserId });

/** Get product form pre-fill data from a 'found' ghost barcode. Manager+ only. */
export const ghostPrefill = (id: string, actorUserId: string): Promise<ProductPrefill> =>
  invoke("ghost_prefill", { id, actorUserId });
