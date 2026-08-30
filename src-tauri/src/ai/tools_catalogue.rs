use crate::ai::client::ToolDef;
use crate::ai::engine::ops::{
    BulkPriceAdjust, BulkProductArchive, BulkPromotionApply, BulkPromotionRemove,
    BulkReorderPointUpdate, BulkStockSet, BulkStockVarianceFix, BulkSupplierPriceSync, Operation,
    ProductCreate,
};
use serde_json::{json, Value};
use std::sync::OnceLock;

// STACK-OVERFLOW GUARD: this catalogue was previously ONE ~1,100-line function
// building ~200 `json!` definitions in a single `vec![]`. Under the release
// profile (lto="fat", codegen-units=1) LLVM laid all the temporaries out in a
// single enormous stack frame, and the first call — when the admin sends an AI
// message — blew the thread stack (Windows exception 0xc00000fd, app closes
// instantly). It is now split into #[inline(never)] chunks so each frame stays
// small. Do NOT merge the chunks back together or remove #[inline(never)].
static TOOL_DEFINITIONS: OnceLock<Vec<ToolDef>> = OnceLock::new();

pub fn all_tool_definitions() -> Vec<ToolDef> {
    TOOL_DEFINITIONS.get_or_init(build_tool_definitions).clone()
}

#[inline(never)]
fn build_tool_definitions() -> Vec<ToolDef> {
    let mut tools: Vec<ToolDef> = Vec::new();
    tools.extend(defs_core_and_inventory());
    tools.extend(defs_customers_web());
    tools.extend(defs_roles_users_tax());
    tools.extend(defs_settings_sync());
    tools.extend(defs_shift_bulk_engine());
    tools.extend(defs_extensions());
    tools.extend(defs_interactive_input());
    tools.extend(defs_bulk_system_analytics());
    finish_tool_definitions(tools)
}

#[inline(never)]
fn defs_core_and_inventory() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "get_today_summary".into(),
            description: "Get today's sales summary including totals, transaction count, and payment breakdown.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        ToolDef {
            name: "list_products".into(),
            description: "List all active products with their current prices.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        ToolDef {
            name: "search_products".into(),
            description: "Search products by name, SKU, or barcode.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search term" }
                },
                "required": ["query"]
            }),
        },
        ToolDef {
            name: "get_product".into(),
            description: "Get full details of a specific product by ID.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "product_id": { "type": "string" }
                },
                "required": ["product_id"]
            }),
        },
        ToolDef {
            name: "update_product_price".into(),
            description: "Update the selling price of a product.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "product_id": { "type": "string" },
                    "new_price_minor": { "type": "integer", "description": "New price in minor units (e.g. 1500 = BHD 1.500)" },
                    "reason": { "type": "string", "description": "Reason for price change" }
                },
                "required": ["product_id", "new_price_minor"]
            }),
        },
        ToolDef {
            name: "set_product_active".into(),
            description: "Enable or disable a product. Disabled products do not appear in POS.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "product_id": { "type": "string" },
                    "is_active": { "type": "boolean" }
                },
                "required": ["product_id", "is_active"]
            }),
        },
        ToolDef {
            name: "update_product_name".into(),
            description: "Rename a product.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "product_id": { "type": "string" },
                    "new_name": { "type": "string" }
                },
                "required": ["product_id", "new_name"]
            }),
        },
        // ── Inventory tools ───────────────────────────────────────────────────
        ToolDef {
            name: "get_stock_levels".into(),
            description: "List all inventory-tracked products with their current stock quantity, reorder point, and low-stock status.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        ToolDef {
            name: "get_low_stock".into(),
            description: "List only the products that are at or below their reorder point (low stock or out of stock).".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        ToolDef {
            name: "adjust_stock".into(),
            description: "Apply a positive or negative quantity adjustment to a product's stock. Use for corrections, write-offs, or manual receives.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "product_id": { "type": "string" },
                    "quantity_delta": { "type": "number", "description": "Amount to add (positive) or remove (negative)" },
                    "notes": { "type": "string", "description": "Reason for adjustment" }
                },
                "required": ["product_id", "quantity_delta"]
            }),
        },
        ToolDef {
            name: "stock_take".into(),
            description: "Set a product's stock to an exact counted quantity from a physical stock take.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "product_id": { "type": "string" },
                    "new_quantity": { "type": "number", "description": "The counted quantity on hand" },
                    "notes": { "type": "string", "description": "Optional notes" }
                },
                "required": ["product_id", "new_quantity"]
            }),
        },
        ToolDef {
            name: "get_cash_summary".into(),
            description: "Get the current cash drawer reconciliation for the active shift: opening float, cash sales, refunds, paid-in/out, safe drops, expected total, and counted total if entered.".into(),
            input_schema: json!({ "type": "object", "properties": { "shift_id": { "type": "string" } }, "required": ["shift_id"] }),
        },
        ToolDef {
            name: "get_recent_refunds".into(),
            description: "List the most recent refunds (up to 20). Shows refund ID, original sale, amount, reason, and date.".into(),
            input_schema: json!({ "type": "object", "properties": { "limit": { "type": "integer", "description": "Max refunds to return (default 10, max 20)" } }, "required": [] }),
        },
        ToolDef {
            name: "get_audit_log".into(),
            description: "Retrieve recent audit log entries for today. Useful for reviewing cashier actions, voids, and refunds.".into(),
            input_schema: json!({ "type": "object", "properties": { "event_type": { "type": "string", "description": "Optional filter by event type, e.g. sale.created, sale.voided, CART_VOID, NO_SALE, X_REPORT, refund.created" } }, "required": [] }),
        },
        ToolDef {
            name: "get_sync_status".into(),
            description: "Check the multi-terminal sync status: whether the LAN hub is configured, last sync time, pending queue count, and any failed or conflicted events.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        ToolDef {
            name: "get_sync_diagnostics".into(),
            description: "Full sync diagnostics: per-table breakdown showing pending vs stuck (attempts>=10) rows, max/avg attempt counts, last error, hub connection state. Use when sync appears stuck or when debugging why events aren't syncing.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        ToolDef {
            name: "get_system_health_check".into(),
            description: "Complete ZANPOS system health check: database integrity, migration status, terminal/device records, hub mode, sync queue health, stuck AI runs/actions, and recommended fix actions. Use for troubleshooting database, terminal, hub, sync, or system-side issues.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        // ── Extended analytics & audit tools ──────────────────────────────────
        ToolDef {
            name: "get_daily_report".into(),
            description: "Get sales summary for a specific date (YYYY-MM-DD). Use for historical reports or comparing days.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "date": { "type": "string", "description": "Date in YYYY-MM-DD format" }
                },
                "required": ["date"]
            }),
        },
        ToolDef {
            name: "get_date_range_report".into(),
            description: "Get aggregated sales summary for a date range. Both dates inclusive. Max 90 days.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "from": { "type": "string", "description": "Start date YYYY-MM-DD (inclusive)" },
                    "to":   { "type": "string", "description": "End date YYYY-MM-DD (inclusive)" }
                },
                "required": ["from", "to"]
            }),
        },
        ToolDef {
            name: "get_top_products".into(),
            description: "List top-selling products by revenue for the last N days. Use to see bestsellers.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "limit":       { "type": "integer", "description": "How many products to return (default 10, max 25)" },
                    "period_days": { "type": "integer", "description": "Lookback window in days (default 30)" }
                },
                "required": []
            }),
        },
        ToolDef {
            name: "get_shift_history".into(),
            description: "List recent cashier shifts with open/close times, opening float, cashier name, and total sales.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "description": "Number of shifts to return (default 10, max 30)" }
                },
                "required": []
            }),
        },
        ToolDef {
            name: "list_categories".into(),
            description: "List product categories with their IDs and names, optionally filtered by category name.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Optional category-name filter",
                        "maxLength": 200
                    }
                },
                "required": []
            }),
        },
        ToolDef {
            name: "list_safe_drops".into(),
            description: "List safe drop events for a shift (cash physically removed from drawer for security).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "shift_id": { "type": "string", "description": "The shift ID to query" }
                },
                "required": ["shift_id"]
            }),
        },
        ToolDef {
            name: "list_no_sale_events".into(),
            description: "List no-sale (drawer opened without a transaction) events for a shift. Key audit signal.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "shift_id": { "type": "string", "description": "The shift ID to query" }
                },
                "required": ["shift_id"]
            }),
        },
        ToolDef {
            name: "get_audit_chain_status".into(),
            description: "Verify the SHA-256 hash chain integrity for audit logs on this device. Detects tampering or data loss.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        // ── Extended analytics ────────────────────────────────────────────────
        ToolDef {
            name: "get_hourly_sales".into(),
            description: "Break down today's sales by hour. Useful for identifying peak hours and staffing patterns.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        ToolDef {
            name: "get_sales_by_category".into(),
            description: "Show today's revenue and transaction count broken down by product category.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        ToolDef {
            name: "get_cashier_performance".into(),
            description: "Compare sales performance by cashier for a date range.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "from": { "type": "string", "description": "Start date YYYY-MM-DD" },
                    "to":   { "type": "string", "description": "End date YYYY-MM-DD" }
                },
                "required": ["from", "to"]
            }),
        },
        // ── Mutation: update reorder point ────────────────────────────────────
        ToolDef {
            name: "update_reorder_point".into(),
            description: "Update the reorder point (low-stock threshold) for a product. When stock falls to or below this number, a low-stock alert fires.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "product_id":    { "type": "string" },
                    "reorder_point": { "type": "number", "description": "New reorder threshold (e.g. 5 means alert when qty ≤ 5)" }
                },
                "required": ["product_id", "reorder_point"]
            }),
        },
        // ── Mutation: create product ──────────────────────────────────────────
        ToolDef {
            name: "create_product".into(),
            description: "Create a new product in the catalog with a name, price, and category.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name":        { "type": "string",  "description": "Product display name" },
                    "price_minor": { "type": "integer", "description": "Selling price in minor currency units (e.g. 1500 = BHD 1.500)" },
                    "category_id": { "type": "string",  "description": "Category ID — use list_categories to get valid IDs" },
                    "sku":         { "type": "string",  "description": "Optional SKU / product code" },
                    "barcode":     { "type": "string",  "description": "Optional barcode (EAN/UPC)" }
                },
                "required": ["name", "price_minor", "category_id"]
            }),
        },
    ]
}

#[inline(never)]
fn defs_customers_web() -> Vec<ToolDef> {
    vec![
        // ── Customer tools ────────────────────────────────────────────────────
        ToolDef {
            name: "list_customers".into(),
            description: "List customers, optionally filtered by name or phone. Shows loyalty points.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "search": { "type": "string", "description": "Optional name or phone search filter" }
                },
                "required": []
            }),
        },
        ToolDef {
            name: "get_customer".into(),
            description: "Get full details of a specific customer by ID including loyalty points, phone, email, and notes.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "customer_id": { "type": "string" }
                },
                "required": ["customer_id"]
            }),
        },
        ToolDef {
            name: "create_customer".into(),
            description: "Create a new customer record.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name":  { "type": "string", "description": "Customer full name" },
                    "phone": { "type": "string", "description": "Phone number (optional)" },
                    "email": { "type": "string", "description": "Email address (optional)" },
                    "notes": { "type": "string", "description": "Free-text notes (optional)" }
                },
                "required": ["name"]
            }),
        },
        ToolDef {
            name: "update_customer".into(),
            description: "Update an existing customer's contact details or notes.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "customer_id": { "type": "string" },
                    "name":        { "type": "string" },
                    "phone":       { "type": "string", "description": "Phone (blank to clear)" },
                    "email":       { "type": "string", "description": "Email (blank to clear)" },
                    "notes":       { "type": "string", "description": "Notes (blank to clear)" }
                },
                "required": ["customer_id", "name"]
            }),
        },
        // ── Delivery tools ────────────────────────────────────────────────────
        ToolDef {
            name: "list_deliveries".into(),
            description: "List delivery orders, optionally filtered by status. Shows rider, payment status, and amount.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "status": { "type": "string", "description": "Optional status filter: pending | in_transit | delivered | cancelled" },
                    "limit":  { "type": "integer", "description": "Max results (default 20, max 50)" }
                },
                "required": []
            }),
        },
        ToolDef {
            name: "advance_delivery_status".into(),
            description: "Advance a delivery order to the next status (pending → in_transit → delivered).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "delivery_id": { "type": "string" },
                    "new_status":  { "type": "string", "description": "Target status: in_transit | delivered | cancelled" }
                },
                "required": ["delivery_id", "new_status"]
            }),
        },
        // ── Bulk stock take ───────────────────────────────────────────────────
        ToolDef {
            name: "bulk_stock_take".into(),
            description: "Set exact stock counts for multiple products at once from a physical count. More efficient than individual stock_take calls.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
                        "minItems": 1,
                        "description": "List of product stock counts",
                        "items": {
                            "type": "object",
                            "properties": {
                                "product_id":   { "type": "string" },
                                "new_quantity": { "type": "number" }
                            },
                            "required": ["product_id", "new_quantity"]
                        }
                    }
                },
                "required": ["items"]
            }),
        },
        // ── Staff management (read-only) ──────────────────────────────────────
        ToolDef {
            name: "list_users".into(),
            description: "List all staff accounts with their roles (cashier, manager, owner) and active status.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        // ── Stock movements ───────────────────────────────────────────────────
        ToolDef {
            name: "get_stock_movements".into(),
            description: "View the stock movement history for a specific product: sales, adjustments, receives, stock-takes.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "product_id": { "type": "string" },
                    "limit":      { "type": "integer", "description": "Max movements to return (default 20, max 50)" }
                },
                "required": ["product_id"]
            }),
        },
        // ── Reports (extended) ────────────────────────────────────────────────
        ToolDef {
            name: "get_tax_report".into(),
            description: "Get daily tax collection totals for a date range. Useful for VAT reporting.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "from": { "type": "string", "description": "Start date YYYY-MM-DD" },
                    "to":   { "type": "string", "description": "End date YYYY-MM-DD" }
                },
                "required": ["from", "to"]
            }),
        },
        // ── Free web search (DuckDuckGo — no API key) ─────────────────────────
        ToolDef {
            name: "web_search".into(),
            description: "Search the internet for current information using DuckDuckGo. Free to use. Use for price research, product availability, competitor pricing, supplier information, or any public information not in the POS database. Returns titles, snippets, and links.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query string" },
                    "max_results": { "type": "integer", "description": "Max results to return (default 5, max 10)" }
                },
                "required": ["query"]
            }),
        },
        ToolDef {
            name: "search_market_prices".into(),
            description: "Search for current local market prices of a specific product (in Bahrain by default). Automatically crafts a focused price-comparison web search. Use when admin asks about competitor pricing, fair market value, or supplier prices.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "product_name": { "type": "string", "description": "Product or item to price-check" },
                    "location":     { "type": "string", "description": "Market location (default: Bahrain)" }
                },
                "required": ["product_name"]
            }),
        },
        // ── Free URL reader (Jina.ai — no API key) ────────────────────────────
        ToolDef {
            name: "fetch_url".into(),
            description: "Fetch any public webpage and return its content as clean readable text. Use AFTER web_search to read full product pages, supplier websites, price lists, or news articles. Works on most public sites. Free, no API key. Example: fetch a supermarket product page to get exact price.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "Full URL to fetch (must start with http:// or https://)" }
                },
                "required": ["url"]
            }),
        },
        // ── Barcode / product lookup (Open Food Facts — no API key) ───────────
        ToolDef {
            name: "lookup_barcode".into(),
            description: "Look up a product by its barcode (UPC/EAN). Returns product name, brand, categories, and nutrition info from the Open Food Facts open database. Free, no API key. Useful for verifying product names and details when restocking.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "barcode": { "type": "string", "description": "UPC or EAN barcode number (digits only)" }
                },
                "required": ["barcode"]
            }),
        },
        // ── Live currency rates (Frankfurter ECB — no API key) ────────────────
        ToolDef {
            name: "get_exchange_rates".into(),
            description: "Get today's live foreign exchange rates relative to BHD (Bahraini Dinar). Useful for calculating import costs, comparing prices with international suppliers, or converting USD/EUR quotes. Rates from the European Central Bank via Frankfurter. Free, no API key.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "currencies": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Currency codes to include (e.g. [\"USD\",\"EUR\",\"SAR\"]). Leave empty for all major currencies."
                    }
                },
                "required": []
            }),
        },
        // ── Prayer times (Aladhan — no API key) ───────────────────────────────
        ToolDef {
            name: "get_prayer_times".into(),
            description: "Get today's Islamic prayer times for Manama, Bahrain. Useful for scheduling staff breaks, planning shift handovers, or checking store operating hours around prayer times. Free, no API key.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "date": { "type": "string", "description": "Date in DD-MM-YYYY format (default: today)" }
                },
                "required": []
            }),
        },
        // ── Bahrain public holidays (nager.date — no API key) ─────────────────
        ToolDef {
            name: "get_bahrain_holidays".into(),
            description: "Get the list of official Bahrain public holidays for a given year. Useful for planning promotions, staffing, and forecasting slow/busy periods. Free, no API key.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "year": { "type": "integer", "description": "Year (default: current year)" }
                },
                "required": []
            }),
        },
    ]
}

#[inline(never)]
fn defs_roles_users_tax() -> Vec<ToolDef> {
    vec![
        // ── Roles & Tax Rules (read) ───────────────────────────────────────────
        ToolDef {
            name: "list_roles".into(),
            description: "List all staff roles available in the system (e.g. cashier, manager, owner). Use before creating a user so you can pick the right role_id.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        ToolDef {
            name: "list_tax_rules".into(),
            description: "List all tax rules with their IDs, names, rates, and inclusive/exclusive status. Use to find the correct tax_rule_id when creating or updating products.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        ToolDef {
            name: "get_store_settings".into(),
            description: "Get the active branch/store configuration: name, address, phone, tax number, CR number, timezone, receipt header/footer texts.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        ToolDef {
            name: "get_business_rules".into(),
            description: "Get the current business rule flags: allow_negative_stock, require_discount_reason, cashier_can_discount, auto_print_receipt.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        ToolDef {
            name: "list_devices".into(),
            description: "List all registered POS terminal devices with their codes, active status, and last seen timestamps.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        // ── Category mutations ──────────────────────────────────────────────────
        ToolDef {
            name: "create_category".into(),
            description: "Create a new product category.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Category display name" },
                    "sort_order": { "type": "integer", "description": "Display order (default 0)" }
                },
                "required": ["name"]
            }),
        },
        ToolDef {
            name: "update_category".into(),
            description: "Update an existing category's name, sort order, or active status.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "category_id": { "type": "string" },
                    "name": { "type": "string", "description": "New display name" },
                    "sort_order": { "type": "integer", "description": "New display order" },
                    "is_active": { "type": "boolean", "description": "Enable or disable this category" }
                },
                "required": ["category_id"]
            }),
        },
        // ── User/staff mutations ────────────────────────────────────────────────
        ToolDef {
            name: "create_user".into(),
            description: "Create a new staff account (cashier, manager, or owner). PIN must be 4+ digits.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "display_name": { "type": "string", "description": "Staff display name" },
                    "username": { "type": "string", "description": "Login username (lowercase, no spaces)" },
                    "pin": { "type": "string", "description": "Login PIN, 4–6 digits" },
                    "role_id": { "type": "string", "description": "Role ID — use list_roles to get valid IDs" }
                },
                "required": ["display_name", "username", "pin", "role_id"]
            }),
        },
        ToolDef {
            name: "update_user".into(),
            description: "Update a staff account: change display name, role, active status, or reset PIN.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "user_id": { "type": "string" },
                    "display_name": { "type": "string", "description": "New display name" },
                    "role_id": { "type": "string", "description": "New role ID" },
                    "is_active": { "type": "boolean", "description": "Enable or disable this account" },
                    "pin": { "type": "string", "description": "New PIN (4–6 digits). Omit to keep current PIN." }
                },
                "required": ["user_id"]
            }),
        },
        // ── Tax rule mutations ─────────────────────────────────────────────────
        ToolDef {
            name: "create_tax_rule".into(),
            description: "Create a new tax rule (for example, 10% VAT inclusive).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Tax rule name (e.g. 'VAT 10%')" },
                    "rate_basis_points": { "type": "integer", "description": "Tax rate in basis points (e.g. 1000 for 10%)" },
                    "inclusive": { "type": "boolean", "description": "True if prices include tax, false if tax is added on top" }
                },
                "required": ["name", "rate_basis_points"]
            }),
        },
        ToolDef {
            name: "update_tax_rule".into(),
            description: "Update an existing tax rule's name, rate, inclusive flag, or active status.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "tax_rule_id": { "type": "string" },
                    "name": { "type": "string", "description": "New tax rule name" },
                    "rate_basis_points": { "type": "integer", "description": "New tax rate in basis points (e.g. 1000 for 10%)" },
                    "inclusive": { "type": "boolean", "description": "Price includes tax?" },
                    "is_active": { "type": "boolean", "description": "Enable or disable this tax rule" }
                },
                "required": ["tax_rule_id"]
            }),
        },
        // ── Holistic product update ────────────────────────────────────────────
        ToolDef {
            name: "update_product_full".into(),
            description: "Update all product fields at once: name, category, SKU, barcode, price, tax rule, inventory tracking, reorder point, and active status. Use this instead of multiple individual product mutations.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "product_id": { "type": "string" },
                    "name": { "type": "string", "description": "New display name" },
                    "category_id": { "type": "string", "description": "New category ID" },
                    "sku": { "type": "string", "description": "New SKU code" },
                    "barcode": { "type": "string", "description": "New primary barcode" },
                    "price_minor": { "type": "integer", "description": "New selling price in minor units" },
                    "cost_minor": { "type": "integer", "description": "New product cost in minor units" },
                    "default_supplier_id": { "type": "string", "description": "Default supplier ID" },
                    "tax_rule_id": { "type": "string", "description": "Tax rule ID (use list_tax_rules to find IDs). Pass empty string to remove." },
                    "track_inventory": { "type": "boolean", "description": "Enable inventory tracking" },
                    "allow_decimal_quantity": { "type": "boolean", "description": "Allow fractional quantities" },
                    "reorder_point": { "type": "number", "description": "Low-stock alert threshold" },
                    "is_active": { "type": "boolean", "description": "Show in POS?" }
                },
                "required": ["product_id"]
            }),
        },
    ]
}

#[inline(never)]
fn defs_settings_sync() -> Vec<ToolDef> {
    vec![
        // ── Store settings mutation ────────────────────────────────────────────
        ToolDef {
            name: "update_store_settings".into(),
            description: "Update the active store or branch settings: name, address, phone, tax number, CR number, and receipt header or footer.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Store/business name" },
                    "address": { "type": "string", "description": "Store address" },
                    "phone": { "type": "string", "description": "Contact phone number" },
                    "tax_number": { "type": "string", "description": "Tax/VAT registration number" },
                    "cr_number": { "type": "string", "description": "Commercial Registration number" },
                    "receipt_header": { "type": "string", "description": "Text printed at top of receipts" },
                    "receipt_footer": { "type": "string", "description": "Text printed at bottom of receipts" },
                    "timezone": { "type": "string", "description": "IANA timezone (e.g. Asia/Bahrain). Use get_store_settings to see current value." }
                },
                "required": []
            }),
        },
        // ── Business rules mutation ────────────────────────────────────────────
        ToolDef {
            name: "update_business_rules".into(),
            description: "Update business-operation rule toggles.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "allow_negative_stock": { "type": "boolean", "description": "Allow sales even when stock goes below zero" },
                    "require_discount_reason": { "type": "boolean", "description": "Force cashiers to enter a reason when applying discounts" },
                    "cashier_can_discount": { "type": "boolean", "description": "Allow cashiers to apply discounts without manager override" },
                    "auto_print_receipt": { "type": "boolean", "description": "Automatically print receipt after each sale" }
                },
                "required": []
            }),
        },
        // ── Delivery payment management ────────────────────────────────────────
        ToolDef {
            name: "confirm_delivery_payment".into(),
            description: "Record that a customer has paid for a delivery order and mark the delivery as paid.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "delivery_id": { "type": "string" },
                    "payment_reference": { "type": "string", "description": "Optional payment reference number" },
                    "payment_note": { "type": "string", "description": "Optional note about the payment" }
                },
                "required": ["delivery_id"]
            }),
        },
        ToolDef {
            name: "cancel_delivery".into(),
            description: "Cancel a delivery order by setting its status to cancelled.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "delivery_id": { "type": "string" }
                },
                "required": ["delivery_id"]
            }),
        },
        // ── Session timeout ────────────────────────────────────────────────────
        ToolDef {
            name: "get_session_timeout".into(),
            description: "Get the current idle session timeout in minutes (0 = never auto-lock).".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        // ── DB backup ──────────────────────────────────────────────────────────
        ToolDef {
            name: "backup_database".into(),
            description: "Create a full database backup in the system backup directory. Use before risky bulk changes or when required by a recovery workflow.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        // ── Smart barcode lookup (OFFF + web fallback) ─────────────────────────
        ToolDef {
            name: "smart_barcode_lookup".into(),
            description: "Enhanced barcode lookup: first tries Open Food Facts, then falls back to web search. Returns product name, brand, category suggestion, size/quantity, and typical images. Use this when scanning a barcode for a product that isn't in your database yet — it gathers everything needed to create the product. Free, no API key.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "barcode": { "type": "string", "description": "UPC or EAN barcode number (8-14 digits)" }
                },
                "required": ["barcode"]
            }),
        },
        // ── Multi-store price comparison ────────────────────────────────────────
        ToolDef {
            name: "compare_store_prices".into(),
            description: "Search multiple known Bahrain retailers simultaneously for a product's price. Checks Lulu Hypermarket, Carrefour Bahrain, Alosra Supermarket, Talabat Mart, and general web sources. Returns a structured price comparison with store names, prices, and source URLs. Use to find the best local market price for any product. Free, no API key.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "product_name": { "type": "string", "description": "Product name to price-check (e.g. 'Nido milk powder 900g')" },
                    "location": { "type": "string", "description": "Market location (default: Bahrain)" }
                },
                "required": ["product_name"]
            }),
        },
        // ── Bahrain food/grocery delivery search ────────────────────────────────
        ToolDef {
            name: "bahrain_market_price_check".into(),
            description: "Search Bahrain grocery delivery platforms (Talabat, Talabat Mart, Lulu Online) for a specific product. Use to find current in-market prices on platforms Bahrain consumers actually order from. Returns store name, price, and product availability. Free, no API key.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "product_name": { "type": "string", "description": "Product to find (e.g. 'Almarai milk 2L')" },
                    "max_results": { "type": "integer", "description": "Max results per source (default 3, max 5)" }
                },
                "required": ["product_name"]
            }),
        },
        // ── Parity: is every terminal actually holding the same data ─────────
        ToolDef {
            name: "check_terminal_parity".into(),
            description: "Compare every synced table on this terminal against the hub and report which ones differ, with a 0-100 parity score and the row counts on each side. Answers 'is this till showing the same catalogue and prices as the others'. Read-only. Follow a mismatch with find_diverged_rows on the named table to get the actual row IDs rather than resyncing everything.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        ToolDef {
            name: "find_diverged_rows".into(),
            description: "Name the exact rows that differ between this terminal and the hub for one table, and say which side is missing or stale for each. Use after check_terminal_parity reports a mismatch — this is what turns 'products differs' on a 28,000-row catalogue into a short list of product IDs. Each row is reported as missing_locally (a pull that never landed), missing_on_hub (a push still queued or lost), or different (both hold it and the contents disagree). Read-only.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "table": { "type": "string", "description": "A synced table, e.g. products, product_prices, categories, customers, stock_levels." }
                },
                "required": ["table"]
            }),
        },
        ToolDef {
            name: "preview_reconciliation".into(),
            description: "Say what a reconciliation of one table would repair and what it would refuse, without changing anything. Use before any resync. Rows only one side holds can be delivered safely — nothing is overwritten. Rows both sides hold with different contents are reported separately and must not be repaired automatically, because whichever copy loses may be a real edit; for sales, payments, refunds, cash events and shifts that copy may be the only record of a transaction. Read-only.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "table": { "type": "string", "description": "A synced table, e.g. products, product_prices, customers, sales, payments." }
                },
                "required": ["table"]
            }),
        },
        ToolDef {
            name: "get_terminal_roster".into(),
            description: "List every till registered to this branch with its device code, status, whether it is active, and when it was last seen. Use to answer 'which terminals are there' and to spot one that has not checked in — a till that stopped syncing days ago is the usual reason two screens disagree.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        ToolDef {
            name: "get_catalogue_parity_summary".into(),
            description: "Product-focused parity: how many products, barcodes and prices this terminal holds versus the hub, and whether the catalogue checksums agree. Use for 'does this till have the same product list as the others' without dumping the catalogue.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        // ── Sync repair tools ─────────────────────────────────────────────────
        ToolDef {
            name: "sync_reset_stuck".into(),
            description: "Reset ALL stuck rows (sync_attempts >= 10) across all tables back to pending with 0 attempts. Use when sync diagnostic shows stuck events blocking the queue. After fixing the root cause (e.g. reconnecting to the hub in Settings → Hub), call this to unblock sync.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        ToolDef {
            name: "sync_queue_list".into(),
            description: "List up to 200 pending/failed sync events with their table, entity ID, status, attempt count, and error message. Use to inspect individual stuck events.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        ToolDef {
            name: "sync_queue_retry".into(),
            description: "Retry a specific failed sync event by its composite ID (format: table:entity_id). Resets attempts to 0 and status to pending.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "event_id": { "type": "string", "description": "Composite event ID in format 'table:entity_id' (e.g. 'shifts:01KTEVBEKM91V1YE5TK6MKFS49')" }
                },
                "required": ["event_id"]
            }),
        },
        ToolDef {
            name: "sync_queue_dismiss".into(),
            description: "Dismiss a pending/failed sync event — marks it as 'synced' so it stops retrying. Use for events that can't or shouldn't be synced (e.g., test data, duplicate rows).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "event_id": { "type": "string", "description": "Composite event ID in format 'table:entity_id'" }
                },
                "required": ["event_id"]
            }),
        },
    ]
}

#[inline(never)]
fn defs_shift_bulk_engine() -> Vec<ToolDef> {
    vec![
        // ── Shift management tools ────────────────────────────────────────────
        ToolDef {
            name: "get_active_shift".into(),
            description: "Check the currently active (open) shift for the POS terminal. Returns shift ID, cashier, opening time, opening float, and status.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        // ── Void / Refund tools ───────────────────────────────────────────────
        ToolDef {
            name: "void_sale".into(),
            description: "Void a completed sale by receipt number. Requires manager/owner PIN. The sale must exist and not already be voided. This is irreversible!".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "receipt_number": { "type": "string", "description": "Receipt number to void" },
                    "reason": { "type": "string", "description": "Reason for voiding (required)" }
                },
                "required": ["receipt_number", "reason"]
            }),
        },
        // ── Delete / Deactivate tools ─────────────────────────────────────────
        ToolDef {
            name: "delete_customer".into(),
            description: "Permanently delete a customer record. USE WITH CAUTION — this removes the customer and all their loyalty points. Consider deactivating instead.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "customer_id": { "type": "string", "description": "Customer ID to delete" }
                },
                "required": ["customer_id"]
            }),
        },
        ToolDef {
            name: "set_device_active".into(),
            description: "Activate or deactivate a POS terminal device. Deactivated devices cannot log in or process sales.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "device_id": { "type": "string", "description": "Device ID to modify" },
                    "is_active": { "type": "boolean", "description": "True to activate, false to deactivate" }
                },
                "required": ["device_id", "is_active"]
            }),
        },
        // ── Stock / Inventory tools ───────────────────────────────────────────
        ToolDef {
            name: "receive_stock".into(),
            description: "Receive incoming stock for a product — adds quantity to on-hand and creates a stock movement record with receipt notes (PO number, supplier info).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "product_id": { "type": "string", "description": "Product ID to receive stock for" },
                    "quantity": { "type": "string", "description": "Quantity to receive (e.g. '5' or '2.5')" },
                    "expiry_date": { "type": "string", "description": "Optional received-lot expiry date in YYYY-MM-DD format" },
                    "notes": { "type": "string", "description": "Optional notes (PO number, supplier, batch, etc.)" }
                },
                "required": ["product_id", "quantity"]
            }),
        },
        // ── Loyalty / Customer tools ──────────────────────────────────────────
        ToolDef {
            name: "add_loyalty_points".into(),
            description: "Add loyalty points to a customer's account. Points can be used for rewards or discounts.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "customer_id": { "type": "string", "description": "Customer ID" },
                    "points": { "type": "integer", "description": "Number of points to add (positive or negative)" }
                },
                "required": ["customer_id", "points"]
            }),
        },
        // ── Bulk tools ────────────────────────────────────────────────────────
        ToolDef {
            name: "bulk_update_prices".into(),
            description: "Bulk update selling prices for multiple products at once. Provide a list of product_id:price_minor pairs. Prices in fils (1000 fils = 1 BHD).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "updates": { "type": "array", "minItems": 1, "items": {
                        "type": "object",
                        "properties": {
                            "product_id": { "type": "string" },
                            "price_minor": { "type": "integer", "description": "New price in fils (minor units)" }
                        },
                        "required": ["product_id", "price_minor"]
                    }, "description": "Array of {product_id, price_minor} pairs" }
                },
                "required": ["updates"]
            }),
        },
        // ── Bulk engine ops ───────────────────────────────────────────────────
        ToolDef {
            name: "bulk_price_adjust".into(),
            description: "Increase, decrease, or set prices for ALL products matching a \
                category tree, text filter, or active-status filter. The selector field uses \
                category_subtree (category ID — includes all descendants), active (bool), or \
                text (name/sku/barcode LIKE). The adjustment is one of: \
                {\"mode\":\"Percent\",\"value\":20.0} for +20%, \
                {\"mode\":\"Absolute\",\"value\":-500} for -500 fils, or \
                {\"mode\":\"Set\",\"value\":5000} to set a fixed price in fils. \
                Always previews the count before executing — safe to call with large sets. \
                Example: increase all Toys > Girls prices 20% = \
                selector:{category_subtree:\"girls\"}, adjustment:{mode:\"Percent\",value:20.0}".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "object",
                        "description": "Product filter — at least one field recommended",
                        "properties": {
                            "category_subtree": { "type": "string", "description": "Category ID — matches this category and all subcategories recursively" },
                            "active": { "type": "boolean", "description": "true = active products only, false = inactive only" },
                            "text": { "type": "string", "description": "Case-insensitive filter on product name, SKU, or barcode" }
                        }
                    },
                    "adjustment": {
                        "type": "object",
                        "description": "Price change to apply — one of Percent / Absolute / Set",
                        "properties": {
                            "mode": { "type": "string", "enum": ["Percent", "Absolute", "Set"] },
                            "value": { "type": "number", "description": "Percent: e.g. 20.0 for +20%. Absolute: fils delta e.g. -500. Set: exact fils price e.g. 5000." }
                        },
                        "required": ["mode", "value"]
                    }
                },
                "required": ["selector", "adjustment"]
            }),
        },
    ]
}

/// The interactive-form tool, in its own chunk.
///
/// Its schema is by far the deepest in the catalogue, and the file-level
/// STACK-OVERFLOW GUARD above is about exactly this: `json!` temporaries all
/// land in one stack frame under fat LTO. Keep it here rather than folding it
/// into a neighbouring chunk.
#[inline(never)]
fn defs_interactive_input() -> Vec<ToolDef> {
    let cell_types = json!([
        "text", "textarea", "number", "money", "integer", "barcode", "select", "date", "toggle"
    ]);
    let options = json!({
        "type": "array",
        "description": "Required when type is select.",
        "items": { "type": "object", "properties": { "value": { "type": "string" }, "label": { "type": "string" } }, "required": ["value", "label"] }
    });
    vec![ToolDef {
        name: "request_input".into(),
        description: "Ask the operator for values using an on-screen form instead of asking in prose. \
Use it when the request names an operation but omits what it needs (\"price update\", \"add a customer\"), when a choice between a few concrete options decides what happens next, or when a document has been read and every extracted row must be checked and corrected before anything is written. \
Prefer a read tool over a form for anything the database already knows: forms are for values only the operator holds. \
Fields collect one value each; choices offer one-tap answers; table draws an editable grid, one row per line item, which is how an extracted purchase bill is confirmed. \
This performs no business action and changes nothing. Your turn ends the moment the form is shown, and the operator's answers arrive as their next message — read them and then carry out the operation with the normal tools.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "title": { "type": "string", "description": "Short heading, e.g. 'Update price'." },
                "note": { "type": "string", "description": "One line of context above the inputs. Use it to flag what needs attention, e.g. 'Two lines have no barcode.'" },
                "submit_label": { "type": "string", "description": "Verb for the submit button, e.g. 'Update price'. Defaults to 'Send'." },
                "fields": {
                    "type": "array",
                    "description": "Up to 12 single-value inputs. Prefill 'value' with anything already known so the operator only corrects it.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string", "description": "Machine name, lowercase letters, digits and underscores. This is the key the answer comes back under." },
                            "label": { "type": "string", "description": "What the operator reads." },
                            "type": { "type": "string", "enum": cell_types, "description": "money opens the numeric keypad and expects BHD with three decimals; barcode opens the scanner-friendly numeric field." },
                            "value": { "type": "string", "description": "Prefilled value." },
                            "placeholder": { "type": "string" },
                            "help": { "type": "string", "description": "Short hint under the input." },
                            "required": { "type": "boolean" },
                            "options": options
                        },
                        "required": ["name", "label", "type"]
                    }
                },
                "choices": {
                    "type": "array",
                    "description": "Up to 8 one-tap answers. Use for a decision, not for data entry. style 'primary' marks the recommended answer and 'danger' the destructive one.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "value": { "type": "string", "description": "What comes back when tapped." },
                            "label": { "type": "string" },
                            "detail": { "type": "string", "description": "Second line under the label." },
                            "style": { "type": "string", "enum": ["default", "primary", "danger"] }
                        },
                        "required": ["value", "label"]
                    }
                },
                "table": {
                    "type": "object",
                    "description": "An editable grid, up to 8 columns and 60 rows. Every cell is editable; fill rows with what was extracted so the operator confirms rather than retypes. Leave a cell empty to make the operator supply it.",
                    "properties": {
                        "columns": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "name": { "type": "string", "description": "Machine name; every row must use these keys and no others." },
                                    "label": { "type": "string" },
                                    "type": { "type": "string", "enum": cell_types },
                                    "required": { "type": "boolean", "description": "Empty cells in a required column block submission and are highlighted for the operator to fill." },
                                    "options": options
                                },
                                "required": ["name", "label", "type"]
                            }
                        },
                        "rows": {
                            "type": "array",
                            "description": "One object per line, keyed by column name. Values may be strings, numbers or booleans.",
                            "items": { "type": "object" }
                        },
                        "row_label": { "type": "string", "description": "Singular noun for a row, e.g. 'line'. Used on the add-row button." },
                        "allow_add": { "type": "boolean", "description": "Let the operator add a line the document was missing." },
                        "allow_remove": { "type": "boolean", "description": "Let the operator drop a line that should not be entered." }
                    },
                    "required": ["columns", "rows"]
                }
            },
            "required": ["title"]
        }),
    }]
}

#[inline(never)]
fn defs_extensions() -> Vec<ToolDef> {
    vec![
        // ── Extension read tools ──────────────────────────────────────────────
        ToolDef { name: "request_full_tool_access".into(), description: "Request the full mutation-tool catalogue on the next ZanAI step when the currently visible mutation tools cannot complete the operator's request. This changes available schemas only; it performs no business action.".into(), input_schema: json!({"type":"object","properties":{"reason":{"type":"string"}},"required":["reason"]}) },
        ToolDef { name: "report_expiring_stock".into(), description: "List received inventory lots expiring within a configurable number of days, ordered first-expired-first. Includes remaining quantity and a clearance-review suggestion; it never changes prices.".into(), input_schema: json!({"type":"object","properties":{"lead_days":{"type":"integer","minimum":0,"maximum":365}}}) },
        ToolDef { name: "get_margin_erosion".into(), description: "Find products whose latest supplier cost increased without a later shelf-price update. Reports margin loss in basis points and prominently reports recent sale lines with unknown cost.".into(), input_schema: json!({"type":"object","properties":{"threshold_basis_points":{"type":"integer","minimum":1,"maximum":10000}}}) },
        ToolDef { name: "get_seasonal_demand_plan".into(), description: "Build an advisory reorder plan from the store's own same-period sales in prior years. The operator supplies the period; no Ramadan or holiday dates are hardcoded.".into(), input_schema: json!({"type":"object","properties":{"from":{"type":"string","description":"Planned period start YYYY-MM-DD"},"to":{"type":"string","description":"Planned period end YYYY-MM-DD"},"comparison_years":{"type":"integer","minimum":1,"maximum":3}},"required":["from","to"]}) },
        ToolDef { name: "get_cash_flow_forecast".into(), description: "Show known near-term cash-delivery receivables against outstanding ordered/partial purchase-order commitments. Highlights PO lines with unknown cost; advisory only.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "get_sales_list".into(), description: "List sales for a date range. date_from, date_to (YYYY-MM-DD), optional limit (max 200).".into(), input_schema: json!({"type":"object","properties":{"date_from":{"type":"string"},"date_to":{"type":"string"},"limit":{"type":"integer"}}}) },
        ToolDef { name: "get_sale_detail".into(), description: "Get full detail of a single sale by receipt number, including all items and payments.".into(), input_schema: json!({"type":"object","properties":{"receipt_number":{"type":"string"}},"required":["receipt_number"]}) },
        ToolDef { name: "get_z_report".into(), description: "Get Z-report (end-of-day summary) for a specific date (YYYY-MM-DD). Shows revenue, payments, refunds.".into(), input_schema: json!({"type":"object","properties":{"date":{"type":"string"}}}) },
        ToolDef { name: "get_eod_cashup".into(), description: "Get end-of-day cashup summary for a specific date. Same as Z-report.".into(), input_schema: json!({"type":"object","properties":{"date":{"type":"string"}}}) },
        ToolDef { name: "get_x_report".into(), description: "Get X-report (intra-day cash summary) for a specific shift_id. Shows opening, sales, cash in/out, expected vs counted.".into(), input_schema: json!({"type":"object","properties":{"shift_id":{"type":"string"}},"required":["shift_id"]}) },
        ToolDef { name: "get_product_barcodes".into(), description: "List all extra barcodes registered for a product.".into(), input_schema: json!({"type":"object","properties":{"product_id":{"type":"string"}},"required":["product_id"]}) },
        ToolDef { name: "get_whatsapp_status".into(), description: "Check WhatsApp sidecar connection status (running/connected/disconnected).".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "get_branch_settings".into(), description: "Get the current branch/store configuration (name, address, VAT, receipt header/footer, phone, currency).".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "get_hub_status".into(), description: "Check LAN hub sync configuration: whether this device is the hub or a connected terminal, hub address, store token presence, pending rows, last sync.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "get_held_carts".into(), description: "List all parked/held carts on the current device.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "get_db_integrity".into(), description: "Run SQLite integrity check on the local database.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "get_thermal_config".into(), description: "Get the current thermal printer configuration (port, baud rate, enabled status).".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "get_delivery_detail".into(), description: "Get full detail of a delivery order by delivery_id.".into(), input_schema: json!({"type":"object","properties":{"delivery_id":{"type":"string"}},"required":["delivery_id"]}) },
        ToolDef { name: "get_rider_suggestions".into(), description: "Get a list of suggested rider names based on past deliveries.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "get_sync_queue_stats".into(), description: "Show per-table sync queue stats (pending rows, stuck rows) for all sync tables.".into(), input_schema: json!({"type":"object","properties":{}}) },
        // ── Extension mutation tools ──────────────────────────────────────────
        ToolDef { name: "create_refund".into(), description: "Process a full refund for a past sale by receipt number. Refunds all items.".into(), input_schema: json!({"type":"object","properties":{"receipt_number":{"type":"string"},"reason":{"type":"string"}},"required":["receipt_number"]}) },
        ToolDef { name: "create_cash_event".into(), description: "Record a cash event for a shift: event_type (paid_in/paid_out/safe_drop), amount_bhd, shift_id, optional note.".into(), input_schema: json!({"type":"object","properties":{"shift_id":{"type":"string"},"event_type":{"type":"string","enum":["paid_in","paid_out","safe_drop"]},"amount_bhd":{"type":"string"},"note":{"type":"string"}},"required":["shift_id","event_type","amount_bhd"]}) },
        ToolDef { name: "open_shift".into(), description: "Open a new cashier shift. Requires cashier_user_id and optional opening_cash_bhd.".into(), input_schema: json!({"type":"object","properties":{"cashier_user_id":{"type":"string"},"opening_cash_bhd":{"type":"string"}},"required":["cashier_user_id"]}) },
        ToolDef { name: "close_shift".into(), description: "Close a cashier shift by shift_id. Optional: counted_cash_bhd, notes.".into(), input_schema: json!({"type":"object","properties":{"shift_id":{"type":"string"},"counted_cash_bhd":{"type":"string"},"notes":{"type":"string"}},"required":["shift_id"]}) },
        ToolDef { name: "add_product_barcode".into(), description: "Register an additional barcode for a product.".into(), input_schema: json!({"type":"object","properties":{"product_id":{"type":"string"},"barcode":{"type":"string"}},"required":["product_id","barcode"]}) },
        ToolDef { name: "remove_product_barcode".into(), description: "Remove a barcode registration by barcode_id.".into(), input_schema: json!({"type":"object","properties":{"barcode_id":{"type":"string"}},"required":["barcode_id"]}) },
        ToolDef { name: "trigger_sync_now".into(), description: "Reset sync retry counters so the background sync worker picks up pending rows in the next cycle (within 30 seconds).".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "apply_system_health_fix".into(), description: "Apply a fix recommended by get_system_health_check: reset_stuck_sync, clear_stuck_ai_runs, clear_stuck_ai_actions, reconcile_stock_drift, or trigger_sync_now.".into(), input_schema: json!({"type":"object","properties":{"fix_action":{"type":"string","enum":["reset_stuck_sync","clear_stuck_ai_runs","clear_stuck_ai_actions","reconcile_stock_drift","trigger_sync_now"]}},"required":["fix_action"]}) },
        ToolDef { name: "force_full_resync".into(), description: "Force a complete resync: marks all synced rows as pending and resets all watermarks to epoch. Use when data is inconsistent with the hub.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "revert_delivery_payment".into(), description: "Reverse a delivery payment — set delivery payment_status from 'paid' back to 'unpaid'.".into(), input_schema: json!({"type":"object","properties":{"delivery_id":{"type":"string"},"reason":{"type":"string"}},"required":["delivery_id"]}) },
        ToolDef { name: "update_branch_settings".into(), description: "Update branch/store configuration. All fields optional; only provided fields are changed.".into(), input_schema: json!({"type":"object","properties":{"name":{"type":"string"},"timezone":{"type":"string"},"address":{"type":"string"},"phone":{"type":"string"},"receipt_header":{"type":"string"},"receipt_footer":{"type":"string"},"tax_number":{"type":"string"},"cr_number":{"type":"string"}}}) },
        ToolDef { name: "register_device".into(), description: "Register a new POS device/terminal. Requires device_code and name.".into(), input_schema: json!({"type":"object","properties":{"device_code":{"type":"string"},"name":{"type":"string"}},"required":["device_code","name"]}) },
        ToolDef { name: "send_whatsapp_delivery_alert".into(), description: "Send a WhatsApp delivery alert to a phone number. Optional custom message.".into(), input_schema: json!({"type":"object","properties":{"phone":{"type":"string"},"message":{"type":"string"}},"required":["phone"]}) },
        ToolDef { name: "send_whatsapp_payment_reminder".into(), description: "Send a WhatsApp payment reminder to a phone number. Optional custom message.".into(), input_schema: json!({"type":"object","properties":{"phone":{"type":"string"},"message":{"type":"string"}},"required":["phone"]}) },
        ToolDef { name: "send_whatsapp_arrival_notice".into(), description: "Send a WhatsApp order arrival notice to a phone number. Optional custom message.".into(), input_schema: json!({"type":"object","properties":{"phone":{"type":"string"},"message":{"type":"string"}},"required":["phone"]}) },
        ToolDef { name: "disconnect_whatsapp".into(), description: "Disconnect the active WhatsApp session from the sidecar.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "update_thermal_config".into(), description: "Update thermal printer configuration. Fields: port (e.g. COM3), baud (e.g. 9600), enabled (1/0).".into(), input_schema: json!({"type":"object","properties":{"port":{"type":"string"},"baud":{"type":"string"},"enabled":{"type":"string"}}}) },
        ToolDef { name: "open_cash_drawer".into(), description: "Send an ESC/POS pulse to physically open the cash drawer connected to the thermal printer.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "reprint_receipt".into(), description: "Reprint a past receipt by receipt_number to the configured thermal printer.".into(), input_schema: json!({"type":"object","properties":{"receipt_number":{"type":"string"}},"required":["receipt_number"]}) },
        ToolDef { name: "delete_held_cart".into(), description: "Permanently delete a held/parked cart by held_cart_id.".into(), input_schema: json!({"type":"object","properties":{"held_cart_id":{"type":"string"}},"required":["held_cart_id"]}) },
        ToolDef { name: "update_benefit_number".into(), description: "Update the Benefit/Sadad payment phone number stored in app config.".into(), input_schema: json!({"type":"object","properties":{"benefit_number":{"type":"string"}},"required":["benefit_number"]}) },
        ToolDef { name: "open_tab".into(), description: "Navigate the admin workspace to a destination accepted by the tab schema.".into(), input_schema: json!({"type":"object","properties":{"tab":{"type":"string","enum":["products","categories","inventory","reports","cashier","eod","deliveries","customers","users","purchasing","settings","audit","devices"]}},"required":["tab"]}) },
        ToolDef { name: "get_product_detail".into(), description: "Get full details of a single product including prices and stock levels.".into(), input_schema: json!({"type":"object","properties":{"product_id":{"type":"string"}},"required":["product_id"]}) },
        ToolDef { name: "get_sales_report".into(), description: "Get sales report for a date range with totals and breakdowns.".into(), input_schema: json!({"type":"object","properties":{"from_date":{"type":"string"},"to_date":{"type":"string"}},"required":["from_date","to_date"]}) },
        ToolDef { name: "get_cash_status".into(), description: "Current cash drawer status: paid in/out, safe drops.".into(), input_schema: json!({"type":"object","properties":{}}) },
        // ── Missing CRUD ───────────────────────────────────────────────────────
        ToolDef { name: "delete_product".into(), description: "Permanently delete a product. Blocked if the product has any sales history — in that case use set_product_active instead.".into(), input_schema: json!({"type":"object","properties":{"product_id":{"type":"string"}},"required":["product_id"]}) },
        ToolDef { name: "delete_category".into(), description: "Delete a category. Blocked if any products still belong to it.".into(), input_schema: json!({"type":"object","properties":{"category_id":{"type":"string"}},"required":["category_id"]}) },
        ToolDef { name: "delete_tax_rule".into(), description: "Delete a tax rule. Blocked if any products reference it.".into(), input_schema: json!({"type":"object","properties":{"tax_rule_id":{"type":"string"}},"required":["tax_rule_id"]}) },
        ToolDef { name: "delete_user".into(), description: "Delete a staff account. Blocked if they have shifts or sales on record.".into(), input_schema: json!({"type":"object","properties":{"user_id":{"type":"string"}},"required":["user_id"]}) },
        ToolDef { name: "update_delivery_details".into(), description: "Update the rider name, contact number, address, or notes on an existing delivery.".into(), input_schema: json!({"type":"object","properties":{"delivery_id":{"type":"string"},"delivery_staff_name":{"type":"string"},"contact_number":{"type":"string"},"address_text":{"type":"string"},"house_number":{"type":"string"},"area":{"type":"string"},"delivery_note":{"type":"string"}},"required":["delivery_id"]}) },
        ToolDef { name: "search_sales_by_customer".into(), description: "Find all sales linked to a specific customer_id. Returns receipt numbers, dates, and amounts.".into(), input_schema: json!({"type":"object","properties":{"customer_id":{"type":"string"},"limit":{"type":"integer"}},"required":["customer_id"]}) },
        ToolDef { name: "send_whatsapp_to_customer".into(), description: "Send a free-form WhatsApp message to a customer by phone number or customer_id.".into(), input_schema: json!({"type":"object","properties":{"phone":{"type":"string"},"customer_id":{"type":"string"},"message":{"type":"string","description":"Message text to send"}},"required":["message"]}) },
        ToolDef { name: "duplicate_product".into(), description: "Clone an existing product with a new name. Copies price, category, tax rule, and inventory settings. Useful for adding size/variant variants.".into(), input_schema: json!({"type":"object","properties":{"product_id":{"type":"string"},"new_name":{"type":"string"}},"required":["product_id","new_name"]}) },
        ToolDef { name: "find_products_without_barcode".into(), description: "List active products that have no barcode registered. Useful to identify items that can't be scanned at POS.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "reset_user_pin".into(), description: "Admin reset: set a new PIN for a staff account. PIN must be 4–6 digits.".into(), input_schema: json!({"type":"object","properties":{"user_id":{"type":"string"},"new_pin":{"type":"string","description":"New 4–6 digit PIN"}},"required":["user_id","new_pin"]}) },
        ToolDef { name: "lock_user".into(), description: "Temporarily disable a staff account (e.g. suspicious activity, failed PIN attempts). Account can be unlocked with unlock_user.".into(), input_schema: json!({"type":"object","properties":{"user_id":{"type":"string"}},"required":["user_id"]}) },
        ToolDef { name: "unlock_user".into(), description: "Re-enable a locked/inactive staff account.".into(), input_schema: json!({"type":"object","properties":{"user_id":{"type":"string"}},"required":["user_id"]}) },
        ToolDef { name: "get_user_permissions".into(), description: "List the effective permissions for a user based on their role.".into(), input_schema: json!({"type":"object","properties":{"user_id":{"type":"string"}},"required":["user_id"]}) },
        ToolDef { name: "create_customer_note".into(), description: "Attach or replace a free-text note on a customer record (allergies, preferences, account notes).".into(), input_schema: json!({"type":"object","properties":{"customer_id":{"type":"string"},"note":{"type":"string"}},"required":["customer_id","note"]}) },
        ToolDef { name: "get_customer_notes".into(), description: "Read the notes attached to a customer record.".into(), input_schema: json!({"type":"object","properties":{"customer_id":{"type":"string"}},"required":["customer_id"]}) },
        // ── Suppliers & Purchase Orders ────────────────────────────────────────
        ToolDef { name: "list_suppliers".into(), description: "List all suppliers/vendors. Optional name search filter.".into(), input_schema: json!({"type":"object","properties":{"search":{"type":"string"}}}) },
        ToolDef { name: "create_supplier".into(), description: "Add a new supplier/vendor with contact details.".into(), input_schema: json!({"type":"object","properties":{"name":{"type":"string"},"phone":{"type":"string"},"email":{"type":"string"},"contact_name":{"type":"string"},"address":{"type":"string"},"notes":{"type":"string"}},"required":["name"]}) },
        ToolDef { name: "update_supplier".into(), description: "Update supplier details. All fields optional; only provided fields are changed.".into(), input_schema: json!({"type":"object","properties":{"supplier_id":{"type":"string"},"name":{"type":"string"},"phone":{"type":"string"},"email":{"type":"string"},"contact_name":{"type":"string"},"address":{"type":"string"},"notes":{"type":"string"},"is_active":{"type":"boolean"}},"required":["supplier_id"]}) },
        ToolDef { name: "list_purchase_orders".into(), description: "List purchase orders. Filter by supplier_id, status (draft/ordered/partial/received/cancelled), or date range.".into(), input_schema: json!({"type":"object","properties":{"supplier_id":{"type":"string"},"status":{"type":"string"},"from":{"type":"string"},"to":{"type":"string"},"limit":{"type":"integer"}}}) },
        ToolDef { name: "create_purchase_order".into(), description: "Create a purchase order with line items (products, quantities, costs). Status starts as 'draft'.".into(), input_schema: json!({"type":"object","properties":{"supplier_id":{"type":"string"},"expected_date":{"type":"string"},"notes":{"type":"string"},"lines":{"type":"array","minItems":1,"items":{"type":"object","properties":{"product_id":{"type":"string"},"product_name":{"type":"string"},"ordered_qty":{"type":"number"},"unit_cost_minor":{"type":"integer"}},"required":["ordered_qty","unit_cost_minor"]}}},"required":["lines"]}) },
        ToolDef { name: "update_purchase_order".into(), description: "Update a PO status (ordered/partial/received/cancelled) or received quantities.".into(), input_schema: json!({"type":"object","properties":{"po_id":{"type":"string"},"status":{"type":"string","enum":["draft","ordered","partial","received","cancelled"]},"notes":{"type":"string"},"received_date":{"type":"string"}},"required":["po_id"]}) },
        // ── Analytics / Intelligence ───────────────────────────────────────────
        ToolDef { name: "get_dead_stock".into(), description: "Active products with zero units sold in the last N days. Identifies dead inventory tying up cash.".into(), input_schema: json!({"type":"object","properties":{"days":{"type":"integer","description":"Lookback window (default 30)"},"limit":{"type":"integer"}}}) },
        ToolDef { name: "get_discount_by_cashier".into(), description: "Total discounts given per cashier for a date range. Audit signal for excessive discounting.".into(), input_schema: json!({"type":"object","properties":{"from":{"type":"string"},"to":{"type":"string"}},"required":["from","to"]}) },
        ToolDef { name: "get_customer_purchase_history".into(), description: "All sales for a specific customer: receipt numbers, dates, amounts, and items.".into(), input_schema: json!({"type":"object","properties":{"customer_id":{"type":"string"},"limit":{"type":"integer"}},"required":["customer_id"]}) },
        ToolDef { name: "get_revenue_by_payment_method".into(), description: "Revenue split by payment method (cash/card/wallet/other) for a date range.".into(), input_schema: json!({"type":"object","properties":{"from":{"type":"string"},"to":{"type":"string"}},"required":["from","to"]}) },
        ToolDef { name: "get_profit_margin_report".into(), description: "Revenue minus cost per product, sorted by margin %. Requires cost_minor to be set on products.".into(), input_schema: json!({"type":"object","properties":{"limit":{"type":"integer"},"period_days":{"type":"integer"}}}) },
        ToolDef { name: "get_shelf_label_gap".into(), description: "Products where cost price is higher than selling price (negative margin). Critical alert for repricing.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "get_product_sales_rank".into(), description: "Rank all products by revenue or units sold for a period. Returns top N and bottom N.".into(), input_schema: json!({"type":"object","properties":{"period_days":{"type":"integer"},"limit":{"type":"integer"},"sort_by":{"type":"string","enum":["revenue","units"]}}}) },
        ToolDef { name: "get_hourly_heatmap".into(), description: "Transaction count by hour of day × day of week for the last N days. Perfect for staffing decisions.".into(), input_schema: json!({"type":"object","properties":{"period_days":{"type":"integer","description":"Lookback (default 28)"}}}) },
        ToolDef { name: "get_category_performance".into(), description: "Revenue per category as % of total sales, with comparison to the previous same-length period.".into(), input_schema: json!({"type":"object","properties":{"from":{"type":"string"},"to":{"type":"string"}},"required":["from","to"]}) },
        ToolDef { name: "get_cash_discrepancy_log".into(), description: "Shifts where the counted cash differs from expected by more than a threshold. Theft / error signal.".into(), input_schema: json!({"type":"object","properties":{"min_gap_minor":{"type":"integer","description":"Minimum gap in fils to flag (default 500 = BHD 0.500)"},"limit":{"type":"integer"}}}) },
        ToolDef { name: "get_void_rate_by_cashier".into(), description: "Void count and void rate (%) per cashier for a date range. Fraud indicator.".into(), input_schema: json!({"type":"object","properties":{"from":{"type":"string"},"to":{"type":"string"}},"required":["from","to"]}) },
        ToolDef { name: "get_peak_hours".into(), description: "Top 5 busiest hours of the day ranked by average transaction volume for the last N days.".into(), input_schema: json!({"type":"object","properties":{"period_days":{"type":"integer"}}}) },
        ToolDef { name: "get_customer_visit_frequency".into(), description: "Average days between visits per customer. Highlights lapsing-risk customers.".into(), input_schema: json!({"type":"object","properties":{"min_visits":{"type":"integer","description":"Only include customers with at least this many visits (default 2)"},"limit":{"type":"integer"}}}) },
        ToolDef { name: "get_average_basket_by_time".into(), description: "Average basket size (spend per transaction) grouped by day of week. Shows spending patterns.".into(), input_schema: json!({"type":"object","properties":{"period_days":{"type":"integer"}}}) },
        ToolDef { name: "get_tax_collected_report".into(), description: "Total VAT / tax collected per tax rule for a filing period. Bahrain NBR-ready format.".into(), input_schema: json!({"type":"object","properties":{"from":{"type":"string"},"to":{"type":"string"}},"required":["from","to"]}) },
        ToolDef { name: "get_unused_products".into(), description: "Active products with zero sales in N days AND zero stock. Candidates for deletion or deactivation.".into(), input_schema: json!({"type":"object","properties":{"days":{"type":"integer","description":"Lookback window (default 60)"}}}) },
        ToolDef { name: "get_category_mix_analysis".into(), description: "What percentage of total sales came from each category this month vs last month.".into(), input_schema: json!({"type":"object","properties":{"months":{"type":"integer","description":"Number of months to show (default 2)"}}}) },
        ToolDef { name: "get_sales_by_device".into(), description: "Revenue per POS terminal for a date range. Identifies underutilised hardware.".into(), input_schema: json!({"type":"object","properties":{"from":{"type":"string"},"to":{"type":"string"}},"required":["from","to"]}) },
        ToolDef { name: "get_revenue_forecast".into(), description: "Project next 7 and 30 days of revenue based on trailing 90-day daily average, adjusted for day-of-week patterns.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "get_customer_ltv".into(), description: "Lifetime value per customer: total revenue, visit count, first/last visit, and avg spend per visit.".into(), input_schema: json!({"type":"object","properties":{"limit":{"type":"integer"}}}) },
        ToolDef { name: "get_churn_risk".into(), description: "Customers who haven't visited in longer than their usual interval — ranked by likelihood of churn.".into(), input_schema: json!({"type":"object","properties":{"days_since_last_visit":{"type":"integer","description":"Flag customers not seen in this many days (default 30)"},"limit":{"type":"integer"}}}) },
        ToolDef { name: "get_day_of_week_comparison".into(), description: "Revenue and transaction count per day of week for a period. Shows which days are strongest.".into(), input_schema: json!({"type":"object","properties":{"period_days":{"type":"integer"}}}) },
        ToolDef { name: "get_month_over_month_growth".into(), description: "Revenue growth % month over month for the last N months, with category breakdown.".into(), input_schema: json!({"type":"object","properties":{"months":{"type":"integer","description":"Number of months to compare (default 6)"}}}) },
        ToolDef { name: "get_new_vs_returning".into(), description: "Percentage of revenue from first-time vs repeat customers for a period.".into(), input_schema: json!({"type":"object","properties":{"from":{"type":"string"},"to":{"type":"string"}},"required":["from","to"]}) },
        ToolDef { name: "get_void_report".into(), description: "All voided sales in a date range with cashier, reason, and amount.".into(), input_schema: json!({"type":"object","properties":{"from":{"type":"string"},"to":{"type":"string"}},"required":["from","to"]}) },
        ToolDef { name: "get_loyalty_summary".into(), description: "Overall loyalty programme summary: total points issued, top customers by points, points-to-BHD ratio.".into(), input_schema: json!({"type":"object","properties":{"limit":{"type":"integer"}}}) },
        ToolDef { name: "get_customer_segments".into(), description: "Group customers by purchase frequency: one-time, regular (3–9 visits), loyal (10+).".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "get_top_spenders".into(), description: "Top N customers ranked by total lifetime spend. VIP list.".into(), input_schema: json!({"type":"object","properties":{"limit":{"type":"integer","description":"Number of customers to return (default 10)"}}}) },
        ToolDef { name: "get_lapsed_customers".into(), description: "Customers who have made at least one purchase but haven't returned in N days.".into(), input_schema: json!({"type":"object","properties":{"days":{"type":"integer","description":"Days of inactivity (default 30)"},"limit":{"type":"integer"}}}) },
        ToolDef { name: "get_customer_outstanding_balance".into(), description: "Customers with unpaid delivery orders — total outstanding amount.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "get_active_deliveries_map".into(), description: "All pending and in-transit deliveries with rider, address, contact, and amount.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "get_delivery_performance".into(), description: "Average delivery time and on-time rate per rider for a date range.".into(), input_schema: json!({"type":"object","properties":{"from":{"type":"string"},"to":{"type":"string"}}}) },
        ToolDef { name: "get_delivery_payment_outstanding".into(), description: "All deliveries with status 'delivered' but payment_status 'unpaid'. Shows amount at risk.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "get_product_versions".into(), description: "Price history for a product — all price changes with dates and amounts.".into(), input_schema: json!({"type":"object","properties":{"product_id":{"type":"string"}},"required":["product_id"]}) },
        ToolDef { name: "get_tax_filing_summary".into(), description: "Taxable vs exempt sales, total VAT collected, net revenue for a filing period. Bahrain NBR format.".into(), input_schema: json!({"type":"object","properties":{"from":{"type":"string"},"to":{"type":"string"}},"required":["from","to"]}) },
        ToolDef { name: "validate_tax_config".into(), description: "Check all active products have a tax_rule_id assigned. Returns list of products missing tax configuration.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "get_z_report_archive".into(), description: "List past Z-reports (end-of-day summaries) with dates and totals — audit trail.".into(), input_schema: json!({"type":"object","properties":{"limit":{"type":"integer"},"from":{"type":"string"},"to":{"type":"string"}}}) },
        ToolDef { name: "get_low_stock_with_velocity".into(), description: "Low-stock items sorted by daily sales rate — most urgently needed items first.".into(), input_schema: json!({"type":"object","properties":{"period_days":{"type":"integer","description":"Days to calculate velocity (default 14)"}}}) },
        // ── ZanAI Insights — Phase 1 ──────────────────────────────────────────
        ToolDef { name: "get_frequently_bought_together".into(), description: "Find product pairs most often purchased together in the same transaction. Use for cross-sell recommendations and bundle design.".into(), input_schema: json!({"type":"object","properties":{"product_id":{"type":"string","description":"Anchor product (optional — omit for all pairs)"},"limit":{"type":"integer","description":"Max pairs (default 10)"},"period_days":{"type":"integer","description":"Lookback window in days (default 30)"}}}) },
        ToolDef { name: "get_bundle_suggestions".into(), description: "Margin-aware bundle recommendations: product pairs frequently co-purchased together with positive combined margin. Great for combo pricing decisions.".into(), input_schema: json!({"type":"object","properties":{"limit":{"type":"integer","description":"Max suggestions (default 10)"},"min_confidence":{"type":"number","description":"Min co-purchase rate 0.0–1.0 (default 0.1)"}}}) },
        ToolDef { name: "get_weekly_forecast".into(), description: "Project next 7 days of revenue by day of week, based on trailing N-week average. Shows expected revenue per day.".into(), input_schema: json!({"type":"object","properties":{"weeks_lookback":{"type":"integer","description":"Weeks of history to use (default 8)"}}}) },
        ToolDef { name: "get_rfm_segmentation".into(), description: "RFM customer segmentation: Champions, Loyal, Promising, At Risk, and Lapsed segments based on Recency, Frequency, and Monetary value.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "get_margin_trend".into(), description: "Monthly gross margin % trend for the last N months. Note: uses current product cost as approximation — historical margins are directional.".into(), input_schema: json!({"type":"object","properties":{"months":{"type":"integer","description":"Months to show (default 6)"}}}) },
        ToolDef { name: "get_restock_priority".into(), description: "Prioritised restock list for products at or below reorder point, scored by sales velocity, urgency, and margin. Most critical items first.".into(), input_schema: json!({"type":"object","properties":{"period_days":{"type":"integer","description":"Velocity lookback in days (default 14)"},"limit":{"type":"integer","description":"Max products (default 20)"}}}) },
        ToolDef { name: "get_dead_stock_value".into(), description: "Dead-stock items with zero sales in N days showing: quantity on hand, cost per unit, total value at risk, and estimated carrying cost.".into(), input_schema: json!({"type":"object","properties":{"days":{"type":"integer","description":"Sales lookback window (default 30)"},"limit":{"type":"integer","description":"Max products (default 50)"}}}) },
        ToolDef { name: "get_category_forecast".into(), description: "Project next 7 days of revenue per product category, based on trailing weekly averages. Shows which categories will drive the week.".into(), input_schema: json!({"type":"object","properties":{"weeks_lookback":{"type":"integer","description":"Weeks of history to use (default 8)"}}}) },
        // ── Product Quality ───────────────────────────────────────────────────
        ToolDef { name: "find_duplicate_products".into(), description: "Scan for duplicate products by exact name match, same barcode, or same SKU. Returns groups of likely duplicates with their IDs so you can merge or delete the extras.".into(), input_schema: json!({"type":"object","properties":{"include_inactive":{"type":"boolean","description":"Also scan inactive products (default false)"}}}) },
        ToolDef { name: "merge_products".into(), description: "Merge a duplicate source product into a target product. Combines stock, transfers stock movements, optionally transfers sale history, then archives the source. Irreversible — confirm before proceeding.".into(), input_schema: json!({"type":"object","properties":{"source_product_id":{"type":"string","description":"Product to archive (the duplicate)"},"target_product_id":{"type":"string","description":"Product to keep"},"transfer_history":{"type":"boolean","description":"Also reassign historical sale_items to the target (default false — keeps historical data intact)"}},"required":["source_product_id","target_product_id"]}) },
    ]
}

#[inline(never)]
fn defs_bulk_system_analytics() -> Vec<ToolDef> {
    vec![
        // ── Bulk ops ───────────────────────────────────────────────────────────
        ToolDef { name: "bulk_activate_products".into(), description: "Activate all inactive products in a category. Returns count activated.".into(), input_schema: json!({"type":"object","properties":{"category_id":{"type":"string"}},"required":["category_id"]}) },
        ToolDef { name: "bulk_deactivate_products".into(), description: "Deactivate all active products in a category (e.g. retire a seasonal menu).".into(), input_schema: json!({"type":"object","properties":{"category_id":{"type":"string"}},"required":["category_id"]}) },
        ToolDef { name: "bulk_set_category".into(), description: "Move all products from one category to another.".into(), input_schema: json!({"type":"object","properties":{"from_category_id":{"type":"string"},"to_category_id":{"type":"string"}},"required":["from_category_id","to_category_id"]}) },
        ToolDef { name: "bulk_set_tax_rule".into(), description: "Apply a tax rule to all products in a category at once.".into(), input_schema: json!({"type":"object","properties":{"category_id":{"type":"string"},"tax_rule_id":{"type":"string"}},"required":["category_id","tax_rule_id"]}) },
        ToolDef { name: "bulk_update_reorder_point".into(), description: "Set the reorder point for all products in a category.".into(), input_schema: json!({"type":"object","properties":{"category_id":{"type":"string"},"reorder_point":{"type":"number"}},"required":["category_id","reorder_point"]}) },
        ToolDef { name: "bulk_update_cost".into(), description: "Set cost_minor for multiple products at once (e.g. after a supplier price hike). Provide list of {product_id, cost_minor} pairs.".into(), input_schema: json!({"type":"object","properties":{"updates":{"type":"array","minItems":1,"items":{"type":"object","properties":{"product_id":{"type":"string"},"cost_minor":{"type":"integer"}},"required":["product_id","cost_minor"]}}},"required":["updates"]}) },
        ToolDef { name: "reassign_delivery_rider".into(), description: "Change the rider assigned to an active delivery.".into(), input_schema: json!({"type":"object","properties":{"delivery_id":{"type":"string"},"rider_name":{"type":"string"}},"required":["delivery_id","rider_name"]}) },
        ToolDef { name: "batch_dispatch_deliveries".into(), description: "Set multiple pending deliveries to dispatched status in one call. Provide a list of delivery_ids.".into(), input_schema: json!({"type":"object","properties":{"delivery_ids":{"type":"array","minItems":1,"items":{"type":"string"}}},"required":["delivery_ids"]}) },
        // ── Workflow loader ──────────────────────────────────────────────────
        ToolDef { name: "load_workflow".into(), description: "Load a detailed step-by-step guide for a supported complex workflow. Use a workflow_name accepted by the current schema; do not call this for a simple single-tool question.".into(), input_schema: json!({"type":"object","properties":{"workflow_name":{"type":"string","enum":crate::ai::workflows::workflow_names()}},"required":["workflow_name"]}) },
        // ── System ─────────────────────────────────────────────────────────────
        ToolDef { name: "vacuum_database".into(), description: "Run SQLite VACUUM to reclaim disk space and defragment the database. Safe to run at any time.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "export_product_catalog".into(), description: "Return the full product catalog as a formatted text table — useful for review, printing, or sharing.".into(), input_schema: json!({"type":"object","properties":{"include_inactive":{"type":"boolean"}}}) },
        ToolDef { name: "get_database_size".into(), description: "SQLite DB file size, WAL size, and page count.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "get_table_row_counts".into(), description: "Row count per table — gives a quick data-volume snapshot.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "check_foreign_key_integrity".into(), description: "Run PRAGMA foreign_key_check to find orphaned references. Returns PASS or lists broken foreign-key relationships.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "run_quick_integrity_check".into(), description: "Run PRAGMA quick_check — a faster alternative to full integrity_check for routine database health monitoring.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "get_database_fragmentation".into(), description: "Report database fragmentation: page count, freelist count, page size, and a recommendation (Healthy / Moderate / High fragmentation).".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "reindex_database".into(), description: "Rebuild all database indexes (REINDEX on all user tables) to restore query performance when indexes become fragmented.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "force_wal_checkpoint".into(), description: "Run a truncating WAL checkpoint to shrink an oversized write-ahead log and flush committed pages to the database.".into(), input_schema: json!({"type":"object","properties":{}}) },
        // ── Ghost barcode management ───────────────────────────────────────
        ToolDef { name: "list_ghost_barcodes".into(), description: "List unresolved (unrecognised) barcodes that have been scanned but not yet linked to a product.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "resolve_ghost_barcode".into(), description: "Link an unresolved ghost barcode to an existing product using barcode_id and product_id.".into(), input_schema: json!({"type":"object","properties":{"barcode_id":{"type":"string"},"product_id":{"type":"string"}},"required":["barcode_id","product_id"]}) },
        // ── Sync conflict management ──────────────────────────────────────
        ToolDef { name: "list_sync_conflicts".into(), description: "List unresolved multi-terminal sync conflicts. Each entry shows the conflicting table, entity, and local vs remote version.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "resolve_sync_conflict".into(), description: "Resolve a multi-terminal data conflict by retrying, pulling hub truth, reconciling stock, or dismissing it.".into(), input_schema: json!({"type":"object","properties":{"conflict_id":{"type":"string"},"action":{"type":"string","enum":["retry","pull_hub_truth","reconcile_stock","dismiss"]}},"required":["conflict_id","action"]}) },
        // ── Database maintenance ────────────────────────────────────────────
        ToolDef { name: "clear_ghost_sync_records".into(), description: "Find orphaned pending sync records whose parent entity no longer exists and mark only those orphaned records as synced.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "run_diagnostics_and_fix".into(), description: "Run system diagnostics and auto-fix common issues: DB integrity check, clear stuck AI runs (>5 min), clear stuck AI actions (>10 min). Returns a summary report.".into(), input_schema: json!({"type":"object","properties":{}}) },
        // ── Bulk imports ────────────────────────────────────────────────────
        ToolDef { name: "get_task_ledger".into(), description: "Read the persistent task ledger — your saved progress state for the current multi-step task. ALWAYS call this first when the admin says 'continue', 'complete all', or after any error/restart, instead of re-searching the database to rediscover progress.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "set_task_ledger".into(), description: "Save/update the persistent task ledger. Call this whenever you start a multi-step task and after each completed step (e.g. description: 'Importing 27 Mixto products', state: '{\"done\":12,\"remaining_barcodes\":[…]}'). Pass {\"clear\":true} when the task is fully complete.".into(), input_schema: json!({"type":"object","properties":{"description":{"type":"string"},"state":{"type":"string"},"clear":{"type":"boolean"}}}) },
        ToolDef { name: "create_products".into(), description: "Create 1–500 products from a structured JSON array without CSV or base64. Each item requires name, price_minor, and cost_minor in integer fils (BHD 0.225 = 225), and may include sku, barcode, category_id or exact category_name, tax_rule_id or exact tax_rule_name, and reorder_point. Duplicate barcodes within the batch or already in the database are skipped and reported.".into(), input_schema: json!({"type":"object","properties":{"products":{"type":"array","minItems":1,"maxItems":500,"items":{"type":"object","properties":{"name":{"type":"string"},"sku":{"type":"string"},"barcode":{"type":"string"},"category_id":{"type":"string"},"category_name":{"type":"string"},"tax_rule_id":{"type":"string"},"tax_rule_name":{"type":"string"},"price_minor":{"type":"integer"},"cost_minor":{"type":"integer"},"reorder_point":{"type":"number"}},"required":["name","price_minor","cost_minor"]}}},"required":["products"]}) },
        ToolDef { name: "bulk_import_products".into(), description: "Import products from a base64-encoded CSV supplied by the administrator. The header row is required; columns are name, sku, barcode, category_name, tax_rule_name, price_minor, cost_minor, and reorder_point. Money fields use integer fils; category and tax-rule names must match exactly. Duplicate barcodes are skipped and reported.".into(), input_schema: json!({"type":"object","properties":{"csv_base64":{"type":"string"}},"required":["csv_base64"]}) },
        ToolDef { name: "bulk_import_categories".into(), description: "Import categories from a base64-encoded CSV containing a name column.".into(), input_schema: json!({"type":"object","properties":{"csv_base64":{"type":"string"}},"required":["csv_base64"]}) },
        // ── WhatsApp receipt delivery ───────────────────────────────────────
        ToolDef { name: "send_receipt_via_whatsapp".into(), description: "Send a sale receipt PDF via WhatsApp to the supplied country-code-qualified phone number.".into(), input_schema: json!({"type":"object","properties":{"receipt_number":{"type":"string"},"phone":{"type":"string"}},"required":["receipt_number","phone"]}) },
        // ── Supplier CRUD (complete) ───────────────────────────────────────────
        ToolDef { name: "get_supplier".into(), description: "Get a single supplier record with contact details and summary of linked products and purchase orders.".into(), input_schema: json!({"type":"object","properties":{"supplier_id":{"type":"string"}},"required":["supplier_id"]}) },
        ToolDef { name: "delete_supplier".into(), description: "Delete a supplier. Blocked if they have purchase orders or products linked to them.".into(), input_schema: json!({"type":"object","properties":{"supplier_id":{"type":"string"}},"required":["supplier_id"]}) },
        ToolDef { name: "get_purchase_order".into(), description: "Get a single purchase order with all line items, ordered vs received quantities, and total cost.".into(), input_schema: json!({"type":"object","properties":{"po_id":{"type":"string"}},"required":["po_id"]}) },
        ToolDef { name: "receive_purchase_order".into(), description: "Mark purchase order lines as received. Automatically increments stock for each line. Updates PO status to received or partial.".into(), input_schema: json!({"type":"object","properties":{"po_id":{"type":"string"},"lines":{"type":"array","description":"Leave empty to receive all lines in full","items":{"type":"object","properties":{"po_line_id":{"type":"string"},"received_qty":{"type":"number"},"expiry_date":{"type":"string","description":"Optional lot expiry in YYYY-MM-DD"}},"required":["po_line_id","received_qty"]}}},"required":["po_id"]}) },
        ToolDef { name: "delete_purchase_order".into(), description: "Cancel and delete a purchase order. Blocked if it has already been received (full or partial).".into(), input_schema: json!({"type":"object","properties":{"po_id":{"type":"string"}},"required":["po_id"]}) },
        ToolDef { name: "get_supplier_products".into(), description: "List all products whose default_supplier_id matches this supplier.".into(), input_schema: json!({"type":"object","properties":{"supplier_id":{"type":"string"}},"required":["supplier_id"]}) },
        ToolDef { name: "bulk_assign_supplier".into(), description: "Set the default supplier for all products in a category.".into(), input_schema: json!({"type":"object","properties":{"category_id":{"type":"string"},"supplier_id":{"type":"string"}},"required":["category_id","supplier_id"]}) },
        // ── Promotions ─────────────────────────────────────────────────────────
        ToolDef { name: "list_promotions".into(), description: "List active and upcoming promotional prices. Returns product name, promo price, effective from/to, and status (active/upcoming/expired).".into(), input_schema: json!({"type":"object","properties":{"status":{"type":"string","enum":["active","upcoming","expired","all"]},"search":{"type":"string"},"limit":{"type":"integer"}}}) },
        ToolDef { name: "get_promotion".into(), description: "Get details of a single promotion by its price_id, including the product it applies to.".into(), input_schema: json!({"type":"object","properties":{"price_id":{"type":"string"}},"required":["price_id"]}) },
        ToolDef { name: "bulk_promotion_remove".into(), description: "End active promotions for products matching a selector. Sets effective_to = now for all matching promotional prices. Preview first, then confirm.".into(), input_schema: json!({"type":"object","properties":{"selector":{"type":"object","properties":{"category_subtree":{"type":"string"},"text":{"type":"string"},"active":{"type":"boolean"}}}}}) },
        // ── Inventory intelligence ────────────────────────────────────────────
        ToolDef { name: "get_inventory_valuation".into(), description: "Total inventory value = stock_quantity × cost_minor per product. Products without cost are flagged.".into(), input_schema: json!({"type":"object","properties":{"include_zero_cost":{"type":"boolean","description":"Include products with no cost set (default false)"}}}) },
        ToolDef { name: "get_overstock_alert".into(), description: "Products with current stock > N days supply at current sales rate. Ties up cash unnecessarily.".into(), input_schema: json!({"type":"object","properties":{"overstock_days":{"type":"integer","description":"Flag items with more than this many days of supply (default 60)"},"period_days":{"type":"integer","description":"Days to calculate velocity (default 14)"}}}) },
        ToolDef { name: "get_sales_velocity".into(), description: "Average daily units sold per product, ranked fast → slow. Useful for ordering decisions.".into(), input_schema: json!({"type":"object","properties":{"period_days":{"type":"integer"},"limit":{"type":"integer"}}}) },
        ToolDef { name: "get_stock_turnover_ratio".into(), description: "COGS ÷ average inventory value. Higher = faster-moving stock. Calculated per category and overall.".into(), input_schema: json!({"type":"object","properties":{"period_days":{"type":"integer","description":"Period to calculate COGS (default 30)"}}}) },
        // ── Shift / cash ─────────────────────────────────────────────────────
        ToolDef { name: "get_open_shifts".into(), description: "All currently open (unclosed) shifts across all terminals with cashier name, start time, and sales so far.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "get_expected_cash_position".into(), description: "Expected cash in drawer right now: opening cash + cash sales − paid_outs − safe_drops + paid_ins. For a specific shift or all open shifts.".into(), input_schema: json!({"type":"object","properties":{"shift_id":{"type":"string","description":"Leave empty for all open shifts"}}}) },
        ToolDef { name: "get_petty_cash_log".into(), description: "All paid_in / paid_out / safe_drop events for a period with cashier, amount, and note.".into(), input_schema: json!({"type":"object","properties":{"from":{"type":"string"},"to":{"type":"string"}}}) },
        ToolDef { name: "force_close_shift".into(), description: "Admin override: force-close a shift that is stuck open (cashier forgot). Logs the action in the audit trail.".into(), input_schema: json!({"type":"object","properties":{"shift_id":{"type":"string"}},"required":["shift_id"]}) },
        // ── User analytics ────────────────────────────────────────────────────
        ToolDef { name: "get_user_shift_summary".into(), description: "All shifts for a staff member: open time, close time, total sales, and variance for each.".into(), input_schema: json!({"type":"object","properties":{"user_id":{"type":"string"},"from":{"type":"string"},"to":{"type":"string"}},"required":["user_id"]}) },
        ToolDef { name: "compare_cashiers".into(), description: "Side-by-side comparison of cashier performance: sales, voids, discounts, average basket, transaction count.".into(), input_schema: json!({"type":"object","properties":{"from":{"type":"string"},"to":{"type":"string"}}}) },
        // ── Customer ops ──────────────────────────────────────────────────────
        ToolDef { name: "export_customers".into(), description: "Full customer list as a formatted table: name, phone, visit count, last visit, lifetime spend, loyalty points.".into(), input_schema: json!({"type":"object","properties":{"limit":{"type":"integer"}}}) },
        // ── System ────────────────────────────────────────────────────────────
        ToolDef { name: "get_migration_status".into(), description: "List applied database migrations with name and timestamp. Useful for debugging version mismatches.".into(), input_schema: json!({"type":"object","properties":{}}) },
        ToolDef { name: "get_app_version".into(), description: "Current app version, build info, Rust toolchain version, and migration count.".into(), input_schema: json!({"type":"object","properties":{}}) },
        // ── Analytics (round 2) ───────────────────────────────────────────────
        ToolDef { name: "get_basket_size_trend".into(), description: "Average number of items per transaction per day over the last N days. Shows whether customers are buying more or less per visit.".into(), input_schema: json!({"type":"object","properties":{"period_days":{"type":"integer"}}}) },
        ToolDef { name: "get_stockout_cost".into(), description: "Products that hit zero stock in last N days — estimate lost revenue as avg daily sales rate × days out of stock × price.".into(), input_schema: json!({"type":"object","properties":{"period_days":{"type":"integer"}}}) },
        ToolDef { name: "get_refund_rate".into(), description: "Refund amount as a % of gross sales by cashier and by product for a date range.".into(), input_schema: json!({"type":"object","properties":{"from":{"type":"string"},"to":{"type":"string"}},"required":["from","to"]}) },
        ToolDef { name: "get_refund_by_product".into(), description: "Products ranked by refund count and refund amount — signal for quality issues.".into(), input_schema: json!({"type":"object","properties":{"from":{"type":"string"},"to":{"type":"string"},"limit":{"type":"integer"}}}) },
        // ── WhatsApp commerce (read) ──────────────────────────────────────────
        ToolDef { name: "list_whatsapp_orders".into(), description: "List WhatsApp Business orders customers placed from the store's catalog. Optional status filter: new, reviewed, fulfilled, cancelled. Shows customer, line items, and total.".into(), input_schema: json!({"type":"object","properties":{"status":{"type":"string"},"limit":{"type":"integer"}}}) },
        // ── Compliance ────────────────────────────────────────────────────────
        ToolDef { name: "verify_receipt_sequence".into(), description: "Check receipt numbers for gaps or duplicates. A sequential break could indicate a deleted or tampered sale.".into(), input_schema: json!({"type":"object","properties":{"from":{"type":"string"},"to":{"type":"string"}}}) },
        ToolDef { name: "get_audit_trail_full".into(), description: "Every data mutation in the audit_logs table: who changed what, when, and what it looked like before and after.".into(), input_schema: json!({"type":"object","properties":{"entity_type":{"type":"string","description":"Filter by entity type e.g. product, sale, user"},"user_id":{"type":"string"},"from":{"type":"string"},"to":{"type":"string"},"limit":{"type":"integer"}}}) },
    ]
}

#[inline(never)]
fn finish_tool_definitions(mut tools: Vec<ToolDef>) -> Vec<ToolDef> {
    // Read intents already have explicit catalogue entries above. Keeping one
    // source avoids silently resolving duplicate schemas by insertion order.
    let engine_ids = [
        "bulk_price_adjust",
        "bulk_stock_set",
        "bulk_stock_variance_fix",
        "bulk_promotion_apply",
        "bulk_promotion_remove",
        "bulk_supplier_price_sync",
        "bulk_product_archive",
        "bulk_reorder_point_update",
        "create_product",
    ];
    // Engine operations own their schemas; discard the legacy handwritten
    // placeholders by explicit name before adding the canonical definitions.
    tools.retain(|tool| !engine_ids.contains(&tool.name.as_str()));
    let engine_ops: Vec<Box<dyn Operation>> = vec![
        Box::new(BulkPriceAdjust),
        Box::new(BulkStockSet),
        Box::new(BulkStockVarianceFix),
        Box::new(BulkPromotionApply),
        Box::new(BulkPromotionRemove),
        Box::new(BulkSupplierPriceSync),
        Box::new(BulkProductArchive),
        Box::new(BulkReorderPointUpdate),
        Box::new(ProductCreate),
    ];
    for operation in engine_ops {
        let description = if operation.id() == "bulk_stock_set" {
            "Set the exact stock quantity for every inventory-tracked product matched by a compact selector. Use this for uniform large-catalogue stock takes without enumerating product IDs. Preview returns the matched count before execution."
                .into()
        } else {
            format!(
                "Deterministic operation '{}'. Preview its affected records before execution.",
                operation.id()
            )
        };
        tools.push(ToolDef {
            name: operation.id().into(),
            description,
            input_schema: operation.schema(),
        });
    }
    for tool in &mut tools {
        close_object_schemas(&mut tool.input_schema);
    }
    tools
}

fn close_object_schemas(schema: &mut Value) {
    match schema {
        Value::Object(object) => {
            if object.get("type").and_then(Value::as_str) == Some("object") {
                object
                    .entry("additionalProperties")
                    .or_insert(Value::Bool(false));
            }
            for child in object.values_mut() {
                close_object_schemas(child);
            }
        }
        Value::Array(values) => {
            for child in values {
                close_object_schemas(child);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn assert_closed_objects(tool_name: &str, schema: &Value, path: &str) {
        if schema.get("type").and_then(Value::as_str) == Some("object") {
            assert_eq!(
                schema.get("additionalProperties"),
                Some(&Value::Bool(false)),
                "{tool_name} has an open object schema at {path}"
            );
            if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
                for (name, property) in properties {
                    assert_closed_objects(tool_name, property, &format!("{path}.{name}"));
                }
            }
        }
        if schema.get("type").and_then(Value::as_str) == Some("array") {
            if let Some(items) = schema.get("items") {
                assert_closed_objects(tool_name, items, &format!("{path}[]"));
            }
        }
    }

    #[test]
    fn every_advertised_object_schema_is_closed_recursively() {
        for definition in all_tool_definitions() {
            assert_closed_objects(&definition.name, &definition.input_schema, "input");
        }
    }

    #[test]
    fn required_bulk_arrays_advertise_at_least_one_item() {
        for (tool_name, field) in [
            ("bulk_stock_take", "items"),
            ("bulk_update_prices", "updates"),
            ("create_purchase_order", "lines"),
            ("bulk_update_cost", "updates"),
            ("batch_dispatch_deliveries", "delivery_ids"),
            ("bulk_supplier_price_sync", "updates"),
        ] {
            let definition = all_tool_definitions()
                .into_iter()
                .find(|definition| definition.name == tool_name)
                .unwrap();
            assert_eq!(
                definition.input_schema["properties"][field]["minItems"], 1,
                "{tool_name}.{field} must reject empty arrays"
            );
        }
    }

    #[test]
    fn schema_normalization_preserves_explicit_additional_properties() {
        let mut schema = serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": true
        });

        close_object_schemas(&mut schema);

        assert_eq!(schema["additionalProperties"], true);
    }

    #[test]
    fn catalogue_exposes_one_canonical_product_create_tool() {
        let definitions = all_tool_definitions();
        let product_create_tools: Vec<_> = definitions
            .iter()
            .filter(|definition| {
                definition.name == "create_product" || definition.name == "product_create"
            })
            .collect();

        assert_eq!(product_create_tools.len(), 1);
        assert_eq!(product_create_tools[0].name, "create_product");
        assert_eq!(
            product_create_tools[0].input_schema["required"],
            serde_json::json!(["name", "category_id", "price_minor"])
        );
    }

    #[test]
    fn catalogue_describes_compact_bulk_stock_set_without_product_ids() {
        let definition = all_tool_definitions()
            .into_iter()
            .find(|definition| definition.name == "bulk_stock_set")
            .unwrap();

        assert!(definition.description.contains("exact stock quantity"));
        assert!(definition
            .description
            .contains("Preview returns the matched count"));
        assert!(
            definition.input_schema["properties"]["selector"]["properties"]
                .get("track_inventory")
                .is_some()
        );
        assert!(definition.input_schema["properties"].get("items").is_none());
    }

    #[test]
    fn tool_descriptions_define_semantics_without_duplicating_runtime_authorization() {
        for definition in all_tool_definitions() {
            let description = definition.description.to_ascii_lowercase();
            assert!(
                !description.contains("confirmation")
                    && !description.contains("after approval")
                    && !description.contains("requires admin"),
                "{} duplicates runtime authorization policy in its description: {}",
                definition.name,
                definition.description
            );
        }
    }

    #[test]
    fn create_products_contract_matches_its_schema_and_duplicate_behavior() {
        let definition = all_tool_definitions()
            .into_iter()
            .find(|definition| definition.name == "create_products")
            .unwrap();

        assert_eq!(
            definition.input_schema["properties"]["products"]["minItems"],
            1
        );
        assert_eq!(
            definition.input_schema["properties"]["products"]["maxItems"],
            500
        );
        assert!(definition.description.contains("1–500"));
        assert!(definition.description.contains("Duplicate barcodes"));
        assert!(definition.description.contains("skipped and reported"));
    }

    #[test]
    fn navigation_schema_accepts_the_existing_purchasing_workspace() {
        let definition = all_tool_definitions()
            .into_iter()
            .find(|definition| definition.name == "open_tab")
            .unwrap();
        let tabs = definition.input_schema["properties"]["tab"]["enum"]
            .as_array()
            .unwrap();

        assert!(tabs.iter().any(|tab| tab == "purchasing"));
    }

    #[test]
    fn workflow_loader_schema_is_generated_from_the_canonical_workflow_registry() {
        let definition = all_tool_definitions()
            .into_iter()
            .find(|definition| definition.name == "load_workflow")
            .unwrap();
        let advertised = definition.input_schema["properties"]["workflow_name"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(advertised, crate::ai::workflows::workflow_names());
        assert!(!definition.description.contains("whatsapp_message"));
    }
}
