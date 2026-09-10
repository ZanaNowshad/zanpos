import { invoke, Channel } from "@tauri-apps/api/core";
import type {
  AdminProduct,
  AiActionSummary,
  UndoAvailability,
  AdminUserRow,
  AiChatInput,
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
  HubTruthCompareResult,
  MarginSummary,
  PaymentInput,
  ProductMarginRow,
  ProductBarcodeRow,
  ProductWithPrice,
  PurchaseOrderCreateInput,
  PurchaseOrderDetail,
  PurchaseOrderRow,
  ReceivePurchaseOrderInput,
  ReceivePurchaseOrderResult,
  ProviderConfig,
  RangeSummary,
  RefundItemInput,
  RefundResult,
  RiderRow,
  RoleRow,
  SaleForRefund,
  SaleListRow,
  SaleListPage,
  SaleResult,
  SessionUser,
  Shift,
  StockLevel,
  StockMovementRow,
  SupplierRow,
  SupplierUpsertInput,
  SyncConflictRow,
  StockDriftRow,
  SyncQueueItem,
  SyncDiagnostics,
  SyncTableStats,
  SyncStatus,
  TaxRuleRow,
  ThermalConfig,
  TodaySummary,
  ReprintQueueEntry,
  TopProduct,
  UndoActionResult,
  StreamEvent,
  UserSummary,
  SessionToken,
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
  DuplicateGroup,
  DiagnosticReport,
  HealthFixResult,
  SystemHealthReport,
  WaContact,
  WaGroup,
  WaTargets,
  WaMessage,
  WaMedia,
  WaOrder,
  WaOrderMatch,
  PaymentConfirmation,
  CatalogImportProposal,
  CatalogApplyInput,
  CatalogApplyResult,
  HubStatus,
  HubTestResult,
  StartupComponentStatus,
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
  products: number;
  categories: number;
  product_barcodes: number;
  product_prices: number;
  users: number;
  devices: number;
  stock_levels: number;
  suppliers: number;
  settings: number;
  pending_sync: number;
  consistency_score: number;
  schema_match: boolean;
  hub_truth_ok: boolean;
  mismatched_tables: string[];
  /** Whether this terminal holds the catalogue it needs to sell. This decides
   *  whether the POS may open — not whole-database parity, which a terminal
   *  with unsent local sales can never reach. */
  catalogue_ready: boolean;
  /** Tables the hub and this terminal agree about, so "synced and genuinely
   *  empty" can be told apart from "not downloaded yet". */
  matched_tables: string[];
}

/** Blocking initial store-data pull called after joining a hub store. */
export const setupPullCatalog = (): Promise<PullSummary> =>
  invoke("setup_pull_catalog");

export const settingsGetBranch = (sessionToken: SessionToken): Promise<BranchSettings> =>
  invoke("settings_get_branch", { sessionToken });

export const settingsUpdateBranch = (input: {
  name: string;
  address?: string;
  phone?: string;
  receipt_header?: string;
  receipt_footer?: string;
  tax_number?: string;
  cr_number?: string;
  timezone: string;
}, sessionToken: SessionToken): Promise<BranchSettings> =>
  invoke("settings_update_branch", { input, sessionToken });

// ─── Business flags ────────────────────────────────────────────────────────────

export const businessFlagsLoad = (): Promise<BusinessFlags> =>
  invoke("business_flags_load");

export const businessFlagsSave = (
  flags: BusinessFlags,
  sessionToken: SessionToken,
): Promise<void> =>
  invoke("business_flags_save", { input: { flags }, sessionToken });

// ─── Operational settings ──────────────────────────────────────────────────────

export const operationalSettingsLoad = (): Promise<import("../types").OperationalSettings> =>
  invoke("operational_settings_load");

export const operationalSettingsSave = (
  settings: import("../types").OperationalSettings,
  sessionToken: SessionToken,
): Promise<void> =>
  invoke("operational_settings_save", { input: { settings }, sessionToken });

// ─── Onboarding wizard progress (resumable) ──────────────────────────────────

export interface OnboardingStepRow {
  step: string;
  completed_at: string;
}

export const onboardingGetState = (): Promise<OnboardingStepRow[]> =>
  invoke("onboarding_get_state");

export const onboardingMarkStep = (step: string, sessionToken: SessionToken): Promise<void> =>
  invoke("onboarding_mark_step", { step, sessionToken });

// ─── Auth commands ────────────────────────────────────────────────────────────

export const authListUsers = (): Promise<UserSummary[]> =>
  invoke("auth_list_users");

export const authLoginPin = (username: string, pin: string): Promise<SessionUser> =>
  invoke("auth_login_pin", { input: { username, pin } });

export const authLogout = (sessionToken: SessionToken): Promise<void> =>
  invoke("auth_logout", { sessionToken });

// ─── Shift commands ───────────────────────────────────────────────────────────

export const shiftGetActive = (device_id: string, actorUserId = ""): Promise<Shift | null> =>
  invoke("shift_get_active", { deviceId: device_id, actorUserId });

export const shiftOpen = (
  branch_id: string,
  device_id: string,
  session_token: SessionToken,
  opening_cash_minor: number
): Promise<Shift> =>
  invoke("shift_open", { input: { branch_id, device_id, session_token, opening_cash_minor } });

export const shiftClose = (
  shift_id: string,
  session_token: SessionToken,
  counted_cash_minor?: number,
  notes?: string
): Promise<Shift> =>
  invoke("shift_close", { input: { shift_id, session_token, counted_cash_minor, notes } });

// ─── Product commands ─────────────────────────────────────────────────────────

export const productSearch = (sessionToken: SessionToken, query: string): Promise<ProductWithPrice[]> =>
  invoke("product_search", { sessionToken, query });

export const productGetByBarcode = (sessionToken: SessionToken, barcode: string): Promise<ProductWithPrice | null> =>
  invoke("product_get_by_barcode", { sessionToken, barcode });

export const productListAll = (
  sessionToken: SessionToken,
  afterId?: string | null,
  pageSize?: number,
): Promise<ProductWithPrice[]> =>
  invoke("product_list_all", {
    sessionToken,
    afterId: afterId ?? null,
    pageSize: pageSize ?? 50,
  });

// ─── POS commands ─────────────────────────────────────────────────────────────

export const posStartCart = (
  branch_id: string,
  device_id: string,
  shift_id: string,
  session_token: SessionToken
): Promise<{ cart: Cart }> =>
  invoke("pos_start_cart", { input: { branch_id, device_id, shift_id, session_token } });

export const posAddItem = (cart: Cart, product_id: string, quantity: string | undefined, session_token: SessionToken): Promise<Cart> =>
  invoke("pos_add_item", { input: { cart, product_id, quantity, session_token } });

export const posAddItemByBarcode = (cart: Cart, barcode: string, session_token: SessionToken): Promise<Cart> =>
  invoke("pos_add_item_by_barcode", { input: { cart, barcode, session_token } });

export const posUpdateQuantity = (cart: Cart, cart_line_id: string, quantity: string, session_token: SessionToken): Promise<Cart> =>
  invoke("pos_update_quantity", { input: { cart, cart_line_id, quantity, session_token } });

export const posSetLinePrice = (cart: Cart, cart_line_id: string, price_minor: number, manager_override_token: string): Promise<Cart> =>
  invoke("pos_set_line_price", { input: { cart, cart_line_id, price_minor, manager_override_token } });

export interface RepricedLine {
  cart_line_id: string;
  product_name: string;
  was_minor: number;
  now_minor: number;
}

export interface RepriceCartResult {
  cart: Cart;
  changed: RepricedLine[];
}

/** Bring every line up to the catalogue price currently in force.
 *
 *  Needs no manager authorization because it can only ever move a price toward
 *  what the shop has published — the opposite direction from the one worth
 *  controlling. Lines carrying an approved manager override are left alone. */
export const posRepriceCart = (cart: Cart, session_token: SessionToken): Promise<RepriceCartResult> =>
  invoke("pos_reprice_cart", { input: { cart, session_token } });

export const posRemoveLine = (cart: Cart, cart_line_id: string, session_token: SessionToken): Promise<Cart> =>
  invoke("pos_remove_line", { input: { cart, cart_line_id, session_token } });

export const posFinalizeSale = (
  cart: Cart,
  payments: PaymentInput[],
  session_token: SessionToken,
  idempotency_key: string = crypto.randomUUID(),
  customer_id?: string,
  delivery?: import("../types").DeliveryInput,
): Promise<SaleResult> => {
  return invoke("pos_finalize_sale", { input: { cart, payments, session_token, idempotency_key, customer_id, delivery } });
};

export const posApplyBillDiscount = (cart: Cart, discount_minor: number, reason: string, session_token: SessionToken, manager_override_token?: string): Promise<Cart> =>
  invoke("pos_apply_bill_discount", { input: { cart, discount_minor, reason, session_token, manager_override_token } });

export const posApplyLineDiscount = (cart: Cart, cart_line_id: string, discount_minor: number, reason: string, session_token: SessionToken, manager_override_token?: string): Promise<Cart> =>
  invoke("pos_apply_line_discount", { input: { cart, cart_line_id, discount_minor, reason, session_token, manager_override_token } });

export const posSetLineNote = (cart: Cart, cart_line_id: string, note: string | null, session_token: SessionToken): Promise<Cart> =>
  invoke("pos_set_line_note", { input: { cart, cart_line_id, note, session_token } });

export const posAddCustomItem = (
  cart: Cart,
  name: string,
  price_minor: number,
  quantity: string,
  session_token: SessionToken,
): Promise<Cart> =>
  invoke("pos_add_custom_item", { input: { cart, name, price_minor, quantity, session_token } });

/** Load a completed sale back into a Cart for editing.
 *  Returns a pre-populated Cart with each line item as a custom item
 *  preserving original prices, quantities, and discounts. */
export const posLoadSaleForEdit = (
  receipt_number: string,
  branch_id: string,
  device_id: string,
  shift_id: string,
  session_token: SessionToken,
): Promise<Cart> =>
  invoke("pos_load_sale_for_edit", {
    input: { receipt_number, branch_id, device_id, shift_id, session_token },
  });

export interface VoidSaleResult {
  voided: boolean;
  stock_warning: string | null;
}

export const posVoidSale = (sale_id: string, manager_override_token: string): Promise<VoidSaleResult> =>
  invoke("pos_void_sale", { saleId: sale_id, managerOverrideToken: manager_override_token });

export const posRecordVoid = (
  cart_id: string,
  device_id: string,
  sessionToken: SessionToken,
  line_count: number,
  net_total_minor: number,
): Promise<void> =>
  invoke("pos_record_void", { cartId: cart_id, deviceId: device_id, sessionToken, lineCount: line_count, netTotalMinor: net_total_minor });

export const posCartSummary = (cart: Cart, sessionToken: SessionToken): Promise<{
  gross_total_minor: number;
  tax_total_minor: number;
  discount_total_minor: number;
  net_total_minor: number;
  line_count: number;
}> =>
  invoke("pos_cart_summary", { cart, sessionToken });

// ─── Held cart commands ───────────────────────────────────────────────────────

export const heldCartSave = (sessionToken: SessionToken, cart: Cart, note?: string): Promise<HeldCartSummary> =>
  invoke("held_cart_save", { input: { cart, note }, sessionToken });

export const heldCartList = (sessionToken: SessionToken, device_id: string): Promise<HeldCartSummary[]> =>
  invoke("held_cart_list", { sessionToken, deviceId: device_id });

export const heldCartResume = (sessionToken: SessionToken, held_cart_id: string, shift_id: string): Promise<Cart> =>
  invoke("held_cart_resume", { input: { held_cart_id, shift_id }, sessionToken });

export const heldCartDelete = (sessionToken: SessionToken, held_cart_id: string): Promise<void> =>
  invoke("held_cart_delete", { sessionToken, heldCartId: held_cart_id });

// ─── Refund commands ──────────────────────────────────────────────────────────

export const refundGetSale = (receipt_number: string, sessionToken: SessionToken): Promise<SaleForRefund> =>
  invoke("refund_get_sale", { receiptNumber: receipt_number, sessionToken });

export const refundCreate = (
  original_sale_id: string,
  items: RefundItemInput[],
  reason: string,
  session_token: SessionToken,
  return_reason_code?: string,
  manager_override_token?: string,
  idempotency_key?: string,
): Promise<RefundResult> =>
  invoke("refund_create", {
    input: {
      original_sale_id,
      items,
      reason,
      return_reason_code: return_reason_code ?? null,
      session_token,
      manager_override_token: manager_override_token ?? null,
      idempotency_key: idempotency_key ?? null,
    },
  });

export const receiptReprint = (receipt_number: string, sessionToken: SessionToken): Promise<SaleResult> =>
  invoke("receipt_reprint", { receiptNumber: receipt_number, sessionToken });

// ─── Report commands ──────────────────────────────────────────────────────────

export const reportToday = (sessionToken: SessionToken, branch_id: string, business_date: string): Promise<TodaySummary> =>
  invoke("report_today", { sessionToken, branchId: branch_id, businessDate: business_date });

export const reportDateRange = (sessionToken: SessionToken, branch_id: string, from_date: string, to_date: string): Promise<RangeSummary> =>
  invoke("report_date_range", { sessionToken, branchId: branch_id, fromDate: from_date, toDate: to_date });

export const reportTopProducts = (sessionToken: SessionToken, branch_id: string, from_date: string, to_date: string): Promise<TopProduct[]> =>
  invoke("report_top_products", { sessionToken, branchId: branch_id, fromDate: from_date, toDate: to_date });

export const reportMargin = (sessionToken: SessionToken, branch_id: string, from_date: string, to_date: string): Promise<MarginSummary> =>
  invoke("report_margin", { sessionToken, branchId: branch_id, fromDate: from_date, toDate: to_date });

export const reportProductMargin = (
  sessionToken: SessionToken,
  branch_id: string,
  from_date: string,
  to_date: string,
  limit = 50,
): Promise<ProductMarginRow[]> =>
  invoke("report_product_margin", { sessionToken, branchId: branch_id, fromDate: from_date, toDate: to_date, limit });

// ─── Purchasing commands ─────────────────────────────────────────────────────

export const supplierList = (sessionToken: SessionToken): Promise<SupplierRow[]> =>
  invoke("supplier_list", { sessionToken });

export const supplierUpsert = (sessionToken: SessionToken, input: SupplierUpsertInput): Promise<SupplierRow> =>
  invoke("supplier_upsert", { sessionToken, input });

export const supplierDelete = (sessionToken: SessionToken, supplierId: string): Promise<void> =>
  invoke("supplier_delete", { sessionToken, supplierId });

export const poList = (sessionToken: SessionToken, status?: string): Promise<PurchaseOrderRow[]> =>
  invoke("po_list", { sessionToken, status: status ?? null });

export const poGet = (sessionToken: SessionToken, poId: string): Promise<PurchaseOrderDetail> =>
  invoke("po_get", { sessionToken, poId });

export const poCreate = (sessionToken: SessionToken, input: PurchaseOrderCreateInput): Promise<PurchaseOrderDetail> =>
  invoke("po_create", { sessionToken, input });

export const poReceive = (input: ReceivePurchaseOrderInput, sessionToken: SessionToken): Promise<ReceivePurchaseOrderResult> =>
  invoke("po_receive", { input, sessionToken });

/**
 * Recent cost changes, newest first. Manager/owner only and bounded server-side.
 * Store-wide by nature — see ProductCostChange for why there is no branch here.
 */
export const productCostHistoryList = (
  sessionToken: SessionToken, productId?: string, limit?: number,
): Promise<import("../types").ProductCostChange[]> =>
  invoke("product_cost_history_list", {
    sessionToken, productId: productId ?? null, limit: limit ?? null,
  });

/**
 * Persisted AI actions for the Review queue.
 * Branch scope is applied server-side from the session token — the caller
 * cannot request another branch.
 */
export const aiListActions = (
  session_token: SessionToken,
  statuses: string[],
  limit = 50,
  offset = 0,
): Promise<AiActionSummary[]> =>
  invoke("ai_list_actions", { sessionToken: session_token, statuses, limit, offset });

/** Undo availability for one executed action. Branch scope is server-side. */
export const aiUndoAvailability = (
  session_token: SessionToken,
  action_id: string,
): Promise<UndoAvailability | null> =>
  invoke("ai_undo_availability", { sessionToken: session_token, actionId: action_id });

export const poCancel = (sessionToken: SessionToken, poId: string): Promise<void> =>
  invoke("po_cancel", { sessionToken, poId });

/** Fetch paginated sales list. Returns total count alongside items so callers
 *  can detect truncation and implement paging (F-BIZ-002 / F-INT-001).
 *  Defaults: limit=200, offset=0 (backwards-compatible). */
export const reportSalesList = (
  sessionToken: SessionToken,
  branch_id: string,
  from_date: string,
  to_date: string,
  offset?: number,
  limit?: number,
): Promise<SaleListPage> =>
  invoke("report_sales_list", {
    sessionToken,
    branchId: branch_id,
    fromDate: from_date,
    toDate: to_date,
    offset: offset ?? 0,
    limit: limit ?? 200,
  });

export const dbIntegrityCheck = (sessionToken: SessionToken): Promise<string> =>
  invoke("db_integrity_check", { sessionToken });

// ─── Sync commands ────────────────────────────────────────────────────────────

export const syncStatus = (sessionToken: SessionToken): Promise<SyncStatus> => invoke("sync_status", { sessionToken });
export const syncTriggerNow = (sessionToken: SessionToken): Promise<string> => invoke("sync_trigger_now", { sessionToken });

/// Recovery: re-enqueue the full catalog and push immediately. For terminals whose
/// data never reached the cloud (outbox looks empty but cloud is empty).
export const syncForceFullResync = (sessionToken: SessionToken): Promise<string> =>
  invoke("sync_force_full_resync", { sessionToken });

// ─── Hub (LAN sync) ───────────────────────────────────────────────────────────

export const hubStatus = (sessionToken: SessionToken): Promise<HubStatus> =>
  invoke("hub_status", { sessionToken });
export const hubEnable = (sessionToken: SessionToken, port?: number): Promise<HubStatus> =>
  invoke("hub_enable", { sessionToken, port: port ?? null });
export const hubRegenerateToken = (sessionToken: SessionToken): Promise<HubStatus> =>
  invoke("hub_regenerate_token", { sessionToken });
export const hubTestConnection = (url: string, token: string): Promise<HubTestResult> =>
  invoke("hub_test_connection", { url, token });
export const hubJoin = (input: { hub_url: string; token: string; device_name: string; device_code: string })
  : Promise<AppConfig> => invoke("hub_join", { input });
export const hubConnectExisting = (sessionToken: SessionToken, hubUrl: string, token: string): Promise<HubStatus> =>
  invoke("hub_connect_existing", { sessionToken, hubUrl, token });
export const hubSetUrl = (sessionToken: SessionToken, hubUrl: string): Promise<HubStatus> =>
  invoke("hub_set_url", { sessionToken, hubUrl });

// ─── AI Admin — provider management ──────────────────────────────────────────

export const adminGetProviderConfig = (sessionToken: SessionToken): Promise<ProviderConfig> =>
  invoke("admin_get_provider_config", { sessionToken });

export const adminSetAnthropic = (sessionToken: SessionToken, apiKey: string): Promise<void> =>
  invoke("admin_set_anthropic", { sessionToken, apiKey });

export const adminValidateOpenai = (
  sessionToken: SessionToken,
  baseUrl: string,
  apiKey: string
): Promise<ValidateProviderResult> =>
  invoke("admin_validate_openai", { sessionToken, baseUrl, apiKey });

export const adminSetOpenai = (
  sessionToken: SessionToken,
  baseUrl: string,
  apiKey: string,
  model: string
): Promise<void> =>
  invoke("admin_set_openai", { sessionToken, baseUrl, apiKey, model });

// Google Gemini (OpenAI-compatible endpoint; base URL is fixed server-side)
export const adminValidateGemini = (sessionToken: SessionToken, apiKey: string): Promise<ValidateProviderResult> =>
  invoke("admin_validate_gemini", { sessionToken, apiKey });

export const adminSetGemini = (
  sessionToken: SessionToken,
  apiKey: string,
  model: string
): Promise<void> =>
  invoke("admin_set_gemini", { sessionToken, apiKey, model });

export const adminDeleteProvider = (sessionToken: SessionToken): Promise<void> =>
  invoke("admin_delete_provider", { sessionToken });

export const adminGetAiConfig = (sessionToken: SessionToken): Promise<import("../types").AiConfigPayload> =>
  invoke("admin_get_ai_config", { sessionToken });

export const adminSaveAiConfig = (sessionToken: SessionToken, config: import("../types").AiConfigPayload): Promise<void> =>
  invoke("admin_save_ai_config", { sessionToken, config });

export const adminGetAiEnabled = (sessionToken: SessionToken): Promise<boolean> =>
  invoke("admin_get_ai_enabled", { sessionToken });

export const adminSetAiEnabled = (sessionToken: SessionToken, enabled: boolean): Promise<void> =>
  invoke("admin_set_ai_enabled", { sessionToken, enabled });

export const adminGetFeatureToggles = (sessionToken: SessionToken): Promise<import("../types").FeatureToggles> =>
  invoke("admin_get_feature_toggles", { sessionToken });

export const adminSaveFeatureToggles = (sessionToken: SessionToken, toggles: import("../types").FeatureToggles): Promise<void> =>
  invoke("admin_save_feature_toggles", { sessionToken, toggles });

export const adminListAiTools = (sessionToken: SessionToken): Promise<import("../components/settings/ZanAiToolCentre").ZanAiToolCentreRow[]> =>
  invoke("admin_list_ai_tools", { sessionToken });

export const adminSetAiToolEnabled = (sessionToken: SessionToken, toolName: string, enabled: boolean): Promise<void> =>
  invoke("admin_set_ai_tool_enabled", { sessionToken, toolName, enabled });

export const adminListAiToolMetrics = (sessionToken: SessionToken, limit = 100): Promise<import("../components/settings/ZanAiToolMetrics").ZanAiToolMetricRow[]> =>
  invoke("admin_list_ai_tool_metrics", { sessionToken, limit });

export const adminSetAnthropicModel = (sessionToken: SessionToken, model: string): Promise<void> =>
  invoke("admin_set_anthropic_model", { sessionToken, model });

// Validate Anthropic API key by calling the models endpoint.
export const adminValidateAnthropic = (sessionToken: SessionToken, apiKey: string): Promise<ValidateProviderResult> =>
  invoke("admin_validate_anthropic", { sessionToken, apiKey });

export const aiExecuteAction = (sessionToken: SessionToken, input: ExecuteActionInput): Promise<ExecuteActionResult> =>
  invoke("ai_execute_action", { sessionToken, input });

export const aiExecuteBatchActions = (sessionToken: SessionToken, input: {
  action_ids: string[];
  history: ChatMessage[];
  assistant_text: string;
  currency_exponent: number;
}): Promise<{ followup: string; undo_ids: string[] }> =>
  invoke("ai_execute_batch_actions", { sessionToken, input });

export const aiCancelAction = (sessionToken: SessionToken, actionId: string): Promise<void> =>
  invoke("ai_cancel_action", { sessionToken, actionId });

export const aiUndoAction = (
  sessionToken: SessionToken,
  undoId: string,
  currencyExponent: number
): Promise<UndoActionResult> =>
  invoke("ai_undo_action", { sessionToken, undoId, currencyExponent });

export const aiChatStream = (
  sessionToken: SessionToken,
  input: AiChatInput,
  onEvent: Channel<StreamEvent>
): Promise<void> => invoke("ai_chat_stream", { sessionToken, input, onEvent });

export const aiCancelChat = (sessionToken: SessionToken, requestId: string): Promise<void> =>
  invoke("ai_cancel_chat", { sessionToken, requestId });

export const aiRunExecute = (
  sessionToken: SessionToken,
  runId: string,
  onEvent: Channel<StreamEvent>
): Promise<void> => invoke("ai_run_execute", { sessionToken, runId, onEvent });

export const aiRunUndo = (
  sessionToken: SessionToken,
  runId: string,
): Promise<{ followup: string }> => invoke("ai_run_undo", { sessionToken, runId });

export const aiRunCancel = (
  sessionToken: SessionToken,
  runId: string,
): Promise<void> => invoke("ai_run_cancel", { sessionToken, runId });

export const aiSaveMessage = (
  sessionToken: SessionToken,
  sessionId: string,
  branchId: string,
  role: string,
  content: string,
  messageType: string
): Promise<string> =>
  invoke("ai_save_message", { sessionToken, sessionId, branchId, role, content, messageType });

/** Reopens the thread last spoken to, with its id so the next message joins it. */
export const aiLoadHistory = (
  sessionToken: SessionToken,
  branchId: string,
): Promise<import("../types").AiConversationView> =>
  invoke("ai_load_history", { sessionToken, branchId });

export const aiListConversations = (
  sessionToken: SessionToken,
  branchId: string,
): Promise<import("../types").AiConversation[]> =>
  invoke("ai_list_conversations", { sessionToken, branchId });

export const aiOpenConversation = (
  sessionToken: SessionToken,
  branchId: string,
  conversationId: string,
): Promise<import("../types").AiConversationView> =>
  invoke("ai_open_conversation", { sessionToken, branchId, conversationId });

/** Archives rather than destroys — the record of what the AI was asked to do
 *  outlives a tidy-up of the sidebar. */
export const aiDeleteConversation = (
  sessionToken: SessionToken,
  branchId: string,
  conversationId: string,
): Promise<void> =>
  invoke("ai_delete_conversation", { sessionToken, branchId, conversationId });

export const aiRenameConversation = (
  sessionToken: SessionToken,
  branchId: string,
  conversationId: string,
  title: string,
): Promise<void> =>
  invoke("ai_rename_conversation", { sessionToken, branchId, conversationId, title });

export const aiGetTaskLedgerResume = (
  sessionToken: SessionToken,
  branchId: string,
): Promise<import("../types").TaskLedgerResume | null> =>
  invoke("ai_get_task_ledger_resume", { sessionToken, branchId });

export const aiClearHistory = (
  sessionToken: SessionToken,
  branchId: string,
): Promise<void> => invoke("ai_clear_history", { sessionToken, branchId });

export const aiSubmitFeedback = (
  sessionToken: SessionToken,
  sessionId: string,
  messageId: string,
  rating: string,
  comment?: string,
): Promise<void> => invoke("ai_submit_feedback", { sessionToken, sessionId, messageId, rating, comment });

// ─── Proactive alerts ──────────────────────────────────────────────────────

export const adminGetAlerts = (
  sessionToken: SessionToken,
  branchId: string
): Promise<import("../types").ProactiveAlert[]> =>
  invoke("admin_get_alerts", { sessionToken, branchId });

export const adminDismissAlert = (
  sessionToken: SessionToken,
  alertId: string
): Promise<void> =>
  invoke("admin_dismiss_alert", { sessionToken, alertId });

export const aiGetUsageSummary = (sessionToken: SessionToken, days?: number): Promise<unknown> =>
  invoke("ai_get_usage_summary", { sessionToken, days: days ?? null });

// ─── Back-office admin commands ───────────────────────────────────────────────

export interface AdminProductPage {
  items: AdminProduct[];
  total: number;
  offset: number;
  limit: number;
}

/**
 * A catalogue saved view. Filtered server-side, next to the LIMIT, so the
 * counts are the whole catalogue rather than the page currently loaded.
 */
export type ProductView =
  | "all" | "active" | "inactive"
  | "out_of_stock" | "low_stock"
  | "no_barcode" | "no_image";

export const adminListProducts = (
  sessionToken: SessionToken,
  opts?: { search?: string; categoryId?: string; view?: ProductView; offset?: number; limit?: number }
): Promise<AdminProductPage> =>
  invoke("admin_list_products", {
    sessionToken,
    search: opts?.search ?? null,
    categoryId: opts?.categoryId ?? null,
    view: opts?.view ?? null,
    offset: opts?.offset ?? 0,
    limit: opts?.limit ?? 100,
  });

export interface SaleCursorPage {
  items: SaleListRow[];
  next_cursor: string | null;
  has_more: boolean;
}

export const reportSalesCursor = (
  sessionToken: SessionToken,
  branchId: string,
  fromDate: string,
  toDate: string,
  cursor?: string | null,
  limit = 200,
): Promise<SaleCursorPage> =>
  invoke("report_sales_cursor", { sessionToken, branchId, fromDate, toDate, cursor: cursor ?? null, limit });

export const reportSalesExportCsv = (
  sessionToken: SessionToken,
  branchId: string,
  fromDate: string,
  toDate: string,
  destPath: string,
): Promise<number> =>
  invoke("report_sales_export_csv", { sessionToken, branchId, fromDate, toDate, destPath });

export interface ProductImageSearchRequest {
  productName: string;
  barcode?: string;
  sku?: string;
  categoryName?: string;
  currentImageUrl?: string;
  mode: "fetch" | "change";
}

export interface ProductImageSearchResult {
  imageUrl: string;
  source: string;
  searchQuery: string;
  alternateCount: number;
}

export const adminSearchProductImage = (
  sessionToken: SessionToken,
  request: ProductImageSearchRequest,
): Promise<ProductImageSearchResult> =>
  invoke("admin_search_product_image", { sessionToken, request });

export const adminSetProductImage = (
  sessionToken: SessionToken,
  productId: string,
  imageUrl: string,
): Promise<void> =>
  invoke("admin_set_product_image", { sessionToken, productId, imageUrl });

export const adminCreateProduct = (input: {
  category_id: string; name: string; sku?: string; barcode?: string;
  tax_rule_id?: string; price_minor: number;
  track_inventory: boolean; allow_decimal_quantity: boolean;
  reorder_point: number;
  image_path?: string;
}, sessionToken: SessionToken): Promise<AdminProduct> =>
  invoke("admin_create_product", { input, sessionToken });

export const adminUpdateProduct = (input: {
  product_id: string; category_id: string; name: string; sku?: string; barcode?: string;
  tax_rule_id?: string; price_minor: number;
  track_inventory: boolean; allow_decimal_quantity: boolean;
  reorder_point: number; is_active: boolean;
  image_path?: string;
}, sessionToken: SessionToken): Promise<AdminProduct> =>
  invoke("admin_update_product", { input, sessionToken });

/** Scan the whole catalog for duplicate products (by name, barcode, SKU). Manager+ only. */
export const adminFindDuplicateProducts = (
  sessionToken: SessionToken,
  includeInactive = false,
): Promise<DuplicateGroup[]> =>
  invoke("admin_find_duplicate_products", { sessionToken, includeInactive });

/** Merge a duplicate product into another — combines stock, archives the source. Manager+ only. */
export const adminMergeProducts = (
  sessionToken: SessionToken,
  sourceProductId: string,
  targetProductId: string,
  transferHistory = false,
): Promise<void> =>
  invoke("admin_merge_products", {
    sessionToken,
    sourceProductId,
    targetProductId,
    transferHistory,
  });

/** Soft-delete (archive) a single product. Manager+ only. */
export const adminDeleteProduct = (sessionToken: SessionToken, productId: string): Promise<void> =>
  invoke("admin_delete_product", { sessionToken, productId });

export const adminListCategories = (sessionToken: SessionToken): Promise<CategoryRow[]> =>
  invoke("admin_list_categories", { sessionToken });

export const adminListTaxRules = (sessionToken: SessionToken): Promise<TaxRuleRow[]> =>
  invoke("admin_list_tax_rules", { sessionToken });

export const adminSaveTaxRule = (input: {
  tax_rule_id?: string;
  name: string;
  rate_basis_points: number;
  inclusive: boolean;
  is_active: boolean;
}, sessionToken: SessionToken): Promise<TaxRuleRow> =>
  invoke("admin_save_tax_rule", { input, sessionToken });

export const adminDeleteTaxRule = (tax_rule_id: string, sessionToken: SessionToken): Promise<void> =>
  invoke("admin_delete_tax_rule", { input: { tax_rule_id }, sessionToken });

export const adminSaveCategory = (input: {
  category_id?: string; name: string; sort_order: number; is_active: boolean; parent_category_id?: string;
}, sessionToken: SessionToken): Promise<CategoryRow> =>
  invoke("admin_save_category", { input, sessionToken });

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

export const adminBulkImportCategories = (rows: BulkCategoryRow[], sessionToken: SessionToken): Promise<BulkImportResult> =>
  invoke("admin_bulk_import_categories", { rows, sessionToken });

export const adminBulkImportProducts = (rows: BulkProductRow[], sessionToken: SessionToken): Promise<BulkImportResult> =>
  invoke("admin_bulk_import_products", { rows, sessionToken });

export const adminListUsersAll = (sessionToken: SessionToken): Promise<AdminUserRow[]> =>
  invoke("admin_list_users_all", { sessionToken });

export const adminListRoles = (sessionToken: SessionToken): Promise<RoleRow[]> =>
  invoke("admin_list_roles", { sessionToken });

export const adminCreateUser = (input: {
  display_name: string; username: string; pin: string; role_id: string; session_token: SessionToken;
}): Promise<AdminUserRow> =>
  invoke("admin_create_user", { input });

export const adminUpdateUser = (input: {
  user_id: string; display_name: string; pin?: string; role_id: string; is_active: boolean; session_token: SessionToken;
}): Promise<AdminUserRow> =>
  invoke("admin_update_user", { input });

// ─── Inventory commands ───────────────────────────────────────────────────────

export interface StockLevelPage {
  items: StockLevel[];
  total: number;
  offset: number;
  limit: number;
}

export const inventoryGetLevels = (session_token: SessionToken): Promise<StockLevel[]> =>
  invoke("inventory_get_levels", { sessionToken: session_token });

export const inventoryGetLevelsPaged = (
  session_token: SessionToken,
  search: string,
  offset: number,
  limit: number,
): Promise<StockLevelPage> =>
  invoke("inventory_get_levels_paged", { sessionToken: session_token, search: search || null, offset, limit });

export const inventoryGetLowStock = (session_token: SessionToken): Promise<StockLevel[]> =>
  invoke("inventory_get_low_stock", { sessionToken: session_token });

export const inventoryGetMovements = (session_token: SessionToken, productId: string): Promise<StockMovementRow[]> =>
  invoke("inventory_get_movements", { sessionToken: session_token, productId });

export const inventoryReceiveStock = (
  product_id: string,
  quantity: string,
  expiry_date: string | undefined,
  notes: string | undefined,
  session_token: SessionToken,
): Promise<StockLevel> =>
  invoke("inventory_receive_stock", {
    input: { product_id, quantity, expiry_date, notes },
    sessionToken: session_token,
  });

// ─── Phase 10a commands ───────────────────────────────────────────────────────

export const appConfigGetTimeout = (): Promise<number> =>
  invoke("app_config_get_timeout");

export const appConfigSetTimeout = (minutes: number, sessionToken: SessionToken): Promise<void> =>
  invoke("app_config_set_timeout", { minutes, sessionToken });

// ─── Reports device-scope (Phase E) ───────────────────────────────────────────
export type ReportsConfig = {
  device_scope: "origin" | "all";
  device_count: number;
  local_device_id: string;
};

export const reportsConfigLoad = (sessionToken: SessionToken): Promise<ReportsConfig> =>
  invoke("reports_config_load", { sessionToken });

export const reportsConfigSave = (
  device_scope: "origin" | "all",
  sessionToken: SessionToken
): Promise<void> =>
  invoke("reports_config_save", { input: { device_scope }, sessionToken });

export const dbBackup = (destPath: string, sessionToken: SessionToken): Promise<string> =>
  invoke("db_backup", { destPath, sessionToken });

export const reportTaxByDay = (
  sessionToken: SessionToken,
  branch_id: string,
  from_date: string,
  to_date: string,
): Promise<Array<{ day: string; transaction_count: number; tax_minor: number; cumulative_minor: number }>> =>
  invoke("report_tax_by_day", { sessionToken, branchId: branch_id, fromDate: from_date, toDate: to_date });

export const auditLogList = (
  from: string,
  to: string,
  page: number,
  sessionToken: SessionToken,
): Promise<Array<{ audit_log_id: string; event_type: string; entity_type: string; entity_id: string | null; actor_user_id: string | null; created_at: string }>> =>
  invoke("audit_log_list", { from, to, page, sessionToken });

export const auditVerifyChain = (sessionToken: SessionToken): Promise<{
  total_rows: number; legacy_rows: number; verified: number;
  broken_hash: number; broken_link: number; ok: boolean;
}> => invoke("audit_verify_chain", { sessionToken });

export const inventoryAdjustStock = (
  product_id: string,
  new_quantity: string,
  notes: string | undefined,
  session_token: SessionToken,
): Promise<StockLevel> =>
  invoke("inventory_adjust_stock", {
    input: { product_id, new_quantity, notes },
    sessionToken: session_token,
  });

// ─── Phase 10b — Customers ────────────────────────────────────────────────────

export interface CustomerPage {
  items: CustomerRow[];
  total: number;
  offset: number;
  limit: number;
}

/** Branch-scoped and paged server-side. `total` counts the whole branch match,
 *  not the page, so the UI can report scale without loading it. */
export const customerList = (
  sessionToken: SessionToken, search: string, offset = 0, limit?: number,
): Promise<CustomerPage> =>
  invoke("customer_list", { sessionToken, search, offset, limit: limit ?? null });

/** SQL aggregate over the actor's whole branch — never derived from a page. */
export const customerLoyaltySummary = (sessionToken: SessionToken): Promise<{
  outstanding_points: number; holders: number;
  total_customers: number; contactable_holders: number;
}> => invoke("customer_loyalty_summary", { sessionToken });

export const customerTopBalances = (sessionToken: SessionToken, limit = 25): Promise<CustomerRow[]> =>
  invoke("customer_top_balances", { sessionToken, limit });

export const customerCreate = (input: {
  name: string; phone?: string; email?: string; notes?: string;
}, sessionToken: SessionToken): Promise<CustomerRow> =>
  invoke("customer_create", { input, sessionToken });

export const customerUpdate = (input: {
  customer_id: string; name: string; phone?: string; email?: string; notes?: string;
}, sessionToken: SessionToken): Promise<CustomerRow> =>
  invoke("customer_update", { input, sessionToken });

export const customerGet = (sessionToken: SessionToken, customerId: string): Promise<CustomerRow> =>
  invoke("customer_get", { sessionToken, customerId });

export const customerAddLoyalty = (sessionToken: SessionToken, customerId: string, points: number): Promise<number> =>
  invoke("customer_add_loyalty", { sessionToken, customerId, points });

// ─── Phase 10b — Devices ──────────────────────────────────────────────────────

export const deviceList = (sessionToken: SessionToken): Promise<DeviceRow[]> =>
  invoke("device_list", { sessionToken });

export const deviceCreate = (sessionToken: SessionToken, input: {
  device_code: string; device_name: string;
}): Promise<DeviceRow> =>
  invoke("device_create", { input, sessionToken });

export const deviceToggleActive = (sessionToken: SessionToken, deviceId: string, isActive: boolean): Promise<void> =>
  invoke("device_toggle_active", { deviceId, isActive, sessionToken });

/** Soft delete — `device_id` is stamped on every sale and shift this terminal
 *  recorded, and receipt numbering is per-device. Refused for the terminal
 *  making the request and for any device with an open shift. */
export const deviceDelete = (sessionToken: SessionToken, deviceId: string): Promise<void> =>
  invoke("device_delete", { deviceId, sessionToken });

export interface DeviceRekeyResult {
  old_device_id: string;
  device_id: string;
}

/** Re-issue this terminal's device identity with a fresh ULID — the recovery
 *  path for a database cloned onto a second PC, which otherwise shares one
 *  device_id with its sibling forever. Irreversible and audited. */
export const deviceRekey = (sessionToken: SessionToken): Promise<DeviceRekeyResult> =>
  invoke("device_rekey", { sessionToken });

// ─── Phase 10b — Product image picker ────────────────────────────────────────

export const productPickImage = (): Promise<string | null> =>
  invoke("product_pick_image");

// ─── Phase 10b — Auto-updater ─────────────────────────────────────────────────

export const checkForUpdates = (): Promise<string | null> =>
  invoke("check_for_updates");

/// Download + install the available update, then restart. Resolves false if none.
/// On success the app restarts, so the promise typically does not resolve.
export const downloadAndInstallUpdate = (sessionToken: SessionToken): Promise<boolean> =>
  invoke("download_and_install_update", { sessionToken });

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

export const thermalGetConfig = (sessionToken: SessionToken): Promise<ThermalConfig> =>
  invoke("thermal_get_config", { sessionToken });

export const thermalSetConfig = (sessionToken: SessionToken, input: ThermalConfig): Promise<void> =>
  invoke("thermal_set_config", { input, sessionToken });

export const thermalPrintTest = (sessionToken: SessionToken): Promise<string> =>
  invoke("thermal_print_test", { sessionToken });

export const printReceiptRaw = (sessionToken: SessionToken, storeName: string, lines: string[]): Promise<string> =>
  invoke("print_receipt_raw", { sessionToken, storeName, lines });

export const reprintQueuePending = (sessionToken: SessionToken): Promise<ReprintQueueEntry[]> =>
  invoke("reprint_queue_pending", { sessionToken });

export const reprintQueueMarkPrinted = (sessionToken: SessionToken, id: string): Promise<void> =>
  invoke("reprint_queue_mark_printed", { sessionToken, id });

/** Open the cash drawer connected to the ESC/POS printer's RJ-11 port.
 *  Returns "opened" on success, "no_printer" if thermal printing is disabled. */
export const openCashDrawer = (sessionToken: SessionToken): Promise<string> =>
  invoke("open_cash_drawer", { sessionToken });

// ─── Cash events ──────────────────────────────────────────────────────────────

export const cashEventCreate = (
  shift_id: string,
  event_type: "paid_in" | "paid_out" | "safe_drop",
  amount_minor: number,
  note: string | undefined,
  sessionToken: SessionToken,
): Promise<CashEventRow> =>
  invoke("cash_event_create", { shiftId: shift_id, eventType: event_type, amountMinor: amount_minor, note, sessionToken });

export const cashEventsList = (sessionToken: SessionToken, shift_id: string): Promise<CashEventRow[]> =>
  invoke("cash_events_list", { sessionToken, shiftId: shift_id });

export const cashDrawerSummary = (sessionToken: SessionToken, shift_id: string): Promise<CashDrawerSummary> =>
  invoke("cash_drawer_summary", { sessionToken, shiftId: shift_id });

export const cashXReport = (shift_id: string, sessionToken: SessionToken): Promise<CashDrawerSummary> =>
  invoke("cash_x_report", { shiftId: shift_id, sessionToken });

export const cashNoSale = (
  shift_id:     string,
  sessionToken: SessionToken,
  note?:        string,
): Promise<NoSaleRow> =>
  invoke("cash_no_sale", { shiftId: shift_id, sessionToken, note: note ?? null });

// ─── Product barcodes ─────────────────────────────────────────────────────────

export const productBarcodeAdd = (sessionToken: SessionToken, product_id: string, barcode: string): Promise<ProductBarcodeRow> =>
  invoke("product_barcode_add", { sessionToken, productId: product_id, barcode });

export const productBarcodeRemove = (sessionToken: SessionToken, barcode_id: string): Promise<void> =>
  invoke("product_barcode_remove", { sessionToken, barcodeId: barcode_id });

export const productBarcodesList = (sessionToken: SessionToken, product_id: string): Promise<ProductBarcodeRow[]> =>
  invoke("product_barcodes_list", { sessionToken, productId: product_id });

// ─── New pilot-hardening commands ─────────────────────────────────────────────

export const reportByCashier = (
  sessionToken: SessionToken,
  branch_id: string,
  from_date: string,
  to_date: string,
): Promise<CashierSummaryRow[]> =>
  invoke("report_by_cashier", { sessionToken, branchId: branch_id, fromDate: from_date, toDate: to_date });

export const reportEodCashup = (
  sessionToken: SessionToken,
  branch_id: string,
  business_date: string,
): Promise<EodCashupReport> =>
  invoke("report_eod_cashup", { sessionToken, branchId: branch_id, date: business_date });

export const inventoryBulkStockTake = (
  // FIX: string, not number — JS floats corrupt Decimal arithmetic in Rust backend
  entries: Array<{ product_id: string; new_quantity: string; notes?: string }>,
  session_token: SessionToken,
): Promise<BulkStockTakeResult> =>
  invoke("inventory_bulk_stock_take", { entries, sessionToken: session_token });

export const syncQueueList = (sessionToken: SessionToken): Promise<SyncQueueItem[]> =>
  invoke("sync_queue_list", { sessionToken });

export const syncQueueRetry = (sessionToken: SessionToken, syncEventId: string): Promise<void> =>
  invoke("sync_queue_retry", { id: syncEventId, sessionToken });

export const syncQueueDismiss = (sessionToken: SessionToken, syncEventId: string): Promise<void> =>
  invoke("sync_queue_dismiss", { id: syncEventId, sessionToken });

export const syncQueueStats = (sessionToken: SessionToken): Promise<SyncTableStats[]> =>
  invoke("sync_queue_stats", { sessionToken });

export const syncDiagnostics = (sessionToken: SessionToken): Promise<SyncDiagnostics> =>
  invoke("sync_diagnostics", { sessionToken });

export const hubTruthCompare = (sessionToken: SessionToken): Promise<HubTruthCompareResult> =>
  invoke("hub_truth_compare", { sessionToken });

export const hubTruthPull = (sessionToken: SessionToken): Promise<HubTruthCompareResult> =>
  invoke("hub_truth_pull", { sessionToken });

export const syncConflictsList = (sessionToken: SessionToken): Promise<SyncConflictRow[]> =>
  invoke("sync_conflicts_list", { sessionToken });

export const syncConflictResolve = (
  sessionToken: SessionToken,
  conflictId: string,
  resolution: "retry" | "pull_hub_truth" | "reconcile_stock" | "dismiss",
): Promise<string> => invoke("sync_conflict_resolve", { sessionToken, conflictId, resolution });

export const syncStockDriftReport = (sessionToken: SessionToken): Promise<StockDriftRow[]> =>
  invoke("sync_stock_drift_report", { sessionToken });

export const syncStockDriftReconcile = (sessionToken: SessionToken): Promise<number> =>
  invoke("sync_stock_drift_reconcile", { sessionToken });

export const syncResetStuck = (sessionToken: SessionToken): Promise<string> =>
  invoke("sync_reset_stuck", { sessionToken });

export const syncBulkInitial = (sessionToken: SessionToken): Promise<string> =>
  invoke("sync_bulk_initial", { sessionToken });

// ─── Delivery commands ────────────────────────────────────────────────────────

export const deliveryList = (
  filter: DeliveryListFilter,
  session_token: SessionToken,
): Promise<DeliveryRow[]> =>
  invoke("delivery_list", { filter, sessionToken: session_token });

export const deliveryGet = (
  delivery_id: string,
  session_token: SessionToken,
): Promise<DeliveryRow> =>
  invoke("delivery_get", { deliveryId: delivery_id, sessionToken: session_token });

export const deliveryUpdateStatus = (
  input: UpdateDeliveryStatusInput,
  session_token: SessionToken,
): Promise<DeliveryRow> =>
  invoke("delivery_update_status", { input, sessionToken: session_token });

export const deliveryConfirmPayment = (
  input: ConfirmDeliveryPaymentInput,
  session_token: SessionToken,
): Promise<DeliveryRow> =>
  invoke("delivery_confirm_payment", { input, sessionToken: session_token });

export const deliveryCancel = (
  input: CancelDeliveryInput,
  session_token: SessionToken,
): Promise<DeliveryRow> =>
  invoke("delivery_cancel", { input, sessionToken: session_token });

export const deliveryRevertPayment = (
  input: RevertPaymentInput,
  session_token: SessionToken,
): Promise<DeliveryRow> =>
  invoke("delivery_revert_payment", { input, sessionToken: session_token });

export const deliveryRiderSuggestions = (
  branch_id: string,
  session_token: SessionToken,
): Promise<string[]> =>
  invoke("delivery_rider_suggestions", { branchId: branch_id, sessionToken: session_token });

// ─── WhatsApp ─────────────────────────────────────────────────────────────────

export function whatsappStatus(sessionToken: SessionToken): Promise<WhatsAppStatus> {
  return invoke("whatsapp_status", { sessionToken });
}

export function whatsappSendDelivery(sessionToken: SessionToken, input: SendDeliveryInput): Promise<boolean> {
  return invoke("whatsapp_send_delivery", { input, sessionToken });
}

export function whatsappNotifyArrival(sessionToken: SessionToken, to: string, receiptNumber: string, deliveryId: string): Promise<boolean> {
  return invoke("whatsapp_notify_arrival", {
    input: { to, receipt_number: receiptNumber, delivery_id: deliveryId },
    sessionToken,
  });
}

export function whatsappPaymentReminder(
  sessionToken: SessionToken,
  to: string,
  receiptNumber: string,
  amountMinor: number,
  currencyExponent: number,
  currency: string,
  deliveryId: string,
): Promise<boolean> {
  return invoke("whatsapp_payment_reminder", {
    input: {
      to,
      receipt_number: receiptNumber,
      amount_minor: amountMinor,
      currency_exponent: currencyExponent,
      currency,
      delivery_id: deliveryId,
    },
    sessionToken,
  });
}

export function whatsappDisconnect(sessionToken: SessionToken): Promise<boolean> {
  return invoke("whatsapp_disconnect", { sessionToken });
}

export function whatsappSaveConfig(benefitNumber: string, sessionToken: SessionToken): Promise<void> {
  return invoke("whatsapp_save_config", { benefitNumber, sessionToken });
}

export function whatsappImportContacts(sessionToken: SessionToken): Promise<ImportContactsResult> {
  return invoke("whatsapp_import_contacts", { sessionToken });
}

// ── WhatsApp → POS notification inbox (admin) ──────────────────────────────────
export const whatsappListContacts = (sessionToken: SessionToken): Promise<WaContact[]> =>
  invoke("whatsapp_list_contacts", { sessionToken });

export const whatsappListGroups = (sessionToken: SessionToken): Promise<WaGroup[]> =>
  invoke("whatsapp_list_groups", { sessionToken });

export const whatsappGetTargets = (sessionToken: SessionToken): Promise<WaTargets> =>
  invoke("whatsapp_get_targets", { sessionToken });

export const whatsappSetTargets = (
  sessionToken: SessionToken,
  t: WaTargets,
): Promise<void> =>
  invoke("whatsapp_set_targets", {
    sessionToken,
    ownerJid: t.owner_jid,
    ownerName: t.owner_name,
    groupJid: t.group_jid,
    groupName: t.group_name,
  });

/** Pull new owner/group messages from the sidecar; returns current unread count. */
export const whatsappPollMessages = (sessionToken: SessionToken): Promise<number> =>
  invoke("whatsapp_poll_messages", { sessionToken });

export const whatsappListMessages = (sessionToken: SessionToken): Promise<WaMessage[]> =>
  invoke("whatsapp_list_messages", { sessionToken });

/** Download the decrypted image for a message (View action). */
export const whatsappGetMedia = (messageId: string, sessionToken: SessionToken): Promise<WaMedia> =>
  invoke("whatsapp_get_media", { messageId, sessionToken });

export const whatsappMarkRead = (id: string, sessionToken: SessionToken): Promise<void> =>
  invoke("whatsapp_mark_read", { id, sessionToken });

export const whatsappMarkAllRead = (sessionToken: SessionToken): Promise<void> =>
  invoke("whatsapp_mark_all_read", { sessionToken });

/** Delete all stored WhatsApp notifications ("Clear all"). */
export const whatsappClearMessages = (sessionToken: SessionToken): Promise<void> =>
  invoke("whatsapp_clear_messages", { sessionToken });

// ── WhatsApp Business Catalog + Commerce ──────────────────────────────────────
export const whatsappCommerceGetEnabled = (sessionToken: SessionToken): Promise<boolean> =>
  invoke("whatsapp_commerce_get_enabled", { sessionToken });

export const whatsappCommerceSetEnabled = (sessionToken: SessionToken, enabled: boolean): Promise<void> =>
  invoke("whatsapp_commerce_set_enabled", { sessionToken, enabled });

/** True when WhatsApp Commerce OR the public Storefront is enabled — gates the POS Orders button. */
export const whatsappOrdersGetEnabled = (sessionToken: SessionToken): Promise<boolean> =>
  invoke("whatsapp_orders_get_enabled", { sessionToken });

export const whatsappOrderList = (
  sessionToken: SessionToken,
  status?: string,
): Promise<WaOrder[]> =>
  invoke("whatsapp_order_list", { sessionToken, status });

export const whatsappOrderUpdateStatus = (
  sessionToken: SessionToken,
  orderId: string,
  status: string,
  linkedSaleId?: string,
): Promise<void> =>
  invoke("whatsapp_order_update_status", { sessionToken, orderId, status, linkedSaleId });

export const whatsappOrderMatch = (
  sessionToken: SessionToken,
  orderId: string,
): Promise<WaOrderMatch> =>
  invoke("whatsapp_order_match", { sessionToken, orderId });

export const whatsappSendProduct = (
  sessionToken: SessionToken,
  to: string,
  waProductId: string,
): Promise<boolean> =>
  invoke("whatsapp_send_product", { sessionToken, to, waProductId });

export const whatsappOrderMessage = (
  sessionToken: SessionToken,
  orderId: string,
  message: string,
): Promise<boolean> =>
  invoke("whatsapp_order_message", { sessionToken, orderId, message });

// ── AI payment verification (WhatsApp screenshot → OCR → AI confirm) ──────────
/** Resolved payment confirmations (confirmed/failed) for the Notification panel. */
export const paymentConfirmationsList = (sessionToken: SessionToken): Promise<PaymentConfirmation[]> =>
  invoke("payment_confirmations_list", { sessionToken });

/** Count of unseen confirmations — folded into the POS bell badge. */
export const paymentConfirmationsUnseenCount = (sessionToken: SessionToken): Promise<number> =>
  invoke("payment_confirmations_unseen_count", { sessionToken });

/** Mark all confirmations as seen (clears the badge contribution). */
export const paymentConfirmationsMarkAllSeen = (sessionToken: SessionToken): Promise<void> =>
  invoke("payment_confirmations_mark_all_seen", { sessionToken });

/** Manager/owner manual override for a confirmation the AI couldn't auto-verify. */
export const paymentConfirmationOverride = (
  id: string,
  confirm: boolean,
  sessionToken: SessionToken,
): Promise<void> => invoke("payment_confirmation_override", { id, confirm, sessionToken });

// ── Invoice / price-list photo → catalog update (review-first) ─────────────────
/** Extract + match an invoice/price-list image. Read-only — returns proposals. */
export const catalogImportExtract = (
  mediaId: string,
  currencyExponent: number,
  sessionToken: SessionToken,
): Promise<CatalogImportProposal> =>
  invoke("catalog_import_extract", { mediaId, currencyExponent, sessionToken });

/** Apply the owner-approved catalog changes (price/cost/stock/create/supplier). */
export const catalogImportApply = (
  input: CatalogApplyInput,
  sessionToken: SessionToken,
): Promise<CatalogApplyResult> => invoke("catalog_import_apply", { input, sessionToken });

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
  sessionToken: SessionToken,
  input: WhatsAppReceiptPdfInput,
): Promise<boolean> {
  return invoke("whatsapp_send_receipt_pdf", { input, sessionToken });
}

export function setupSaveBenefitNumber(benefitNumber: string): Promise<void> {
  return invoke("setup_save_benefit_number", { benefitNumber });
}

// ── Migration Agent ───────────────────────────────────────────────────────────

export const migrationInspectFile = (
  path: string,
  sessionToken: SessionToken,
): Promise<FileSchema> =>
  invoke("migration_inspect_file", { path, sessionToken });

export const migrationAiMap = (
  schema: FileSchema,
  currencyExponent: number,
  sessionToken: SessionToken,
): Promise<MappingConfig> =>
  invoke("migration_ai_map", { schema, currencyExponent, sessionToken });

export const migrationExecute = (
  path: string,
  mapping: MappingConfig,
  currencyExponent: number,
  sessionToken: SessionToken,
  onEvent: Channel<MigrationProgress>,
): Promise<void> =>
  invoke("migration_execute", { path, mapping, currencyExponent, sessionToken, onEvent });

// ── Migration extended tools ──────────────────────────────────────────────────

export const migrationConnectTest = (
  dbType: string,
  connStr: string,
  sessionToken: SessionToken,
): Promise<ConnectTestResult> =>
  invoke("migration_connect_test", { dbType, connStr, sessionToken });

export const migrationListTables = (
  dbType: string,
  connStr: string,
  sessionToken: SessionToken,
): Promise<RemoteTableInfo[]> =>
  invoke("migration_list_tables", { dbType, connStr, sessionToken });

export const migrationQueryRemote = (
  dbType: string,
  connStr: string,
  query: string,
  sessionToken: SessionToken,
  maxRows?: number,
): Promise<QueryResult> =>
  invoke("migration_query_remote", { dbType, connStr, query, sessionToken, maxRows });

export const migrationListProcesses = (
  sessionToken: SessionToken,
  filter?: string,
): Promise<ProcessInfo[]> =>
  invoke("migration_list_processes", { sessionToken, filter });

export const migrationFindDbFiles = (
  sessionToken: SessionToken,
  extraPaths?: string[],
): Promise<DbFileInfo[]> =>
  invoke("migration_find_db_files", { sessionToken, extraPaths });

export const migrationReadFile = (
  path: string,
  maxChars: number | undefined,
  sessionToken: SessionToken,
): Promise<string> =>
  invoke("migration_read_file", { path, maxChars, sessionToken });

export const migrationDecompress = (
  archivePath: string,
  destDir: string | undefined,
  sessionToken: SessionToken,
): Promise<DecompressResult> =>
  invoke("migration_decompress", { archivePath, destDir, sessionToken });

export const migrationZanposStats = (): Promise<ZanposStats> =>
  invoke("migration_zanpos_stats");

export const migrationRollback = (
  sinceIso: string,
  sessionToken: SessionToken,
): Promise<RollbackResult> =>
  invoke("migration_rollback", { sinceIso, sessionToken });

export const migrationAgentChat = (
  history: ChatMessage[],
  message: string,
  sessionToken: SessionToken,
): Promise<string> =>
  invoke("migration_agent_chat", { input: { history, message }, sessionToken });

// ── Ghost barcode lookup ──────────────────────────────────────────────────────

/** Record a failed barcode scan. Fire-and-forget — never throws. */
export const ghostRecord = (sessionToken: SessionToken, barcode: string): Promise<void> =>
  invoke<void>("ghost_record", { barcode, sessionToken }).catch(() => {});

/** Get counts of pending/found/not_found ghost barcodes. Manager+ only. */
export const ghostSummary = (sessionToken: SessionToken): Promise<GhostSummary> =>
  invoke("ghost_summary", { sessionToken });

/** Full list of non-dismissed ghost barcodes. Manager+ only. */
export const ghostList = (sessionToken: SessionToken): Promise<GhostBarcode[]> =>
  invoke("ghost_list", { sessionToken });

/** Run HTTP lookup chain for all pending barcodes. Manager+ only. */
export const ghostResolve = (sessionToken: SessionToken): Promise<ResolveResult> =>
  invoke("ghost_resolve", { sessionToken });

/** Dismiss a ghost barcode (removes from panel). Manager+ only. */
export const ghostDismiss = (id: string, sessionToken: SessionToken): Promise<void> =>
  invoke("ghost_dismiss", { id, sessionToken });

/** Get product form pre-fill data from a 'found' ghost barcode. Manager+ only. */
export const ghostPrefill = (id: string, sessionToken: SessionToken): Promise<ProductPrefill> =>
  invoke("ghost_prefill", { id, sessionToken });

// ── Diagnostics ──────────────────────────────────────────────────────────────────

/** Run system diagnostics and auto-fix common issues (stuck runs, DB integrity). */
export const adminRunDiagnostics = (sessionToken: SessionToken): Promise<DiagnosticReport> =>
  invoke("admin_run_diagnostics", { sessionToken });

export const systemHealthCheck = (sessionToken: SessionToken): Promise<SystemHealthReport> =>
  invoke("system_health_check", { sessionToken });

export const systemHealthApplyFix = (
  sessionToken: SessionToken,
  fixAction: string,
): Promise<HealthFixResult> =>
  invoke("system_health_apply_fix", {
    input: { fix_action: fixAction },
    sessionToken,
  });

// ── Startup health ────────────────────────────────────────────────────────────────

export const startupHealthCheck = (): Promise<StartupComponentStatus[]> =>
  invoke("startup_health_check");

export const startupRestartSidecar = (): Promise<boolean> =>
  invoke("startup_restart_sidecar");

/** Raise the OS on-screen keyboard. Resolves with "touch" for the Windows touch
 *  keyboard or "osk" when it fell back to the accessibility one. */
export const systemKeyboardOpen = (): Promise<"touch" | "osk"> =>
  invoke("system_keyboard_open");

// ── Delivery riders ───────────────────────────────────────────────────────────

/** `activeOnly` is what the till asks for — a rider who has left should not be
 *  offered a new drop, but stays in the admin list so the record can be fixed. */
export const riderList = (sessionToken: SessionToken, activeOnly?: boolean): Promise<RiderRow[]> =>
  invoke("rider_list", { sessionToken, activeOnly });

export const riderCreate = (input: {
  name: string; phone: string; notes?: string;
}, sessionToken: SessionToken): Promise<RiderRow> => invoke("rider_create", { input, sessionToken });

export const riderUpdate = (input: {
  rider_id: string; name: string; phone: string; notes?: string;
  is_active: boolean;
}, sessionToken: SessionToken): Promise<RiderRow> => invoke("rider_update", { input, sessionToken });

export const riderDelete = (riderId: string, sessionToken: SessionToken): Promise<void> =>
  invoke("rider_delete", { riderId, sessionToken });

// ── Quick POS slots ───────────────────────────────────────────────────────────

/** One position in the till's quick-add row. Empty slots come back too, so the
 *  caller renders the gaps rather than inferring them from the index. */
export interface QuickPosSlot {
  slot: number;
  product_id: string | null;
  name: string | null;
  price_minor: number | null;
  image_path: string | null;
}

/** Ten slots, always. Name and price are resolved live, so a repricing reaches
 *  the till without anyone re-picking the item. */
export const quickPosLoad = (sessionToken: SessionToken): Promise<QuickPosSlot[]> =>
  invoke("quick_pos_load", { sessionToken });

export const quickPosSave = (
  sessionToken: SessionToken,
  productIds: (string | null)[],
): Promise<QuickPosSlot[]> =>
  invoke("quick_pos_save", { input: { product_ids: productIds }, sessionToken });

// ── Terminals and reconciliation ─────────────────────────────────────────────

export interface TerminalRow {
  device_id: string;
  branch_id: string;
  device_code: string;
  name: string;
  /** Derived from heartbeat evidence every time it is read. There is no stored
   *  status behind this: `devices.status` was one, nobody updated it, and
   *  terminals that had never contacted the hub displayed as "online". */
  state: "unpaired" | "never_seen" | "online" | "stale" | "offline";
  /** What to do about that state. Carried with the row so the screen cannot
   *  invent its own wording for a meaning defined in the backend. */
  advice: string;
  seconds_since_seen: number | null;
  last_heartbeat_at: string | null;
  observed_ip: string | null;
  app_version: string | null;
  heartbeat_hub_id: string | null;
  /** Seconds this terminal's clock is ahead of the hub's, when the beat
   *  carried a client timestamp. A slow clock hides rows behind the sync
   *  watermark — the advice field carries the warning. */
  clock_skew_secs: number | null;
  is_paired: boolean;
  is_active: boolean;
}

export const terminalRoster = (sessionToken: SessionToken): Promise<TerminalRow[]> =>
  invoke("terminal_roster", { sessionToken });

export interface ReconciliationPreview {
  table: string;
  diverged: number;
  /** Rows one side simply does not have. Delivering them overwrites nothing. */
  deliverable: string[];
  /** Rows both sides hold with different contents — a person decides. */
  needs_review: string[];
  audit: string[];
  /** The hub cannot answer row-level parity. A reason to stop, not a clean bill. */
  hub_too_old: boolean;
}

export const reconciliationPreview = (
  sessionToken: SessionToken,
  table: string,
): Promise<ReconciliationPreview> =>
  invoke("reconciliation_preview", { sessionToken, table });

export interface ReconciliationOutcome {
  table: string;
  diverged_before: number;
  delivered_from_hub: number;
  delivered_to_hub: number;
  left_for_review: string[];
  /** Measured after the repair, not predicted before it. */
  diverged_after: number;
  audit: string[];
}

/** Repair what may be repaired. Financial rows both sides hold with different
 *  contents are refused by the engine whatever the caller's role. */
export const reconciliationRun = (
  sessionToken: SessionToken,
  table?: string,
): Promise<ReconciliationOutcome[]> =>
  invoke("reconciliation_run", { sessionToken, table: table ?? null });

// ── Market prices ─────────────────────────────────────────────────────────────
//
// Every one of these reads except `marketPriceConfirmMatch`, which records a
// human judgement about which listing is this product — never a price. There is
// deliberately no binding that writes a selling price from a competitor's
// number: the panel fills the price box and the operator saves through
// `updateProductPrice`, which carries the RBAC, the confirmation and the audit
// trail.

export interface MarketObservation {
  /** The pairing this price came from, so a confirmation made in error can be
   *  withdrawn from the panel that shows its consequence. */
  match_id: string;
  retailer_name: string;
  price_minor: number;
  in_stock: boolean;
  source_url: string | null;
  observed_at: string;
}

export interface MarketSummary {
  low_minor: number | null;
  median_minor: number | null;
  high_minor: number | null;
  /** How many retailers the figures are drawn from. One retailer is not a market. */
  retailer_count: number;
  observed_at: string | null;
}

export interface MarketCandidate {
  source_id: string;
  source_product_key: string;
  name: string;
  pack_text: string | null;
  url: string;
  /** 0–100, shown so "almost certainly" and "possibly" do not look alike. */
  confidence: number;
  /** On an aggregator the retailer is a third party — "Al Helli" read from
   *  Akelny — so it is carried per offer rather than taken from the source. */
  offers: { retailer: string; price_minor: number; in_stock: boolean; url: string | null }[];
}

export interface MarketSourceStatus {
  source_id: string;
  name: string;
  status: string;
  reason: string | null;
  fallback_source_id: string | null;
}

export interface MarketPriceReport {
  product_id: string;
  product_name: string;
  /** Confirmed matches only. Safe to quote. */
  trusted: MarketObservation[];
  summary: MarketSummary;
  /** Look right, nobody has approved them. Kept separate from `trusted` on
   *  purpose — one list with a boolean is a careless `.map()` away from being
   *  summed together. */
  candidates: MarketCandidate[];
  /** Sources that could not be consulted, so a thin result reads as "we could
   *  not look" rather than "nobody else sells this". */
  unavailable: MarketSourceStatus[];
  /** On the refresh watchlist. Carried here so the toggle shows the state that
   *  exists rather than the one it last set. */
  tracked: boolean;
}

/** What is already known, without touching the network. */
export const marketPriceCached = (
  sessionToken: SessionToken,
  productId: string,
): Promise<MarketPriceReport> =>
  invoke("market_price_cached", { sessionToken, productId });

/** Go and look. Slow by nature — it is fetching from other retailers. */
export const marketPriceSearch = (
  sessionToken: SessionToken,
  productId: string,
): Promise<MarketPriceReport> =>
  invoke("market_price_search", { sessionToken, productId });

/** Record that a candidate really is this product. The candidate is passed back
 *  verbatim so the pairing is stored against the listing that was looked at. */
export const marketPriceConfirmMatch = (
  sessionToken: SessionToken,
  productId: string,
  candidate: MarketCandidate,
): Promise<MarketPriceReport> =>
  invoke("market_price_confirm_match", { sessionToken, productId, candidate });

export const marketPriceRejectMatch = (
  sessionToken: SessionToken,
  matchId: string,
): Promise<void> => invoke("market_price_reject_match", { sessionToken, matchId });

export const marketPriceHistory = (
  sessionToken: SessionToken,
  productId: string,
  limit?: number,
): Promise<MarketObservation[]> =>
  invoke("market_price_history", { sessionToken, productId, limit: limit ?? null });

export const marketSourceStatus = (
  sessionToken: SessionToken,
): Promise<MarketSourceStatus[]> => invoke("market_source_status", { sessionToken });

export const marketWatchlistSet = (
  sessionToken: SessionToken,
  productId: string,
  tracked: boolean,
): Promise<boolean> => invoke("market_watchlist_set", { sessionToken, productId, tracked });
