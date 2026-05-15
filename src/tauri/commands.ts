import { invoke } from "@tauri-apps/api/core";
import type {
  AdminProduct,
  AdminUserRow,
  AiChatInput,
  AiChatResponse,
  AppConfig,
  BranchSettings,
  Cart,
  CategoryRow,
  ExecuteActionInput,
  ExecuteActionResult,
  HeldCartSummary,
  PaymentInput,
  ProductWithPrice,
  ProviderConfig,
  RangeSummary,
  RefundItemInput,
  RefundResult,
  RoleRow,
  SaleForRefund,
  SaleListRow,
  SaleResult,
  SessionUser,
  Shift,
  StockLevel,
  StockMovementRow,
  SupabaseStatus,
  SyncStatus,
  TaxRuleRow,
  TodaySummary,
  TopProduct,
  UndoActionResult,
  UserSummary,
  ValidateProviderResult,
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
  currency: string;
  timezone: string;
  owner_display_name: string;
  owner_username: string;
  owner_pin: string;
}): Promise<AppConfig> =>
  invoke("setup_wizard_complete", { input });

export const settingsGetBranch = (): Promise<BranchSettings> =>
  invoke("settings_get_branch");

export const settingsUpdateBranch = (input: {
  name: string;
  address?: string;
  phone?: string;
  receipt_header?: string;
  receipt_footer?: string;
  tax_number?: string;
  timezone: string;
}): Promise<BranchSettings> =>
  invoke("settings_update_branch", { input });

// ─── Auth commands ────────────────────────────────────────────────────────────

export const authListUsers = (): Promise<UserSummary[]> =>
  invoke("auth_list_users");

export const authLoginPin = (username: string, pin: string): Promise<SessionUser> =>
  invoke("auth_login_pin", { input: { username, pin } });

// ─── Shift commands ───────────────────────────────────────────────────────────

export const shiftGetActive = (device_id: string): Promise<Shift | null> =>
  invoke("shift_get_active", { deviceId: device_id });

export const shiftOpen = (
  branch_id: string,
  device_id: string,
  cashier_user_id: string,
  opening_cash_minor: number
): Promise<Shift> =>
  invoke("shift_open", { input: { branch_id, device_id, cashier_user_id, opening_cash_minor } });

export const shiftClose = (
  shift_id: string,
  counted_cash_minor?: number,
  notes?: string
): Promise<Shift> =>
  invoke("shift_close", { input: { shift_id, counted_cash_minor, notes } });

// ─── Product commands ─────────────────────────────────────────────────────────

export const productSearch = (query: string): Promise<ProductWithPrice[]> =>
  invoke("product_search", { query });

export const productGetByBarcode = (barcode: string): Promise<ProductWithPrice | null> =>
  invoke("product_get_by_barcode", { barcode });

export const productListAll = (): Promise<ProductWithPrice[]> =>
  invoke("product_list_all");

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

export const posRemoveLine = (cart: Cart, cart_line_id: string): Promise<Cart> =>
  invoke("pos_remove_line", { input: { cart, cart_line_id } });

export const posFinalizeSale = (
  cart: Cart,
  payments: PaymentInput[],
  idempotency_key?: string
): Promise<SaleResult> =>
  invoke("pos_finalize_sale", { input: { cart, payments, idempotency_key } });

export const posApplyBillDiscount = (cart: Cart, discount_minor: number): Promise<Cart> =>
  invoke("pos_apply_bill_discount", { input: { cart, discount_minor } });

export const posApplyLineDiscount = (cart: Cart, cart_line_id: string, discount_minor: number): Promise<Cart> =>
  invoke("pos_apply_line_discount", { input: { cart, cart_line_id, discount_minor } });

export const posSetLineNote = (cart: Cart, cart_line_id: string, note: string | null): Promise<Cart> =>
  invoke("pos_set_line_note", { input: { cart, cart_line_id, note } });

export const posAddCustomItem = (
  cart: Cart,
  name: string,
  price_minor: number,
  quantity: string,
): Promise<Cart> =>
  invoke("pos_add_custom_item", { input: { cart, name, price_minor, quantity } });

export const posVoidSale = (sale_id: string, voided_by_user_id: string): Promise<void> =>
  invoke("pos_void_sale", { saleId: sale_id, voidedByUserId: voided_by_user_id });

// ─── Held cart commands ───────────────────────────────────────────────────────

export const heldCartSave = (cart: Cart, note?: string): Promise<HeldCartSummary> =>
  invoke("held_cart_save", { input: { cart, note } });

export const heldCartList = (device_id: string): Promise<HeldCartSummary[]> =>
  invoke("held_cart_list", { deviceId: device_id });

export const heldCartResume = (held_cart_id: string, shift_id: string): Promise<Cart> =>
  invoke("held_cart_resume", { input: { held_cart_id, shift_id } });

export const heldCartDelete = (held_cart_id: string): Promise<void> =>
  invoke("held_cart_delete", { heldCartId: held_cart_id });

// ─── Refund commands ──────────────────────────────────────────────────────────

export const refundGetSale = (receipt_number: string): Promise<SaleForRefund> =>
  invoke("refund_get_sale", { receiptNumber: receipt_number });

export const refundCreate = (
  original_sale_id: string,
  items: RefundItemInput[],
  reason: string,
  created_by_user_id: string
): Promise<RefundResult> =>
  invoke("refund_create", { input: { original_sale_id, items, reason, created_by_user_id } });

export const receiptReprint = (receipt_number: string): Promise<SaleResult> =>
  invoke("receipt_reprint", { receiptNumber: receipt_number });

// ─── Report commands ──────────────────────────────────────────────────────────

export const reportToday = (branch_id: string, business_date: string): Promise<TodaySummary> =>
  invoke("report_today", { branchId: branch_id, businessDate: business_date });

export const reportDateRange = (branch_id: string, from_date: string, to_date: string): Promise<RangeSummary> =>
  invoke("report_date_range", { branchId: branch_id, fromDate: from_date, toDate: to_date });

export const reportTopProducts = (branch_id: string, from_date: string, to_date: string): Promise<TopProduct[]> =>
  invoke("report_top_products", { branchId: branch_id, fromDate: from_date, toDate: to_date });

export const reportSalesList = (branch_id: string, from_date: string, to_date: string): Promise<SaleListRow[]> =>
  invoke("report_sales_list", { branchId: branch_id, fromDate: from_date, toDate: to_date });

export const dbIntegrityCheck = (): Promise<string> =>
  invoke("db_integrity_check");

// ─── Sync commands ────────────────────────────────────────────────────────────

export const syncStatus = (): Promise<SyncStatus> => invoke("sync_status");
export const syncTriggerNow = (): Promise<string> => invoke("sync_trigger_now");

// ─── Supabase setup ───────────────────────────────────────────────────────────

export const adminSetupSupabase = (
  url: string,
  serviceKey: string,
  pat: string
): Promise<void> =>
  invoke("admin_setup_supabase", { url, serviceKey, pat });

export const adminGetSupabaseStatus = (): Promise<SupabaseStatus> =>
  invoke("admin_get_supabase_status");

// ─── AI Admin — provider management ──────────────────────────────────────────

export const adminGetProviderConfig = (): Promise<ProviderConfig> =>
  invoke("admin_get_provider_config");

export const adminSetAnthropic = (apiKey: string): Promise<void> =>
  invoke("admin_set_anthropic", { apiKey });

export const adminValidateOpenai = (
  baseUrl: string,
  apiKey: string
): Promise<ValidateProviderResult> =>
  invoke("admin_validate_openai", { baseUrl, apiKey });

export const adminSetOpenai = (
  baseUrl: string,
  apiKey: string,
  model: string
): Promise<void> =>
  invoke("admin_set_openai", { baseUrl, apiKey, model });

// Legacy — kept for compat
export const adminGetApiKeySet = (): Promise<boolean> =>
  invoke("admin_get_api_key_set");

export const adminSetApiKey = (key: string): Promise<void> =>
  invoke("admin_set_api_key", { key });

export const aiChat = (input: AiChatInput): Promise<AiChatResponse> =>
  invoke("ai_chat", { input });

export const aiExecuteAction = (input: ExecuteActionInput): Promise<ExecuteActionResult> =>
  invoke("ai_execute_action", { input });

export const aiCancelAction = (actionId: string): Promise<void> =>
  invoke("ai_cancel_action", { actionId });

export const aiUndoAction = (
  undoId: string,
  userId: string,
  currencyExponent: number
): Promise<UndoActionResult> =>
  invoke("ai_undo_action", { undoId, userId, currencyExponent });

// ─── Back-office admin commands ───────────────────────────────────────────────

export const adminListProducts = (): Promise<AdminProduct[]> =>
  invoke("admin_list_products");

export const adminCreateProduct = (input: {
  category_id: string; name: string; sku?: string; barcode?: string;
  tax_rule_id?: string; price_minor: number;
  track_inventory: boolean; allow_decimal_quantity: boolean;
  reorder_point: number; created_by_user_id: string;
}): Promise<AdminProduct> =>
  invoke("admin_create_product", { input });

export const adminUpdateProduct = (input: {
  product_id: string; category_id: string; name: string; sku?: string; barcode?: string;
  tax_rule_id?: string; price_minor: number;
  track_inventory: boolean; allow_decimal_quantity: boolean;
  reorder_point: number; is_active: boolean; updated_by_user_id: string;
}): Promise<AdminProduct> =>
  invoke("admin_update_product", { input });

export const adminListCategories = (): Promise<CategoryRow[]> =>
  invoke("admin_list_categories");

export const adminListTaxRules = (): Promise<TaxRuleRow[]> =>
  invoke("admin_list_tax_rules");

export const adminSaveCategory = (input: {
  category_id?: string; name: string; sort_order: number; is_active: boolean;
}): Promise<CategoryRow> =>
  invoke("admin_save_category", { input });

export const adminListUsersAll = (): Promise<AdminUserRow[]> =>
  invoke("admin_list_users_all");

export const adminListRoles = (): Promise<RoleRow[]> =>
  invoke("admin_list_roles");

export const adminCreateUser = (input: {
  display_name: string; username: string; pin: string; role_id: string;
}): Promise<AdminUserRow> =>
  invoke("admin_create_user", { input });

export const adminUpdateUser = (input: {
  user_id: string; display_name: string; pin?: string; role_id: string; is_active: boolean;
}): Promise<AdminUserRow> =>
  invoke("admin_update_user", { input });

// ─── Inventory commands ───────────────────────────────────────────────────────

export const inventoryGetLevels = (): Promise<StockLevel[]> =>
  invoke("inventory_get_levels");

export const inventoryGetLowStock = (): Promise<StockLevel[]> =>
  invoke("inventory_get_low_stock");

export const inventoryGetMovements = (productId: string): Promise<StockMovementRow[]> =>
  invoke("inventory_get_movements", { productId });

export const inventoryReceiveStock = (
  product_id: string,
  quantity: string,
  notes: string | undefined,
  received_by_user_id: string,
): Promise<StockLevel> =>
  invoke("inventory_receive_stock", {
    input: { product_id, quantity, notes, received_by_user_id },
  });

export const inventoryAdjustStock = (
  product_id: string,
  new_quantity: string,
  notes: string | undefined,
  adjusted_by_user_id: string,
): Promise<StockLevel> =>
  invoke("inventory_adjust_stock", {
    input: { product_id, new_quantity, notes, adjusted_by_user_id },
  });
