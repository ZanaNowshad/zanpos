import { invoke } from "@tauri-apps/api/core";
import type {
  AiChatInput,
  AiChatResponse,
  Cart,
  ExecuteActionInput,
  ExecuteActionResult,
  HeldCartSummary,
  PaymentInput,
  ProductWithPrice,
  ProviderConfig,
  RefundItemInput,
  RefundResult,
  SaleForRefund,
  SaleResult,
  SessionUser,
  Shift,
  StockLevel,
  StockMovementRow,
  SupabaseStatus,
  SyncStatus,
  TodaySummary,
  UndoActionResult,
  UserSummary,
  ValidateProviderResult,
} from "../types";

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

// ─── Inventory commands ───────────────────────────────────────────────────────

export const inventoryGetLevels = (): Promise<StockLevel[]> =>
  invoke("inventory_get_levels");

export const inventoryGetLowStock = (): Promise<StockLevel[]> =>
  invoke("inventory_get_low_stock");

export const inventoryGetMovements = (productId: string): Promise<StockMovementRow[]> =>
  invoke("inventory_get_movements", { productId });
