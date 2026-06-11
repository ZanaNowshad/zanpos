use crate::ai::client::ToolDef;
use crate::db::repositories::{product_repo, report_repo, sync_repo};
use crate::domain::ai_admin::{ToolPreview, ToolPreviewField};
use crate::domain::money;
use crate::errors::{AppError, AppResult};
use crate::inventory::{movements, stock_repo};
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};

// ── Dynamic-bind helper (S-01) ──────────────────────────────────────────────────
// Lets dynamic UPDATE statements bind heterogeneous values as proper parameters
// instead of interpolating escaped strings into SQL. Both match arms return the
// same Query type, so binds can be applied in a loop.
type SqliteQuery<'q> = sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>;

enum SqlBind {
    S(String),
    I(i64),
}

impl SqlBind {
    fn apply<'q>(&'q self, q: SqliteQuery<'q>) -> SqliteQuery<'q> {
        match self {
            SqlBind::S(s) => q.bind(s),
            SqlBind::I(i) => q.bind(i),
        }
    }
}

// ── Tool catalogue ─────────────────────────────────────────────────────────────

pub fn all_tool_definitions() -> Vec<ToolDef> {
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
            description: "Update the selling price of a product. Requires admin confirmation.".into(),
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
            description: "Enable or disable a product. Disabled products don't appear in POS. Requires admin confirmation.".into(),
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
            description: "Rename a product. Requires admin confirmation.".into(),
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
            description: "Apply a positive or negative quantity adjustment to a product's stock. Use for corrections, write-offs, or manual receives. Requires admin confirmation.".into(),
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
            description: "Set a product's stock to an exact counted quantity (full stock take). Requires admin confirmation.".into(),
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
            description: "List all product categories with their IDs, names, and colors.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
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
            description: "Update the reorder point (low-stock threshold) for a product. When stock falls to or below this number, a low-stock alert fires. Requires admin confirmation.".into(),
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
            description: "Create a new product in the catalog with a name, price, and category. Requires admin confirmation.".into(),
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
            description: "Create a new customer record. Requires admin confirmation.".into(),
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
            description: "Update an existing customer's contact details or notes. Requires admin confirmation.".into(),
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
            description: "Advance a delivery order to the next status (pending → in_transit → delivered). Requires admin confirmation.".into(),
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
            description: "Set exact stock counts for multiple products at once from a physical count. More efficient than individual stock_take calls. Requires admin confirmation.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
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
            description: "Create a new product category. Requires admin confirmation.".into(),
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
            description: "Update an existing category's name, sort order, or active status. Requires admin confirmation.".into(),
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
            description: "Create a new staff account (cashier, manager, or owner). PIN must be 4+ digits. Requires admin confirmation.".into(),
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
            description: "Update a staff account: change display name, role, active status, or reset PIN. Requires admin confirmation.".into(),
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
            description: "Create a new tax rule (e.g. 10% VAT inclusive). Requires admin confirmation.".into(),
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
            description: "Update an existing tax rule's name, rate, inclusive flag, or active status. Requires admin confirmation.".into(),
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
            description: "Update all product fields at once: name, category, SKU, barcode, price, tax_rule, inventory tracking, reorder point, active status. Use this instead of calling multiple individual mutations. Requires admin confirmation.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "product_id": { "type": "string" },
                    "name": { "type": "string", "description": "New display name" },
                    "category_id": { "type": "string", "description": "New category ID" },
                    "sku": { "type": "string", "description": "New SKU code" },
                    "barcode": { "type": "string", "description": "New primary barcode" },
                    "price_minor": { "type": "integer", "description": "New selling price in minor units" },
                    "tax_rule_id": { "type": "string", "description": "Tax rule ID (use list_tax_rules to find IDs). Pass empty string to remove." },
                    "track_inventory": { "type": "boolean", "description": "Enable inventory tracking" },
                    "allow_decimal_quantity": { "type": "boolean", "description": "Allow fractional quantities" },
                    "reorder_point": { "type": "number", "description": "Low-stock alert threshold" },
                    "is_active": { "type": "boolean", "description": "Show in POS?" }
                },
                "required": ["product_id"]
            }),
        },
        // ── Store settings mutation ────────────────────────────────────────────
        ToolDef {
            name: "update_store_settings".into(),
            description: "Update the active store/branch settings: name, address, phone, tax number, CR number, receipt header/footer. Requires admin confirmation.".into(),
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
            description: "Update business operation rules (toggles). Requires admin confirmation.".into(),
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
            description: "Confirm that a customer has paid for a delivery order. Marks the delivery as paid. Requires admin confirmation.".into(),
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
            description: "Cancel a delivery order. The delivery status will be set to cancelled. Requires admin confirmation.".into(),
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
            description: "Trigger a full database backup to the system's backup directory. Use before making bulk changes or at end of day. Requires admin confirmation.".into(),
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
                    "updates": { "type": "array", "items": {
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
        // ── Extension read tools ──────────────────────────────────────────────
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
    ]
}

/// L16: Single source of truth for mutation tools.
/// When adding a new tool to `all_tool_definitions`, add its name here too.
/// Omitting a mutation tool from this list makes it silently execute without
/// confirmation — the hardcoded list is intentional but must be kept in sync.
pub const MUTATION_TOOLS: &[&str] = &[
    "update_product_price",
    "set_product_active",
    "update_product_name",
    "adjust_stock",
    "stock_take",
    "create_product",
    "update_reorder_point",
    "create_customer",
    "update_customer",
    "advance_delivery_status",
    "bulk_stock_take",
    "create_category",
    "update_category",
    "create_user",
    "update_user",
    "create_tax_rule",
    "update_tax_rule",
    "update_product_full",
    "update_store_settings",
    "update_business_rules",
    "confirm_delivery_payment",
    "cancel_delivery",
    "backup_database",
    "sync_reset_stuck",
    "sync_queue_retry",
    "sync_queue_dismiss",
    "void_sale",
    "delete_customer",
    "set_device_active",
    "receive_stock",
    "add_loyalty_points",
    "bulk_update_prices",
    // ── Extension mutations ──────────────────────────────────────────────────
    "create_refund",
    "create_cash_event",
    "open_shift",
    "close_shift",
    "add_product_barcode",
    "remove_product_barcode",
    "trigger_sync_now",
    "force_full_resync",
    "revert_delivery_payment",
    "update_branch_settings",
    "register_device",
    "send_whatsapp_delivery_alert",
    "send_whatsapp_payment_reminder",
    "send_whatsapp_arrival_notice",
    "disconnect_whatsapp",
    "update_thermal_config",
    "open_cash_drawer",
    "reprint_receipt",
    "delete_held_cart",
    "update_benefit_number",
];

pub fn is_mutation_tool(name: &str) -> bool {
    MUTATION_TOOLS.contains(&name)
}

// ── Read-only tool executor ────────────────────────────────────────────────────

pub async fn execute_read_tool(
    pool: &SqlitePool,
    tool_name: &str,
    input: &Value,
    branch_id: &str,
    currency_exp: u32,
) -> AppResult<String> {
    match tool_name {
        "get_today_summary" => {
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            let s = report_repo::today_summary(pool, branch_id, &today).await?;
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            Ok(format!(
                "[DB] Today ({}):\n- Transactions: {}\n- Net Total: BHD {}\n- Tax: BHD {}\n- Discounts: BHD {}\n- Cash: BHD {}\n- Card: BHD {}\n- Refunds: {} (BHD {})",
                s.business_date, s.transaction_count,
                fmt(s.net_total_minor), fmt(s.tax_total_minor), fmt(s.discount_total_minor),
                fmt(s.cash_total_minor), fmt(s.card_total_minor),
                s.refund_count, fmt(s.refund_total_minor)
            ))
        }
        "list_products" => {
            let products = product_repo::list_all_active(pool, None, u32::MAX).await?;
            if products.is_empty() {
                return Ok("No active products found.".into());
            }
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            let lines: Vec<String> = products
                .iter()
                .map(|p| {
                    format!(
                        "- {} (ID: {}) — BHD {} — {}",
                        p.product.name,
                        p.product.product_id,
                        fmt(p.price_minor),
                        p.category_name
                    )
                })
                .collect();
            Ok(format!(
                "[DB] {} active products:\n{}",
                products.len(),
                lines.join("\n")
            ))
        }
        "search_products" => {
            let query = input.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let products = product_repo::search_products(pool, query, 20).await?;
            if products.is_empty() {
                return Ok(format!("No products found matching '{}'.", query));
            }
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            let lines: Vec<String> = products
                .iter()
                .map(|p| {
                    format!(
                        "- {} (ID: {}) — BHD {}",
                        p.product.name,
                        p.product.product_id,
                        fmt(p.price_minor)
                    )
                })
                .collect();
            Ok(format!(
                "{} results for '{}':\n{}",
                products.len(),
                query,
                lines.join("\n")
            ))
        }
        "get_product" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            Ok(format!(
                "Product: {}\nID: {}\nSKU: {}\nBarcode: {}\nCategory: {}\nPrice: BHD {}\nActive: {}\nTrack Inventory: {}",
                p.product.name, p.product.product_id,
                p.product.sku.as_deref().unwrap_or("—"),
                p.product.barcode.as_deref().unwrap_or("—"),
                p.category_name,
                fmt(p.price_minor),
                p.product.is_active,
                p.product.track_inventory
            ))
        }
        "get_stock_levels" => {
            let levels = stock_repo::get_all_levels(pool, &active_branch_id(pool).await?).await?;
            if levels.is_empty() {
                return Ok("No inventory-tracked products found.".into());
            }
            let lines: Vec<String> = levels
                .iter()
                .map(|s| {
                    let status = if s.is_out_of_stock {
                        "❌ OUT"
                    } else if s.is_low_stock {
                        "⚠ LOW"
                    } else {
                        "✓"
                    };
                    format!(
                        "- {} (ID: {}) — qty: {} | reorder ≤{} {status}",
                        s.product_name, s.product_id, s.quantity_on_hand, s.reorder_point
                    )
                })
                .collect();
            Ok(format!(
                "[DB] {} tracked products:\n{}",
                levels.len(),
                lines.join("\n")
            ))
        }
        "get_low_stock" => {
            let levels = stock_repo::get_low_stock(pool, &active_branch_id(pool).await?).await?;
            if levels.is_empty() {
                return Ok("All products are above their reorder points. 🎉".into());
            }
            let lines: Vec<String> = levels
                .iter()
                .map(|s| {
                    let status = if s.is_out_of_stock {
                        "OUT OF STOCK"
                    } else {
                        "LOW STOCK"
                    };
                    format!(
                        "- {} — qty: {} | reorder ≤{} [{status}]",
                        s.product_name, s.quantity_on_hand, s.reorder_point
                    )
                })
                .collect();
            Ok(format!(
                "[DB] {} product(s) need restocking:\n{}",
                levels.len(),
                lines.join("\n")
            ))
        }
        "get_cash_summary" => {
            let shift_id = input
                .get("shift_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing shift_id".into()))?;
            let fmt = |n: i64| money::format_minor(n, currency_exp);

            let shift = sqlx::query(
                "SELECT opening_cash_minor, counted_cash_minor FROM shifts WHERE shift_id = ?",
            )
            .bind(shift_id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Shift not found".into()))?;

            let opening: i64 = shift.get("opening_cash_minor");
            let counted: Option<i64> = shift.get("counted_cash_minor");

            let cash_sales: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(p.amount_minor),0) FROM payments p
                 JOIN sales s ON s.sale_id=p.sale_id
                 WHERE s.shift_id=? AND p.payment_method='cash' AND s.status!='voided'
                   AND (s.is_delivery = 0 OR EXISTS (
                       SELECT 1 FROM delivery_orders d WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'
                   ))",
            )
            .bind(shift_id)
            .fetch_one(pool)
            .await?;

            let cash_refunds: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(
                    CASE WHEN s.net_total_minor <= 0 THEN 0
                    ELSE MIN(
                        (SELECT COALESCE(SUM(p2.amount_minor), 0)
                         FROM payments p2
                         WHERE p2.sale_id = s.sale_id AND p2.payment_method = 'cash'),
                        s.net_total_minor
                    ) * r.refund_total_minor / s.net_total_minor
                    END
                ), 0)
                 FROM refunds r
                 JOIN sales s ON s.sale_id = r.original_sale_id
                 WHERE s.shift_id = ?",
            )
            .bind(shift_id)
            .fetch_one(pool)
            .await?;

            let paid_in: i64  = sqlx::query_scalar(
                "SELECT COALESCE(SUM(amount_minor),0) FROM cash_events WHERE shift_id=? AND event_type='paid_in'"
            ).bind(shift_id).fetch_one(pool).await?;
            let paid_out: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(amount_minor),0) FROM cash_events WHERE shift_id=? AND event_type='paid_out'"
            ).bind(shift_id).fetch_one(pool).await?;
            let safe_drop: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(amount_minor),0) FROM cash_events WHERE shift_id=? AND event_type='safe_drop'"
            ).bind(shift_id).fetch_one(pool).await?;

            let expected = opening + cash_sales - cash_refunds + paid_in - paid_out - safe_drop;
            let variance = counted.map(|c| c - expected);

            let mut lines = vec![
                format!("Cash Drawer — Shift {shift_id}"),
                format!("  Opening float:  {}", fmt(opening)),
                format!("  Cash sales:     +{}", fmt(cash_sales)),
                format!("  Cash refunds:   -{}", fmt(cash_refunds)),
                format!("  Paid in:        +{}", fmt(paid_in)),
                format!("  Paid out:       -{}", fmt(paid_out)),
                format!("  Safe drops:     -{}", fmt(safe_drop)),
                format!("  Expected:       {}", fmt(expected)),
            ];
            if let Some(c) = counted {
                let v = variance.unwrap_or(0);
                lines.push(format!("  Counted:        {}", fmt(c)));
                lines.push(format!(
                    "  Variance:       {} {}",
                    if v >= 0 { "+" } else { "" },
                    fmt(v)
                ));
            } else {
                lines.push("  Counted:        (not yet entered)".into());
            }
            Ok(lines.join("\n"))
        }
        "get_recent_refunds" => {
            let limit = input
                .get("limit")
                .and_then(|v| v.as_i64())
                .unwrap_or(10)
                .min(20);
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            let rows = sqlx::query(
                "SELECT r.refund_id, r.original_sale_id, r.refund_total_minor,
                        r.reason, r.return_reason_code, r.created_at
                 FROM refunds r ORDER BY r.created_at DESC LIMIT ?",
            )
            .bind(limit)
            .fetch_all(pool)
            .await?;

            if rows.is_empty() {
                return Ok("No refunds found.".into());
            }
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let total: i64 = r.get("refund_total_minor");
                    let id: String = r.get("refund_id");
                    let sale: String = r.get("original_sale_id");
                    let reason: Option<String> = r.get("reason");
                    let code: Option<String> = r.get("return_reason_code");
                    let at: String = r.get("created_at");
                    format!(
                        "- {} | sale {} | {} | {} [{}] | {}",
                        &id[..8.min(id.len())],
                        &sale[..8.min(sale.len())],
                        fmt(total),
                        reason.as_deref().unwrap_or("—"),
                        code.as_deref().unwrap_or("other"),
                        &at[..10]
                    )
                })
                .collect();
            Ok(format!(
                "{} recent refund(s):\n{}",
                rows.len(),
                lines.join("\n")
            ))
        }
        "get_audit_log" => {
            let event_filter = input.get("event_type").and_then(|v| v.as_str());
            // `created_at` is stored as UTC ISO-8601. Compute UTC start-of-today in Bahrain
            // (UTC+3, no DST) so records from early morning local time are not missed.
            // Bahrain midnight = UTC midnight − 3 h, i.e. previous day 21:00 UTC.
            const BAHRAIN_OFFSET_HOURS: i64 = 3;
            let utc_now = chrono::Utc::now();
            let bahrain_naive_now = utc_now.naive_utc() + chrono::Duration::hours(BAHRAIN_OFFSET_HOURS);
            let bahrain_today = bahrain_naive_now.date();
            let today_utc_start = (bahrain_today
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc()
                - chrono::Duration::hours(BAHRAIN_OFFSET_HOURS))
            .to_rfc3339();
            let rows = if let Some(et) = event_filter {
                sqlx::query(
                    "SELECT audit_log_id, event_type, entity_type, entity_id,
                            actor_user_id, created_at
                     FROM audit_logs
                     WHERE event_type = ? AND created_at >= ?
                     ORDER BY created_at DESC LIMIT 30",
                )
                .bind(et)
                .bind(&today_utc_start)
                .fetch_all(pool)
                .await?
            } else {
                sqlx::query(
                    "SELECT audit_log_id, event_type, entity_type, entity_id,
                            actor_user_id, created_at
                     FROM audit_logs
                     WHERE created_at >= ?
                     ORDER BY created_at DESC LIMIT 30",
                )
                .bind(&today_utc_start)
                .fetch_all(pool)
                .await?
            };

            if rows.is_empty() {
                return Ok("No audit log entries found for today.".into());
            }
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let id: String = r.get("audit_log_id");
                    let et: String = r.get("event_type");
                    let eid: Option<String> = r.get("entity_id");
                    let actor: Option<String> = r.get("actor_user_id");
                    let at: String = r.get("created_at");
                    format!(
                        "- {} | {} | entity: {} | actor: {} | {}",
                        &id[..8.min(id.len())],
                        et,
                        &eid.as_deref().unwrap_or("—")
                            [..8.min(eid.as_deref().unwrap_or("—").len())],
                        actor.as_deref().unwrap_or("system"),
                        &at[11..19.min(at.len())]
                    )
                })
                .collect();
            Ok(format!(
                "{} audit entries today:\n{}",
                rows.len(),
                lines.join("\n")
            ))
        }
        "get_sync_status" => {
            // Fetch device_id from the active device
            let device_id: String = sqlx::query_scalar(
                "SELECT device_id FROM devices WHERE is_active=1 ORDER BY device_code LIMIT 1",
            )
            .fetch_optional(pool)
            .await?
            .flatten()
            .unwrap_or_default();

            let status = sync_repo::get_sync_status(pool, &device_id).await?;
            let pending = status.pending_events;
            let failed: i64 = 0; // new model uses sync_attempts on individual rows

            let cloud = if status.hub_configured {
                "✓ configured"
            } else {
                "✗ not configured"
            };
            let last = status.last_successful_sync_at.as_deref().unwrap_or("never");
            let mut lines = vec![
                format!("Sync Status:"),
                format!("  Hub (LAN sync):   {cloud}"),
                format!("  Last sync:        {last}"),
                format!("  Pending rows:     {pending}"),
                format!("  Stuck rows:       {failed}"),
            ];
            Ok(lines.join("\n"))
        }
        "get_sync_diagnostics" => {
            let mut lines = vec!["[DB] Sync Diagnostics:".to_string()];

            let hub_mode: Option<String> =
                sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'hub_mode'")
                    .fetch_optional(pool).await?.flatten();
            let hub_url: Option<String> =
                sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'hub_url'")
                    .fetch_optional(pool).await?.flatten();
            let token = crate::secure_store::get_secret("hub_store_token").unwrap_or_default();
            let hub_label = if hub_mode.as_deref() == Some("1") {
                "✓ this device IS the hub".to_string()
            } else if hub_url.as_deref().is_some_and(|u| !u.is_empty()) && !token.is_empty() {
                format!("✓ terminal connected to {}", hub_url.as_deref().unwrap_or(""))
            } else {
                "✗ NOT configured (Settings → Hub)".to_string()
            };
            lines.push(format!("  Hub: {hub_label}"));

            let last_sync: Option<String> = sqlx::query_scalar(
                "SELECT last_pushed_at FROM sync_watermark WHERE table_name = 'sales'",
            ).fetch_optional(pool).await.ok().flatten();
            lines.push(format!("  Last Sync: {}", last_sync.as_deref().unwrap_or("never")));

            lines.push("".to_string());
            lines.push("  Per-table breakdown — pending / stuck (attempts>=10) / max-att / avg-att:".to_string());

            for table in crate::commands::sync_commands::SYNC_TABLES {
                let pending: i64 = sqlx::query_scalar(
                    &format!("SELECT COUNT(*) FROM {table} WHERE sync_status = 'pending' AND sync_attempts < 10"),
                ).fetch_one(pool).await.unwrap_or(0);
                let stuck: i64 = sqlx::query_scalar(
                    &format!("SELECT COUNT(*) FROM {table} WHERE sync_status = 'pending' AND sync_attempts >= 10"),
                ).fetch_one(pool).await.unwrap_or(0);
                if pending == 0 && stuck == 0 { continue; }
                let max_att: i64 = sqlx::query_scalar(
                    &format!("SELECT COALESCE(MAX(sync_attempts),0) FROM {table}"),
                ).fetch_one(pool).await.unwrap_or(0);
                let avg = sqlx::query_scalar::<_, f64>(
                    &format!("SELECT COALESCE(AVG(CAST(sync_attempts AS REAL)),0) FROM {table} WHERE sync_status = 'pending'"),
                ).fetch_one(pool).await.unwrap_or(0.0);
                let flag = if stuck > 0 { " ⚠ STUCK" } else if pending > 0 { " ⏳ pending" } else { " ✓ clean" };
                lines.push(format!(
                    "    {table}: {pending} pending, {stuck} stuck, max {max_att} att, avg {avg:.1}{flag}"
                ));
            }

            Ok(lines.join("\n"))
        }
        "sync_queue_list" => {
            let mut items = Vec::new();
            for table in crate::commands::sync_commands::SYNC_TABLES {
                let pk = crate::commands::sync_commands::table_pk(table);
                let sql = format!(
                    "SELECT {pk} AS _pk, sync_status, sync_attempts, created_at
                     FROM {table} WHERE sync_status IN ('pending', 'failed')
                     LIMIT 50",
                );
                if let Ok(rows) = sqlx::query(&sql).fetch_all(pool).await {
                    for r in &rows {
                        let id: String = r.get("_pk");
                        let status: String = r.get("sync_status");
                        let att: i64 = r.get("sync_attempts");
                        let at: String = r.get("created_at");
                        items.push(format!("{}|{}|{}|{}|{}", table, &id[..12.min(id.len())], status, att, &at[11..19]));
                    }
                }
            }
            if items.is_empty() { return Ok("[DB] Sync queue is empty — all events synced.".into()); }
            items.truncate(50);
            Ok(format!("[DB] Sync Queue (top 50):\n  table|entity|status|att|created\n  {}", items.join("\n  ")))
        }
        "get_active_shift" => {
            let shift: Option<(String, String, String, i64, String)> = sqlx::query_as(
                "SELECT s.shift_id, u.display_name, s.opened_at, s.opening_cash_minor, s.status
                 FROM shifts s JOIN users u ON u.user_id = s.cashier_user_id
                 WHERE s.status = 'open' AND s.device_id = (SELECT device_id FROM devices WHERE is_active=1 LIMIT 1)
                 ORDER BY s.opened_at DESC LIMIT 1",
            )
            .fetch_optional(pool).await?;
            match shift {
                Some((id, name, opened, float, status)) => {
                    Ok(format!("[DB] Active Shift: {} | Cashier: {} | Opened: {} | Float: {} fils | Status: {}",
                        &id[..12], name, &opened[11..19], float, status))
                }
                None => Ok("No active shift found. A shift must be opened before processing sales.".into()),
            }
        }
        "get_daily_report" => {
            let date = input
                .get("date")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing date".into()))?;
            let s = report_repo::today_summary(pool, branch_id, date).await?;
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            Ok(format!(
                "Sales Report — {date}:\n- Transactions: {}\n- Net Total: {}\n- Tax: {}\n- Discounts: {}\n- Cash: {}\n- Card: {}\n- Refunds: {} ({})",
                s.transaction_count,
                fmt(s.net_total_minor), fmt(s.tax_total_minor), fmt(s.discount_total_minor),
                fmt(s.cash_total_minor), fmt(s.card_total_minor),
                s.refund_count, fmt(s.refund_total_minor)
            ))
        }
        "get_date_range_report" => {
            let from = input
                .get("from")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing from".into()))?;
            let to = input
                .get("to")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing to".into()))?;
            let fmt = |n: i64| money::format_minor(n, currency_exp);

            let row = sqlx::query(
                "SELECT COUNT(*) AS cnt,
                        COALESCE(SUM(net_total_minor),      0) AS net,
                        COALESCE(SUM(tax_total_minor),      0) AS tax,
                        COALESCE(SUM(discount_total_minor), 0) AS discount
                 FROM sales
                 WHERE branch_id = ? AND business_date BETWEEN ? AND ? AND status != 'voided'",
            )
            .bind(branch_id)
            .bind(from)
            .bind(to)
            .fetch_one(pool)
            .await?;

            let cnt: i64 = row.get("cnt");
            let net: i64 = row.get("net");
            let tax: i64 = row.get("tax");
            let disc: i64 = row.get("discount");

            let cash: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(p.amount_minor),0) FROM payments p
                 JOIN sales s ON s.sale_id=p.sale_id
                 WHERE s.branch_id=? AND s.business_date BETWEEN ? AND ?
                   AND p.payment_method='cash' AND s.status!='voided'",
            )
            .bind(branch_id)
            .bind(from)
            .bind(to)
            .fetch_one(pool)
            .await?;

            let card: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(p.amount_minor),0) FROM payments p
                 JOIN sales s ON s.sale_id=p.sale_id
                 WHERE s.branch_id=? AND s.business_date BETWEEN ? AND ?
                   AND p.payment_method='card' AND s.status!='voided'",
            )
            .bind(branch_id)
            .bind(from)
            .bind(to)
            .fetch_one(pool)
            .await?;

            let refund_cnt: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM refunds r
                 JOIN sales s ON s.sale_id=r.original_sale_id
                 WHERE s.branch_id=? AND s.business_date BETWEEN ? AND ?",
            )
            .bind(branch_id)
            .bind(from)
            .bind(to)
            .fetch_one(pool)
            .await?;

            let refund_total: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(r.refund_total_minor),0) FROM refunds r
                 JOIN sales s ON s.sale_id=r.original_sale_id
                 WHERE s.branch_id=? AND s.business_date BETWEEN ? AND ?",
            )
            .bind(branch_id)
            .bind(from)
            .bind(to)
            .fetch_one(pool)
            .await?;

            Ok(format!(
                "Sales Report {from} → {to}:\n- Transactions: {cnt}\n- Net Total: {}\n- Tax: {}\n- Discounts: {}\n- Cash: {}\n- Card: {}\n- Refunds: {refund_cnt} ({})",
                fmt(net), fmt(tax), fmt(disc), fmt(cash), fmt(card), fmt(refund_total)
            ))
        }
        "get_top_products" => {
            let limit = input
                .get("limit")
                .and_then(|v| v.as_i64())
                .unwrap_or(10)
                .min(25);
            let period_days = input
                .get("period_days")
                .and_then(|v| v.as_i64())
                .unwrap_or(30)
                .min(365);
            let fmt = |n: i64| money::format_minor(n, currency_exp);

            let rows = sqlx::query(
                "SELECT p.name,
                        SUM(si.unit_price_minor * CAST(si.quantity AS REAL)) AS revenue,
                        COUNT(DISTINCT s.sale_id) AS txn_count
                 FROM sale_items si
                 JOIN sales s    ON s.sale_id    = si.sale_id
                 JOIN products p ON p.product_id = si.product_id
                 WHERE s.business_date >= date('now', ? || ' days') AND s.status != 'voided'
                 GROUP BY si.product_id, p.name
                 ORDER BY revenue DESC
                 LIMIT ?",
            )
            .bind(format!("-{}", period_days))
            .bind(limit)
            .fetch_all(pool)
            .await?;

            if rows.is_empty() {
                return Ok(format!("No sales data in the last {period_days} days."));
            }
            let lines: Vec<String> = rows
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    let name: String = r.get("name");
                    let rev: i64 = r.get("revenue");
                    let txn: i64 = r.get("txn_count");
                    format!("{}. {} — {} ({} transactions)", i + 1, name, fmt(rev), txn)
                })
                .collect();
            Ok(format!(
                "Top {} products (last {period_days} days):\n{}",
                rows.len(),
                lines.join("\n")
            ))
        }
        "get_shift_history" => {
            let limit = input
                .get("limit")
                .and_then(|v| v.as_i64())
                .unwrap_or(10)
                .min(30);
            let fmt = |n: i64| money::format_minor(n, currency_exp);

            let rows = sqlx::query(
                "SELECT s.shift_id, u.display_name AS cashier,
                        s.opened_at, s.closed_at, s.opening_cash_minor,
                        COALESCE((
                            SELECT SUM(net_total_minor) FROM sales
                            WHERE shift_id = s.shift_id AND status != 'voided'
                        ), 0) AS sales_total
                 FROM shifts s
                 LEFT JOIN users u ON u.user_id = s.cashier_user_id
                 ORDER BY s.opened_at DESC
                 LIMIT ?",
            )
            .bind(limit)
            .fetch_all(pool)
            .await?;

            if rows.is_empty() {
                return Ok("No shifts found.".into());
            }

            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let cashier: String = r
                        .get::<Option<String>, _>("cashier")
                        .unwrap_or_else(|| "Unknown".into());
                    let opened: String = r.get("opened_at");
                    let closed: Option<String> = r.get("closed_at");
                    let opening: i64 = r.get("opening_cash_minor");
                    let sales: i64 = r.get("sales_total");
                    let status = if closed.is_some() { "Closed" } else { "OPEN" };
                    format!(
                        "- {} [{status}] | Opened: {} | Float: {} | Sales: {}",
                        cashier,
                        &opened[..16.min(opened.len())],
                        fmt(opening),
                        fmt(sales)
                    )
                })
                .collect();
            Ok(format!(
                "{} recent shift(s):\n{}",
                rows.len(),
                lines.join("\n")
            ))
        }
        "list_categories" => {
            let rows = sqlx::query("SELECT category_id, name FROM categories ORDER BY name")
                .fetch_all(pool)
                .await?;

            if rows.is_empty() {
                return Ok("No categories found.".into());
            }
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let id: String = r.get("category_id");
                    let name: String = r.get("name");
                    format!("- {} (ID: {})", name, id)
                })
                .collect();
            Ok(format!("{} categories:\n{}", rows.len(), lines.join("\n")))
        }
        "list_safe_drops" => {
            let shift_id = input
                .get("shift_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing shift_id".into()))?;
            let fmt = |n: i64| money::format_minor(n, currency_exp);

            let rows = sqlx::query(
                "SELECT amount_minor, note, created_by_user_id, created_at
                 FROM cash_events
                 WHERE shift_id = ? AND event_type = 'safe_drop'
                 ORDER BY created_at",
            )
            .bind(shift_id)
            .fetch_all(pool)
            .await?;

            if rows.is_empty() {
                return Ok(format!(
                    "No safe drops recorded for shift {}.",
                    &shift_id[..8.min(shift_id.len())]
                ));
            }
            let total: i64 = rows.iter().map(|r| r.get::<i64, _>("amount_minor")).sum();
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let amt: i64 = r.get("amount_minor");
                    let note: Option<String> = r.get("note");
                    let at: String = r.get("created_at");
                    format!(
                        "- {} | {} | {}",
                        fmt(amt),
                        note.as_deref().unwrap_or("—"),
                        &at[..16.min(at.len())]
                    )
                })
                .collect();
            Ok(format!(
                "{} safe drop(s) | Total: {}\n{}",
                rows.len(),
                fmt(total),
                lines.join("\n")
            ))
        }
        "list_no_sale_events" => {
            let shift_id = input
                .get("shift_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing shift_id".into()))?;

            let rows = sqlx::query(
                "SELECT actor_user_id, note, created_at
                 FROM no_sale_events
                 WHERE shift_id = ?
                 ORDER BY created_at",
            )
            .bind(shift_id)
            .fetch_all(pool)
            .await?;

            if rows.is_empty() {
                return Ok(format!(
                    "No no-sale events recorded for shift {}.",
                    &shift_id[..8.min(shift_id.len())]
                ));
            }
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let actor: String = r.get("actor_user_id");
                    let note: Option<String> = r.get("note");
                    let at: String = r.get("created_at");
                    format!(
                        "- Actor: {} | {} | {}",
                        &actor[..8.min(actor.len())],
                        note.as_deref().unwrap_or("no note"),
                        &at[11..16.min(at.len())]
                    )
                })
                .collect();
            Ok(format!(
                "{} no-sale event(s):\n{}",
                rows.len(),
                lines.join("\n")
            ))
        }
        "get_audit_chain_status" => {
            let device_id: String = sqlx::query_scalar(
                "SELECT device_id FROM devices WHERE is_active = 1 ORDER BY device_code LIMIT 1",
            )
            .fetch_optional(pool)
            .await?
            .flatten()
            .unwrap_or_default();

            let r = crate::db::repositories::audit_hash::verify_chain(pool, &device_id).await?;
            let status = if r.ok {
                "✓ INTACT"
            } else {
                "⚠ ANOMALIES DETECTED"
            };
            Ok(format!(
                "Audit Chain [{status}]:\n- Total rows:   {}\n- Legacy rows:  {} (pre-chain, not verified)\n- Verified:     {}\n- Broken hash:  {}\n- Broken links: {}\n\n{}",
                r.total_rows, r.legacy_rows, r.verified, r.broken_hash, r.broken_link,
                if r.ok {
                    "Chain integrity confirmed — no tampering detected."
                } else {
                    "⚠ WARNING: Chain anomalies found. Contact your system administrator immediately."
                }
            ))
        }
        "get_hourly_sales" => {
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            let rows = sqlx::query(
                "SELECT strftime('%H', sold_at, 'localtime') AS hour,
                        COUNT(*) AS cnt,
                        COALESCE(SUM(net_total_minor), 0) AS net
                 FROM sales
                 WHERE business_date = ? AND status != 'voided'
                 GROUP BY hour
                 ORDER BY hour",
            )
            .bind(&today)
            .fetch_all(pool)
            .await?;

            if rows.is_empty() {
                return Ok(format!("No sales yet today ({today})."));
            }
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let hour: String = r.get("hour");
                    let cnt: i64 = r.get("cnt");
                    let net: i64 = r.get("net");
                    let h: u32 = hour.parse().unwrap_or(0);
                    let label = format!("{:02}:00–{:02}:59", h, h);
                    format!(
                        "  {} | {:>3} sale{} | BHD {}",
                        label,
                        cnt,
                        if cnt == 1 { "" } else { "s" },
                        fmt(net)
                    )
                })
                .collect();
            Ok(format!(
                "Hourly sales breakdown — {today}:\n{}",
                lines.join("\n")
            ))
        }
        "get_sales_by_category" => {
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            let rows = sqlx::query(
                "SELECT COALESCE(c.name, 'Uncategorised') AS category,
                        COUNT(DISTINCT s.sale_id) AS txn_count,
                        COALESCE(SUM(si.line_total_minor), 0) AS revenue
                 FROM sale_items si
                 JOIN sales s ON s.sale_id = si.sale_id
                 LEFT JOIN products p ON p.product_id = si.product_id
                 LEFT JOIN categories c ON c.category_id = p.category_id
                 WHERE s.business_date = ? AND s.status != 'voided'
                 GROUP BY c.category_id, c.name
                 ORDER BY revenue DESC",
            )
            .bind(&today)
            .fetch_all(pool)
            .await?;

            if rows.is_empty() {
                return Ok(format!("No sales today ({today})."));
            }
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let cat: String = r.get("category");
                    let txn: i64 = r.get("txn_count");
                    let rev: i64 = r.get("revenue");
                    format!(
                        "  {:.<30} BHD {} ({} txn{})",
                        format!("{cat} "),
                        fmt(rev),
                        txn,
                        if txn == 1 { "" } else { "s" }
                    )
                })
                .collect();
            Ok(format!(
                "Sales by category — {today}:\n{}",
                lines.join("\n")
            ))
        }
        "get_cashier_performance" => {
            let from = input
                .get("from")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing from".into()))?;
            let to = input
                .get("to")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing to".into()))?;
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            let rows = sqlx::query(
                "SELECT COALESCE(u.display_name, s.cashier_user_id) AS cashier,
                        COUNT(*) AS txn_count,
                        COALESCE(SUM(s.net_total_minor), 0) AS net_total,
                        COALESCE(SUM(s.discount_total_minor), 0) AS discounts,
                        COUNT(CASE WHEN s.status='voided' THEN 1 END) AS voids
                 FROM sales s
                 LEFT JOIN users u ON u.user_id = s.cashier_user_id
                 WHERE s.business_date BETWEEN ? AND ?
                 GROUP BY s.cashier_user_id, u.display_name
                 ORDER BY net_total DESC",
            )
            .bind(from)
            .bind(to)
            .fetch_all(pool)
            .await?;

            if rows.is_empty() {
                return Ok(format!("No sales between {from} and {to}."));
            }
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let name: String = r.get("cashier");
                    let txn: i64 = r.get("txn_count");
                    let net: i64 = r.get("net_total");
                    let disc: i64 = r.get("discounts");
                    let voids: i64 = r.get("voids");
                    format!(
                        "  {} — BHD {} | {} txns | BHD {} discounts | {} voids",
                        name,
                        fmt(net),
                        txn,
                        fmt(disc),
                        voids
                    )
                })
                .collect();
            Ok(format!(
                "Cashier performance {from} → {to}:\n{}",
                lines.join("\n")
            ))
        }
        // ── Customers ─────────────────────────────────────────────────────────
        "list_customers" => {
            let search = input
                .get("search")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let rows = if search.is_empty() {
                sqlx::query(
                    "SELECT customer_id, name, phone, email, loyalty_points
                     FROM customers ORDER BY name LIMIT 50",
                )
                .fetch_all(pool)
                .await?
            } else {
                sqlx::query(
                    "SELECT customer_id, name, phone, email, loyalty_points
                     FROM customers
                     WHERE name LIKE ? OR phone LIKE ?
                     ORDER BY name LIMIT 50",
                )
                .bind(format!("%{search}%"))
                .bind(format!("%{search}%"))
                .fetch_all(pool)
                .await?
            };
            if rows.is_empty() {
                return Ok("No customers found.".into());
            }
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let id: String = r.get("customer_id");
                    let name: String = r.get("name");
                    let phone: Option<String> = r.get("phone");
                    let pts: i64 = r.get("loyalty_points");
                    // PII-01: mask phone — only last 4 digits shown in AI context
                    let masked = phone.as_deref().map(mask_phone).unwrap_or_else(|| "—".into());
                    format!(
                        "- {} (ID: {}) | Phone: {} | Loyalty: {} pts",
                        name,
                        &id[..8.min(id.len())],
                        masked,
                        pts
                    )
                })
                .collect();
            Ok(format!("{} customer(s):\n{}", rows.len(), lines.join("\n")))
        }
        "get_customer" => {
            let customer_id = input
                .get("customer_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing customer_id".into()))?;
            let r = sqlx::query(
                "SELECT customer_id, name, phone, email, loyalty_points, notes, created_at
                 FROM customers WHERE customer_id = ?",
            )
            .bind(customer_id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Customer not found".into()))?;
            let name: String = r.get("name");
            let phone: Option<String> = r.get("phone");
            let email: Option<String> = r.get("email");
            let pts: i64 = r.get("loyalty_points");
            let notes: Option<String> = r.get("notes");
            let at: String = r.get("created_at");
            // PII-01: mask phone — only last 4 digits shown in AI context
            let masked_phone = phone.as_deref().map(mask_phone).unwrap_or_else(|| "—".into());
            Ok(format!(
                "Customer: {name}\nID: {customer_id}\nPhone: {}\nEmail: {}\nLoyalty: {pts} pts\nNotes: {}\nSince: {}",
                masked_phone,
                email.as_deref().unwrap_or("—"),
                notes.as_deref().unwrap_or("—"),
                &at[..10.min(at.len())]
            ))
        }
        // ── Deliveries ────────────────────────────────────────────────────────
        "list_deliveries" => {
            let status_filter = input.get("status").and_then(|v| v.as_str());
            let limit = input
                .get("limit")
                .and_then(|v| v.as_i64())
                .unwrap_or(20)
                .min(50);
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            let rows = if let Some(status) = status_filter {
                sqlx::query(
                    "SELECT d.delivery_id, d.delivery_status, d.delivery_staff_name,
                            d.payment_status, d.amount_minor, d.created_at
                     FROM delivery_orders d
                     WHERE d.delivery_status = ?
                     ORDER BY d.created_at DESC LIMIT ?",
                )
                .bind(status)
                .bind(limit)
                .fetch_all(pool)
                .await?
            } else {
                sqlx::query(
                    "SELECT d.delivery_id, d.delivery_status, d.delivery_staff_name,
                            d.payment_status, d.amount_minor, d.created_at
                     FROM delivery_orders d
                     ORDER BY d.created_at DESC LIMIT ?",
                )
                .bind(limit)
                .fetch_all(pool)
                .await?
            };
            if rows.is_empty() {
                return Ok("No deliveries found.".into());
            }
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let id: String = r.get("delivery_id");
                    let status: String = r.get("delivery_status");
                    let rider: Option<String> = r.get("delivery_staff_name");
                    let pay: String = r.get("payment_status");
                    let total: i64 = r.get("amount_minor");
                    let at: String = r.get("created_at");
                    format!(
                        "- {} | {} | Rider: {} | Pay: {} | BHD {} | {}",
                        &id[..8.min(id.len())],
                        status,
                        rider.as_deref().unwrap_or("—"),
                        pay,
                        fmt(total),
                        &at[..16.min(at.len())]
                    )
                })
                .collect();
            Ok(format!(
                "{} delivery/deliveries:\n{}",
                rows.len(),
                lines.join("\n")
            ))
        }
        // ── Staff ─────────────────────────────────────────────────────────────
        "list_users" => {
            let rows = sqlx::query(
                "SELECT u.user_id, u.display_name, u.username, r.name AS role_name, u.is_active
                 FROM users u
                 JOIN roles r ON r.role_id = u.role_id
                 ORDER BY u.is_active DESC, u.display_name",
            )
            .fetch_all(pool)
            .await?;
            if rows.is_empty() {
                return Ok("No users found.".into());
            }
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let name: String = r.get("display_name");
                    let uname: String = r.get("username");
                    let role: String = r.get("role_name");
                    let active: bool = r.get("is_active");
                    format!(
                        "- {} (@{}) | {} | {}",
                        name,
                        uname,
                        role,
                        if active { "Active" } else { "Inactive" }
                    )
                })
                .collect();
            Ok(format!("{} user(s):\n{}", rows.len(), lines.join("\n")))
        }
        // ── Stock movements ───────────────────────────────────────────────────
        "get_stock_movements" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let limit = input
                .get("limit")
                .and_then(|v| v.as_i64())
                .unwrap_or(20)
                .min(50);
            let rows = sqlx::query(
                "SELECT movement_type, quantity_delta, quantity_after, notes, created_at
                 FROM stock_movements
                 WHERE product_id = ?
                 ORDER BY created_at DESC LIMIT ?",
            )
            .bind(product_id)
            .bind(limit)
            .fetch_all(pool)
            .await?;
            if rows.is_empty() {
                return Ok("No stock movements found for this product.".into());
            }
            // Get product name for context
            let pname: Option<String> = sqlx::query_scalar(
                "SELECT name FROM products WHERE product_id = ?",
            )
            .bind(product_id)
            .fetch_optional(pool)
            .await?;
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let mtype: String = r.get("movement_type");
                    let delta: f64 = r.get("quantity_delta");
                    let after: f64 = r.get("quantity_after");
                    let notes: Option<String> = r.get("notes");
                    let at: String = r.get("created_at");
                    format!(
                        "- {} | {:+.3} → {:.3} | {} | {}",
                        mtype,
                        delta,
                        after,
                        notes.as_deref().unwrap_or("—"),
                        &at[..16.min(at.len())]
                    )
                })
                .collect();
            Ok(format!(
                "Stock movements for {} ({}):\n{}",
                pname.as_deref().unwrap_or(product_id),
                rows.len(),
                lines.join("\n")
            ))
        }
        // ── Tax report ────────────────────────────────────────────────────────
        "get_tax_report" => {
            let from = input
                .get("from")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing from".into()))?;
            let to = input
                .get("to")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing to".into()))?;
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            let rows = sqlx::query(
                "SELECT business_date AS day,
                        COUNT(*) AS transaction_count,
                        COALESCE(SUM(tax_total_minor), 0) AS tax_minor
                 FROM sales
                 WHERE branch_id = ? AND business_date BETWEEN ? AND ?
                   AND status != 'voided'
                 GROUP BY business_date
                 ORDER BY business_date ASC",
            )
            .bind(branch_id)
            .bind(from)
            .bind(to)
            .fetch_all(pool)
            .await?;
            if rows.is_empty() {
                return Ok(format!("No tax data between {from} and {to}."));
            }
            let mut cumulative = 0i64;
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let day: String = r.get("day");
                    let tax: i64 = r.get("tax_minor");
                    let txn: i64 = r.get("transaction_count");
                    cumulative += tax;
                    format!(
                        "  {} | {} txns | Tax: BHD {} | Cumulative: BHD {}",
                        day,
                        txn,
                        fmt(tax),
                        fmt(cumulative)
                    )
                })
                .collect();
            Ok(format!(
                "Tax Report {from} → {to}:\n{}\nTotal tax collected: BHD {}",
                lines.join("\n"),
                fmt(cumulative)
            ))
        }
        // ── Free web search (DuckDuckGo lite — no API key) ────────────────────
        "web_search" => {
            let query = input
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing query".into()))?;
            let max_results = input
                .get("max_results")
                .and_then(|v| v.as_i64())
                .unwrap_or(5)
                .min(10) as usize;
            duckduckgo_search(query, max_results).await
        }
        "search_market_prices" => {
            let product = input
                .get("product_name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_name".into()))?;
            let location = input
                .get("location")
                .and_then(|v| v.as_str())
                .unwrap_or("Bahrain");
            let query = format!("{product} price {location} BHD supermarket shop store");
            duckduckgo_search(&query, 6).await
        }
        // ── Smart barcode lookup (OFFF + web fallback) ─────────────────────────
        "smart_barcode_lookup" => {
            let barcode = input.get("barcode").and_then(|v| v.as_str()).unwrap_or("").trim();
            if barcode.is_empty() || barcode.len() < 8 || barcode.len() > 14 || !barcode.chars().all(|c| c.is_ascii_digit()) {
                return Err(AppError::Validation("barcode must be 8-14 digits (UPC/EAN)".into()));
            }
            // First try Open Food Facts
            let off_result = match open_food_facts_lookup(barcode).await {
                Ok(result) if !result.contains("not found") && !result.contains("product data unavailable") => Some(result),
                _ => None,
            };
            // If OFFF failed or got sparse data, search the web
            let web_results = if off_result.is_some() {
                None // Got good OFFF data, skip web search
            } else {
                match duckduckgo_search(&format!("barcode {barcode} product name"), 3).await {
                    Ok(r) if !r.contains("No results found") => Some(r),
                    _ => None,
                }
            };
            // Capture name hint before consuming the results
            let name_hint = off_result.as_ref().or(web_results.as_ref()).and_then(|r| {
                let needle = "Product: ";
                r.find(needle).map(|i| {
                    let start = i + needle.len();
                    let end = r[start..].find('\n').map(|e| start + e).unwrap_or(r.len());
                    r[start..end].trim().to_string()
                })
            });
            let has_off = off_result.is_some();
            let has_web = web_results.is_some();
            let mut lines = vec![format!("[SCAN] **Smart Barcode Lookup: {barcode}**"), String::new()];
            if let Some(off) = off_result {
                lines.push("### Open Food Facts Data".into());
                lines.push(off);
            }
            if let Some(web) = web_results {
                lines.push(String::new());
                lines.push("### Web Search Results (cross-reference)".into());
                lines.push(web);
            }
            if !has_off && !has_web {
                lines.push("This barcode was not found in Open Food Facts and web search returned no results.".into());
                lines.push("Try searching by product name instead, or manually enter the product details.".into());
            }
            // Append product creation instructions if we found a name
            if let Some(ref name) = name_hint {
                if !name.is_empty() && name != "—" {
                    lines.push(String::new());
                    lines.push(format!("### Suggested Category: {}", categorize_product(&name)));
                    lines.push(String::new());
                    lines.push("📋 **To create this product, I need:**".into());
                    lines.push("- Product name (extracted from lookup above)".into());
                    lines.push("- Selling price in BHD".into());
                    lines.push("- Category ID (use `list_categories` to pick the best fit)".into());
                    lines.push(String::new());
                    lines.push("Reply with: \"Create it at BHD X.XXX\" and I'll create the product for you.".into());
                }
            }
            Ok(lines.join("\n"))
        }
        // ── Multi-store price comparison ────────────────────────────────────────
        "compare_store_prices" => {
            let product = input.get("product_name").and_then(|v| v.as_str()).unwrap_or("");
            let location = input.get("location").and_then(|v| v.as_str()).unwrap_or("Bahrain");
            // Search multiple stores in parallel
            let stores = [
                ("Lulu Hypermarket", format!("{product} price luluhypermarket.com bahrain BHD")),
                ("Carrefour Bahrain", format!("{product} price carrefourbahrain.com BHD")),
                ("Alosra Supermarket", format!("{product} price alosra bahrain BHD")),
                ("Talabat Mart", format!("{product} talabat bahrain price BHD")),
                ("General Search", format!("{product} price {location} BHD supermarket")),
            ];
            let mut results: Vec<(String, String)> = Vec::new();
            for (store, query) in &stores {
                match duckduckgo_search(query, 3).await {
                    Ok(r) if !r.contains("No results found") => {
                        // Truncate each store's results
                        let short: String = r.lines().take(8).collect::<Vec<_>>().join("\n");
                        results.push((store.to_string(), short));
                    }
                    _ => {}
                }
            }
            let mut out = vec![format!("[WEB] **Price Comparison: \"{}\" in {}**", product, location), String::new()];
            if results.is_empty() {
                out.push("No prices found across the checked stores. Try a more specific product name or search manually on the store websites.".into());
            } else {
                for (store, content) in &results {
                    out.push(format!("#### {}", store));
                    out.push(content.clone());
                    out.push(String::new());
                }
                out.push("---".into());
                out.push("**Tip:** The AI does not have real-time API access to these stores. Prices shown are from recent web search results. For live prices, visit the store websites directly.".into());
                out.push("To set a price in your POS based on this research, use `create_product` or `update_product_price`.".into());
            }
            Ok(out.join("\n"))
        }
        // ── Bahrain grocery delivery price check ────────────────────────────────
        "bahrain_market_price_check" => {
            let product = input.get("product_name").and_then(|v| v.as_str()).unwrap_or("");
            let max = input.get("max_results").and_then(|v| v.as_i64()).unwrap_or(3).min(5) as usize;
            let sources = [
                ("Talabat / Talabat Mart", format!("\"{product}\" site:talabat.com bahrain")),
                ("Lulu Online", format!("\"{product}\" price luluhypermarket bahrain BHD")),
            ];
            let mut out = vec![format!("[WEB] **Bahrain Market Check: \"{}\"**", product), String::new()];
            let mut found_any = false;
            for (label, query) in &sources {
                match duckduckgo_search(query, max).await {
                    Ok(r) if !r.contains("No results found") => {
                        let short: String = r.lines().take(6).collect::<Vec<_>>().join("\n");
                        out.push(format!("#### {}", label));
                        out.push(short);
                        out.push(String::new());
                        found_any = true;
                    }
                    _ => {
                        out.push(format!("#### {} — no results", label));
                        out.push(String::new());
                    }
                }
            }
            if !found_any {
                out.push("No current listings found on these platforms. The product may not be listed on delivery apps, or the name may need to be more specific.".into());
            }
            out.push("---".into());
            out.push("These are delivery-platform prices which may include markup. Store shelf prices are typically 5-15% lower.".into());
            Ok(out.join("\n"))
        }
        // ── Free URL reader via Jina.ai Reader (no API key) ───────────────────
        "fetch_url" => {
            let url = input
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing url".into()))?;
            if !url.starts_with("http://") && !url.starts_with("https://") {
                return Err(AppError::Validation(
                    "URL must start with http:// or https://".into(),
                ));
            }
            jina_fetch(url).await
        }
        // ── Barcode lookup via Open Food Facts (no API key) ───────────────────
        "lookup_barcode" => {
            let barcode = input
                .get("barcode")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing barcode".into()))?
                .trim();
            // Validate: only digits, 8-14 chars
            if barcode.is_empty()
                || barcode.len() < 8
                || barcode.len() > 14
                || !barcode.chars().all(|c| c.is_ascii_digit())
            {
                return Err(AppError::Validation(
                    "barcode must be 8-14 digits (UPC/EAN)".into(),
                ));
            }
            open_food_facts_lookup(barcode).await
        }
        // ── Live exchange rates via Frankfurter ECB (no API key) ──────────────
        "get_exchange_rates" => {
            let currencies = input
                .get("currencies")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .map(|s| s.to_uppercase())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            frankfurter_rates(&currencies).await
        }
        // ── Prayer times via Aladhan (no API key) ─────────────────────────────
        "get_prayer_times" => {
            let date = input
                .get("date")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            aladhan_prayer_times(&date).await
        }
        // ── Bahrain public holidays via nager.date (no API key) ───────────────
        "get_bahrain_holidays" => {
            let current_year = chrono::Local::now().format("%Y").to_string().parse::<i64>().unwrap_or(2026);
            let year = input
                .get("year")
                .and_then(|v| v.as_i64())
                .unwrap_or(current_year);
            nager_bahrain_holidays(year as u16).await
        }
        // ── Roles ─────────────────────────────────────────────────────────────
        "list_roles" => {
            let rows = sqlx::query("SELECT role_id, name FROM roles ORDER BY name")
                .fetch_all(pool).await?;
            if rows.is_empty() { return Ok("No roles found.".into()); }
            let lines: Vec<String> = rows.iter().map(|r| {
                let id: String = r.get("role_id");
                let name: String = r.get("name");
                format!("- {} ({})", name, id)
            }).collect();
            Ok(format!("[DB] {} roles:\n{}", rows.len(), lines.join("\n")))
        }
        // ── Tax rules ─────────────────────────────────────────────────────────
        "list_tax_rules" => {
            let rows = sqlx::query(
                "SELECT tax_rule_id, name, rate_basis_points, inclusive, is_active FROM tax_rules ORDER BY name"
            ).fetch_all(pool).await?;
            if rows.is_empty() { return Ok("[DB] No tax rules defined.".into()); }
            let lines: Vec<String> = rows.iter().map(|r| {
                let id: String = r.get("tax_rule_id");
                let name: String = r.get("name");
                let bp: i64 = r.get("rate_basis_points");
                let inclusive: bool = r.get("inclusive");
                let active: bool = r.get("is_active");
                let pct = bp as f64 / 100.0;
                format!("- {} (ID: {}) — {}% {} — {}",
                    name, id, pct,
                    if inclusive { "inclusive" } else { "exclusive" },
                    if active { "Active" } else { "Inactive" })
            }).collect();
            Ok(format!("[DB] {} tax rule(s):\n{}", rows.len(), lines.join("\n")))
        }
        // ── Store settings ────────────────────────────────────────────────────
        "get_store_settings" => {
            let r = sqlx::query(
                "SELECT branch_id, name, timezone, address, phone, receipt_header, receipt_footer, tax_number, cr_number FROM branches WHERE is_active=1 LIMIT 1"
            ).fetch_optional(pool).await?
            .ok_or_else(|| AppError::NotFound("No active branch found".into()))?;
            let _id: String = r.get("branch_id");
            let name: String = r.get("name");
            let tz: Option<String> = r.get("timezone");
            let addr: Option<String> = r.get("address");
            let phone: Option<String> = r.get("phone");
            let rhead: Option<String> = r.get("receipt_header");
            let rfoot: Option<String> = r.get("receipt_footer");
            let tax: Option<String> = r.get("tax_number");
            let cr: Option<String> = r.get("cr_number");
            Ok(format!(
                "[DB] Store Settings:\n- Name: {}\n- Timezone: {}\n- Address: {}\n- Phone: {}\n- Tax/VAT Number: {}\n- CR Number: {}\n- Receipt Header: {}\n- Receipt Footer: {}",
                name,
                tz.as_deref().unwrap_or("—"),
                addr.as_deref().unwrap_or("—"),
                phone.as_deref().unwrap_or("—"),
                tax.as_deref().unwrap_or("—"),
                cr.as_deref().unwrap_or("—"),
                rhead.as_deref().unwrap_or("—"),
                rfoot.as_deref().unwrap_or("—")
            ))
        }
        // ── Business rules ────────────────────────────────────────────────────
        "get_business_rules" => {
            let neg = sqlx::query_scalar::<_, Option<String>>("SELECT value FROM app_config WHERE key='flag_allow_negative_stock'")
                .fetch_optional(pool).await?.flatten().unwrap_or_default() == "1";
            let dis = sqlx::query_scalar::<_, Option<String>>("SELECT value FROM app_config WHERE key='flag_require_discount_reason'")
                .fetch_optional(pool).await?.flatten().unwrap_or_default() == "1";
            let cc = sqlx::query_scalar::<_, Option<String>>("SELECT value FROM app_config WHERE key='flag_cashier_can_discount'")
                .fetch_optional(pool).await?.flatten().unwrap_or_default() == "1";
            let ap = sqlx::query_scalar::<_, Option<String>>("SELECT value FROM app_config WHERE key='flag_auto_print_receipt'")
                .fetch_optional(pool).await?.flatten().unwrap_or_default() == "1";
            Ok(format!(
                "[DB] Business Rules:\n- Allow negative stock: {}\n- Require discount reason: {}\n- Cashier can discount: {}\n- Auto-print receipt: {}",
                if neg { "Yes" } else { "No" },
                if dis { "Yes" } else { "No" },
                if cc { "Yes" } else { "No" },
                if ap { "Yes" } else { "No" }
            ))
        }
        // ── Devices ───────────────────────────────────────────────────────────
        "list_devices" => {
            let rows = sqlx::query(
                "SELECT device_id, device_code, is_active, created_at FROM devices ORDER BY device_code"
            ).fetch_all(pool).await?;
            if rows.is_empty() { return Ok("[DB] No devices registered.".into()); }
            let lines: Vec<String> = rows.iter().map(|r| {
                let id: String = r.get("device_id");
                let code: String = r.get("device_code");
                let active: bool = r.get("is_active");
                let created: Option<String> = r.get("created_at");
                format!("- {} ({}) — {} — Created: {}",
                    code, &id[..8.min(id.len())],
                    if active { "Active" } else { "Inactive" },
                    created.as_deref().map(|s| &s[..10.min(s.len())]).unwrap_or("never"))
            }).collect();
            Ok(format!("[DB] {} device(s):\n{}", rows.len(), lines.join("\n")))
        }
        // ── Session timeout ───────────────────────────────────────────────────
        "get_session_timeout" => {
            let mins: Option<String> = sqlx::query_scalar(
                "SELECT value FROM app_config WHERE key = 'idle_timeout_minutes'"
            ).fetch_optional(pool).await?.flatten();
            let timeout = mins.and_then(|v| v.parse::<i64>().ok()).unwrap_or(5);
            Ok(format!("[DB] Session timeout: {} minutes ({}).", timeout,
                if timeout == 0 { "never locks" } else { "auto-locks after idle" }))
        }
        name => crate::ai::tools_read_ext::execute(pool, name, input, branch_id, currency_exp).await,
    }
}

// ── DuckDuckGo free search (no API key) ───────────────────────────────────────

async fn duckduckgo_search(query: &str, max_results: usize) -> AppResult<String> {
    // Use DuckDuckGo lite HTML endpoint — free, no auth
    let encoded: String = query
        .chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => '+'.to_string(),
            c => format!("%{:02X}", c as u32),
        })
        .collect();

    let url = format!("https://lite.duckduckgo.com/lite/?q={encoded}");

    let http = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Internal(format!("HTTP client error: {e}")))?;

    let html = http
        .get(&url)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Search request failed: {e}")))?
        .text()
        .await
        .map_err(|e| AppError::Internal(format!("Search response read failed: {e}")))?;

    // Parse DDG lite HTML: results are in table rows with class "result-link" and "result-snippet"
    let mut results: Vec<(String, String, String)> = Vec::new(); // (title, url, snippet)

    // Extract result links: <a class="result-link" href="...">Title</a>
    let mut pos = 0;
    while results.len() < max_results {
        // Find next result-link anchor
        let search_str = "class=\"result-link\"";
        match html[pos..].find(search_str) {
            None => break,
            Some(rel) => {
                let abs = pos + rel;
                // Find href
                let href_start = html[..abs].rfind('<').unwrap_or(abs);
                let extra = 200.min(html.len().saturating_sub(abs + search_str.len()));
                let tag_text = &html[href_start..abs + search_str.len() + extra];

                // Extract href value
                let current_url = if let Some(h) = tag_text.find("href=\"") {
                    let after = h + 6;
                    let end = tag_text[after..].find('"').map(|e| after + e).unwrap_or(after);
                    let raw = &tag_text[after..end];
                    // DDG lite hrefs are relative like //duckduckgo.com/l/?uddg=...
                    if raw.starts_with("//") {
                        format!("https:{raw}")
                    } else {
                        raw.to_string()
                    }
                } else {
                    String::new()
                };

                // Extract link text (between > and </a>)
                let current_title = if let Some(gt) = html[abs..].find('>') {
                    let after = abs + gt + 1;
                    let close = html[after..].find("</a>").map(|e| after + e).unwrap_or(after);
                    strip_html_tags(&html[after..close]).trim().to_string()
                } else {
                    String::new()
                };

                pos = abs + search_str.len();

                // Find the snippet that follows (next result-snippet td)
                let snippet_tag = "class=\"result-snippet\"";
                let snippet = if let Some(srel) = html[pos..].find(snippet_tag) {
                    let sabs = pos + srel;
                    if let Some(gt) = html[sabs..].find('>') {
                        let after = sabs + gt + 1;
                        let close = html[after..].find("</td>").map(|e| after + e).unwrap_or(after);
                        strip_html_tags(&html[after..close]).trim().to_string()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };

                if !current_title.is_empty() {
                    results.push((current_title, current_url, snippet));
                }
            }
        }
    }

    if results.is_empty() {
        return Ok(format!(
            "No results found for '{}'. Try rephrasing the query.",
            query
        ));
    }

            let lines: Vec<String> = results
                .iter()
                .enumerate()
                .map(|(i, (title, url, snippet))| {
                    let mut parts = vec![format!("{}. **{}**", i + 1, title)];
                    if !url.is_empty() {
                        parts.push(format!("   [LINK] {url}"));
                    }
                    if !snippet.is_empty() {
                        parts.push(format!("   {snippet}"));
                    }
                    parts.join("\n")
                })
                .collect();

            Ok(format!(
                "[WEB] Results for: \"{query}\"\n\n{}",
                lines.join("\n\n")
            ))
}

// ── Jina.ai Reader: fetch any URL as clean text (free, no API key) ────────────

async fn jina_fetch(url: &str) -> AppResult<String> {
    // Prefix any URL with https://r.jina.ai/ to get clean markdown back
    let jina_url = format!("https://r.jina.ai/{url}");

    let http = reqwest::Client::builder()
        .user_agent("ZANPOS/1.0 (POS AI assistant)")
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| AppError::Internal(format!("HTTP client error: {e}")))?;

    let resp = http
        .get(&jina_url)
        .header("Accept", "text/plain")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("fetch_url request failed: {e}")))?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| AppError::Internal(format!("fetch_url read failed: {e}")))?;

    if !status.is_success() {
        return Err(AppError::Internal(format!(
            "fetch_url returned HTTP {status}: {}",
            &text[..text.len().min(200)]
        )));
    }

    // Truncate to ~6000 chars so we don't overflow the AI context
    let truncated = if text.len() > 6000 {
        format!("{}\n\n[…content truncated at 6000 chars…]", &text[..6000])
    } else {
        text
    };

    Ok(format!("[WEB] Page content from {url}:\n\n{truncated}"))
}

// ── Open Food Facts barcode lookup (free, no API key) ─────────────────────────

async fn open_food_facts_lookup(barcode: &str) -> AppResult<String> {
    let url = format!("https://world.openfoodfacts.net/api/v2/product/{barcode}");

    let http = reqwest::Client::builder()
        .user_agent("ZANPOS/1.0 (POS barcode lookup; contact zanabal.nowshad@gmail.com)")
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Internal(format!("HTTP client error: {e}")))?;

    let resp: Value = http
        .get(&url)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Barcode lookup request failed: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("Barcode lookup JSON parse failed: {e}")))?;

    let status = resp.get("status").and_then(|v| v.as_i64()).unwrap_or(0);
    if status == 0 {
        return Ok(format!(
            "Barcode {barcode} not found in Open Food Facts database. \
             This may be a local/regional product not yet submitted to the open database."
        ));
    }

    let product = match resp.get("product") {
        Some(p) => p,
        None => {
            return Ok(format!(
                "Barcode {barcode}: product data unavailable in response."
            ))
        }
    };

    let s = |key: &str| -> &str {
        product
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
    };

    let name        = s("product_name");
    let brand       = s("brands");
    let categories  = s("categories");
    let quantity    = s("quantity");
    let countries   = s("countries");
    let ingredients = s("ingredients_text");

    // Nutrition per 100g
    let nut = product.get("nutriments");
    let nutriments = if let Some(n) = nut {
        let energy  = n.get("energy-kcal_100g").and_then(|v| v.as_f64());
        let fat     = n.get("fat_100g").and_then(|v| v.as_f64());
        let carbs   = n.get("carbohydrates_100g").and_then(|v| v.as_f64());
        let protein = n.get("proteins_100g").and_then(|v| v.as_f64());
        let mut parts = vec![];
        if let Some(e) = energy  { parts.push(format!("{e:.0} kcal")); }
        if let Some(f) = fat     { parts.push(format!("fat {f:.1}g")); }
        if let Some(c) = carbs   { parts.push(format!("carbs {c:.1}g")); }
        if let Some(p) = protein { parts.push(format!("protein {p:.1}g")); }
        if parts.is_empty() { String::new() } else { format!("Per 100g: {}", parts.join(", ")) }
    } else {
        String::new()
    };

    let mut lines = vec![format!("[WEB] **Barcode {barcode}**")];
    if !name.is_empty()        { lines.push(format!("Product: {name}")); }
    if !brand.is_empty()       { lines.push(format!("Brand: {brand}")); }
    if !quantity.is_empty()    { lines.push(format!("Size/Qty: {quantity}")); }
    if !categories.is_empty()  { lines.push(format!("Categories: {}", &categories[..categories.len().min(120)])); }
    if !countries.is_empty()   { lines.push(format!("Sold in: {countries}")); }
    if !nutriments.is_empty()  { lines.push(nutriments); }
    if !ingredients.is_empty() { lines.push(format!("Ingredients: {}", &ingredients[..ingredients.len().min(300)])); }

    Ok(lines.join("\n"))
}

// ── Frankfurter ECB currency rates (free, no API key) ─────────────────────────

async fn frankfurter_rates(currencies: &[String]) -> AppResult<String> {
    // Base: BHD. Frankfurter uses ECB rates (updated daily on working days).
    let symbols_param = if currencies.is_empty() {
        // Default useful set for Bahrain importers
        "USD,EUR,GBP,SAR,AED,KWD,QAR,INR,CNY".to_string()
    } else {
        currencies.join(",")
    };

    let url = format!(
        "https://api.frankfurter.dev/v1/latest?base=BHD&symbols={symbols_param}"
    );

    let http = reqwest::Client::builder()
        .user_agent("ZANPOS/1.0")
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Internal(format!("HTTP client error: {e}")))?;

    let resp: Value = http
        .get(&url)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Exchange rate request failed: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("Exchange rate JSON parse failed: {e}")))?;

    let date = resp
        .get("date")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown date");

    let rates = match resp.get("rates").and_then(|v| v.as_object()) {
        Some(r) => r,
        None => {
            return Ok(
                "Exchange rate data unavailable. Frankfurter API may not support BHD as base. \
                 BHD is pegged to USD at 1 BHD = 2.6595 USD."
                    .to_string(),
            )
        }
    };

    let mut lines = vec![format!("[WEB] **Exchange rates (base: 1 BHD) — {date}**")];
    lines.push("Source: European Central Bank via Frankfurter".to_string());
    lines.push(String::new());

    let mut sorted: Vec<(&String, f64)> = rates
        .iter()
        .filter_map(|(k, v)| v.as_f64().map(|f| (k, f)))
        .collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));

    for (currency, rate) in &sorted {
        lines.push(format!("  1 BHD = {rate:.4} {currency}"));
    }

    // Always add USD peg note
    lines.push(String::new());
    lines.push("Note: BHD is officially pegged to USD at 1 BHD ≈ 2.6595 USD.".to_string());

    Ok(lines.join("\n"))
}

// ── Aladhan prayer times for Manama Bahrain (free, no API key) ────────────────

async fn aladhan_prayer_times(date_str: &str) -> AppResult<String> {
    // Use timingsByCity endpoint — Manama, Bahrain, method 2 (ISNA)
    let url = if date_str.is_empty() {
        "https://api.aladhan.com/v1/timingsByCity?city=Manama&country=BH&method=2".to_string()
    } else {
        // date_str in DD-MM-YYYY
        format!(
            "https://api.aladhan.com/v1/timingsByCity/{date_str}?city=Manama&country=BH&method=2"
        )
    };

    let http = reqwest::Client::builder()
        .user_agent("ZANPOS/1.0")
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Internal(format!("HTTP client error: {e}")))?;

    let resp: Value = http
        .get(&url)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Prayer times request failed: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("Prayer times JSON parse failed: {e}")))?;

    let code = resp.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
    if code != 200 {
        let msg = resp
            .get("data")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        return Ok(format!("Prayer times unavailable: {msg}"));
    }

    let timings = match resp.pointer("/data/timings") {
        Some(t) => t,
        None => return Ok("Prayer times data not found in response.".to_string()),
    };

    let date_info = resp
        .pointer("/data/date/readable")
        .and_then(|v| v.as_str())
        .unwrap_or(date_str);

    let s = |key: &str| -> &str {
        timings
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("--:--")
    };

    let lines = vec![
        format!("[WEB] **Prayer Times — Manama, Bahrain ({date_info})**"),
        String::new(),
        format!("🌅 Fajr    : {}", s("Fajr")),
        format!("🌄 Sunrise : {}", s("Sunrise")),
        format!("☀️ Dhuhr   : {}", s("Dhuhr")),
        format!("🌇 Asr     : {}", s("Asr")),
        format!("🌆 Maghrib : {}", s("Maghrib")),
        format!("🌃 Isha    : {}", s("Isha")),
        String::new(),
        "Times are local Bahrain time (AST, UTC+3).".to_string(),
    ];

    Ok(lines.join("\n"))
}

// ── Nager.date Bahrain public holidays (free, no API key) ─────────────────────

async fn nager_bahrain_holidays(year: u16) -> AppResult<String> {
    let url = format!("https://date.nager.at/api/v3/PublicHolidays/{year}/BH");

    let http = reqwest::Client::builder()
        .user_agent("ZANPOS/1.0")
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Internal(format!("HTTP client error: {e}")))?;

    let resp = http
        .get(&url)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Holidays request failed: {e}")))?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| AppError::Internal(format!("Holidays read failed: {e}")))?;

    if !status.is_success() {
        return Ok(format!(
            "Could not load Bahrain holidays for {year} (HTTP {status})."
        ));
    }

    let holidays: Vec<Value> = serde_json::from_str(&text).map_err(|e| {
        AppError::Internal(format!("Holidays JSON parse failed: {e}"))
    })?;

    if holidays.is_empty() {
        return Ok(format!(
            "No public holidays found for Bahrain in {year} (data may not yet be available)."
        ));
    }

    let mut lines = vec![format!("[WEB] **Bahrain Public Holidays {year}**"), String::new()];

    for h in &holidays {
        let date = h.get("date").and_then(|v| v.as_str()).unwrap_or("?");
        let name = h
            .get("localName")
            .and_then(|v| v.as_str())
            .or_else(|| h.get("name").and_then(|v| v.as_str()))
            .unwrap_or("Holiday");
        lines.push(format!("  📅 {date}  —  {name}"));
    }

    lines.push(String::new());
    lines.push("Source: nager.date (official Bahrain calendar).".to_string());

    Ok(lines.join("\n"))
}

/// Remove HTML tags from a string slice.
/// Guess a product category from its name using keyword heuristics.
/// Used by smart_barcode_lookup to suggest a category_id to the AI.
fn categorize_product(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower.contains("milk") || lower.contains("laban") || lower.contains("yogurt") || lower.contains("cheese") || lower.contains("cream") || lower.contains("butter") {
        return "Dairy".into();
    }
    if lower.contains("bread") || lower.contains("roti") || lower.contains("bun") || lower.contains("croissant") || lower.contains("bakery") {
        return "Bakery".into();
    }
    if lower.contains("water") || lower.contains("juice") || lower.contains("pepsi") || lower.contains("coca") || lower.contains("soda") || lower.contains("drink") || lower.contains("tea") || lower.contains("coffee") {
        return "Beverages".into();
    }
    if lower.contains("rice") || lower.contains("flour") || lower.contains("sugar") || lower.contains("oil") || lower.contains("salt") || lower.contains("spice") || lower.contains("grain") || lower.contains("lentil") || lower.contains("dal") || lower.contains("pasta") || lower.contains("noodle") {
        return "Groceries".into();
    }
    if lower.contains("chicken") || lower.contains("meat") || lower.contains("beef") || lower.contains("mutton") || lower.contains("fish") || lower.contains("shrimp") || lower.contains("egg") || lower.contains("sausage") {
        return "Meat & Poultry".into();
    }
    if lower.contains("fruit") || lower.contains("apple") || lower.contains("banana") || lower.contains("orange") || lower.contains("vegetable") || lower.contains("tomato") || lower.contains("potato") || lower.contains("onion") {
        return "Fruits & Vegetables".into();
    }
    if lower.contains("chocolate") || lower.contains("biscuit") || lower.contains("cookie") || lower.contains("cake") || lower.contains("candy") || lower.contains("chip") || lower.contains("snack") || lower.contains("nut") || lower.contains("wafer") {
        return "Snacks & Confectionery".into();
    }
    if lower.contains("soap") || lower.contains("shampoo") || lower.contains("detergent") || lower.contains("toothpaste") || lower.contains("clean") || lower.contains("tissue") || lower.contains("diaper") {
        return "Personal Care & Cleaning".into();
    }
    if lower.contains("cigarette") || lower.contains("tobacco") || lower.contains("vape") || lower.contains("shisha") {
        return "Tobacco".into();
    }
    if lower.contains("frozen") || lower.contains("ice cream") || lower.contains("nugget") {
        return "Frozen Foods".into();
    }
    if lower.contains("oil") || lower.contains("lubricant") || lower.contains("battery") || lower.contains("bulb") || lower.contains("tool") {
        return "Hardware & Automotive".into();
    }
    "General".into()
}

fn strip_html_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    // Decode common HTML entities
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

// ── Mutation dry-run: build a human-readable preview ──────────────────────────

pub async fn dry_run_mutation(
    pool: &SqlitePool,
    tool_name: &str,
    input: &Value,
    currency_exp: u32,
) -> AppResult<ToolPreview> {
    let fmt = |n: i64| money::format_minor(n, currency_exp);

    match tool_name {
        "update_product_price" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let new_price = input
                .get("new_price_minor")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| AppError::Validation("Missing new_price_minor".into()))?;
            let reason = input.get("reason").and_then(|v| v.as_str()).unwrap_or("—");

            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;

            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Update selling price of '{}'", p.product.name),
                fields: vec![
                    ToolPreviewField {
                        label: "Product".into(),
                        value: p.product.name.clone(),
                    },
                    ToolPreviewField {
                        label: "Current Price".into(),
                        value: format!("BHD {}", fmt(p.price_minor)),
                    },
                    ToolPreviewField {
                        label: "New Price".into(),
                        value: format!("BHD {}", fmt(new_price)),
                    },
                    ToolPreviewField {
                        label: "Reason".into(),
                        value: reason.into(),
                    },
                ],
            })
        }
        "set_product_active" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let is_active = input
                .get("is_active")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| AppError::Validation("Missing is_active".into()))?;

            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;

            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!(
                    "{} product '{}'",
                    if is_active { "Enable" } else { "Disable" },
                    p.product.name
                ),
                fields: vec![
                    ToolPreviewField {
                        label: "Product".into(),
                        value: p.product.name.clone(),
                    },
                    ToolPreviewField {
                        label: "Action".into(),
                        value: if is_active {
                            "Enable (show in POS)".into()
                        } else {
                            "Disable (hide from POS)".into()
                        },
                    },
                ],
            })
        }
        "update_product_name" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let new_name = input
                .get("new_name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing new_name".into()))?;

            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;

            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: "Rename product".to_string(),
                fields: vec![
                    ToolPreviewField {
                        label: "Current Name".into(),
                        value: p.product.name.clone(),
                    },
                    ToolPreviewField {
                        label: "New Name".into(),
                        value: new_name.into(),
                    },
                ],
            })
        }
        "adjust_stock" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let delta = input
                .get("quantity_delta")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| AppError::Validation("Missing quantity_delta".into()))?;
            let notes = input.get("notes").and_then(|v| v.as_str()).unwrap_or("—");
            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let levels = stock_repo::get_all_levels(pool, &active_branch_id(pool).await?).await?;
            let current = levels
                .iter()
                .find(|s| s.product_id == product_id)
                .map(|s| s.quantity_on_hand.clone())
                .unwrap_or_else(|| "0".into());
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Adjust stock for '{}'", p.product.name),
                fields: vec![
                    ToolPreviewField {
                        label: "Product".into(),
                        value: p.product.name.clone(),
                    },
                    ToolPreviewField {
                        label: "Current Qty".into(),
                        value: current,
                    },
                    ToolPreviewField {
                        label: "Adjustment".into(),
                        value: format!("{:+}", delta),
                    },
                    ToolPreviewField {
                        label: "Reason".into(),
                        value: notes.into(),
                    },
                ],
            })
        }
        "stock_take" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let new_qty = input
                .get("new_quantity")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| AppError::Validation("Missing new_quantity".into()))?;
            let notes = input.get("notes").and_then(|v| v.as_str()).unwrap_or("—");
            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let levels = stock_repo::get_all_levels(pool, &active_branch_id(pool).await?).await?;
            let current = levels
                .iter()
                .find(|s| s.product_id == product_id)
                .map(|s| s.quantity_on_hand.clone())
                .unwrap_or_else(|| "0".into());
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Stock take for '{}'", p.product.name),
                fields: vec![
                    ToolPreviewField {
                        label: "Product".into(),
                        value: p.product.name.clone(),
                    },
                    ToolPreviewField {
                        label: "Current Qty".into(),
                        value: current,
                    },
                    ToolPreviewField {
                        label: "New Count".into(),
                        value: format!("{}", new_qty),
                    },
                    ToolPreviewField {
                        label: "Notes".into(),
                        value: notes.into(),
                    },
                ],
            })
        }
        "create_product" => {
            let name = input
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing name".into()))?;
            let price = input
                .get("price_minor")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| AppError::Validation("Missing price_minor".into()))?;
            let category_id = input
                .get("category_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing category_id".into()))?;
            let sku = input.get("sku").and_then(|v| v.as_str()).unwrap_or("—");
            let barcode = input.get("barcode").and_then(|v| v.as_str()).unwrap_or("—");

            let cat_name: Option<String> =
                sqlx::query_scalar("SELECT name FROM categories WHERE category_id = ?")
                    .bind(category_id)
                    .fetch_optional(pool)
                    .await?;

            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Create new product '{}'", name),
                fields: vec![
                    ToolPreviewField {
                        label: "Name".into(),
                        value: name.into(),
                    },
                    ToolPreviewField {
                        label: "Price".into(),
                        value: format!("BHD {}", fmt(price)),
                    },
                    ToolPreviewField {
                        label: "Category".into(),
                        value: cat_name.unwrap_or_else(|| category_id.into()),
                    },
                    ToolPreviewField {
                        label: "SKU".into(),
                        value: sku.into(),
                    },
                    ToolPreviewField {
                        label: "Barcode".into(),
                        value: barcode.into(),
                    },
                ],
            })
        }
        "update_reorder_point" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let new_point = input
                .get("reorder_point")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| AppError::Validation("Missing reorder_point".into()))?;
            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let levels = stock_repo::get_all_levels(pool, &active_branch_id(pool).await?).await?;
            let current_point = levels
                .iter()
                .find(|s| s.product_id == product_id)
                .map(|s| s.reorder_point.to_string())
                .unwrap_or_else(|| "0".to_string());
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Update reorder point for '{}'", p.product.name),
                fields: vec![
                    ToolPreviewField {
                        label: "Product".into(),
                        value: p.product.name.clone(),
                    },
                    ToolPreviewField {
                        label: "Current Reorder Point".into(),
                        value: current_point,
                    },
                    ToolPreviewField {
                        label: "New Reorder Point".into(),
                        value: format!("{}", new_point),
                    },
                ],
            })
        }
        // ── New customer / delivery / bulk-stock dry-runs ─────────────────────
        "create_customer" => {
            let name = input
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing name".into()))?;
            let phone = input.get("phone").and_then(|v| v.as_str()).unwrap_or("—");
            let email = input.get("email").and_then(|v| v.as_str()).unwrap_or("—");
            let notes = input.get("notes").and_then(|v| v.as_str()).unwrap_or("—");
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Create new customer '{}'", name),
                fields: vec![
                    ToolPreviewField { label: "Name".into(), value: name.into() },
                    ToolPreviewField { label: "Phone".into(), value: phone.into() },
                    ToolPreviewField { label: "Email".into(), value: email.into() },
                    ToolPreviewField { label: "Notes".into(), value: notes.into() },
                ],
            })
        }
        "update_customer" => {
            let customer_id = input
                .get("customer_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing customer_id".into()))?;
            let row = sqlx::query("SELECT name FROM customers WHERE customer_id = ?")
                .bind(customer_id)
                .fetch_optional(pool)
                .await?;
            let old_name: String = row
                .map(|r| r.get::<String, _>("name"))
                .unwrap_or_else(|| "unknown".to_string());
            let new_name = input.get("name").and_then(|v| v.as_str()).unwrap_or("—");
            let phone = input.get("phone").and_then(|v| v.as_str()).unwrap_or("—");
            let email = input.get("email").and_then(|v| v.as_str()).unwrap_or("—");
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Update customer '{}' → '{}'", old_name, new_name),
                fields: vec![
                    ToolPreviewField { label: "Old Name".into(), value: old_name },
                    ToolPreviewField { label: "New Name".into(), value: new_name.into() },
                    ToolPreviewField { label: "Phone".into(), value: phone.into() },
                    ToolPreviewField { label: "Email".into(), value: email.into() },
                ],
            })
        }
        "advance_delivery_status" => {
            let delivery_id = input
                .get("delivery_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing delivery_id".into()))?;
            let new_status = input
                .get("new_status")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing new_status".into()))?;
            let row = sqlx::query(
                "SELECT delivery_status, customer_name FROM delivery_orders WHERE delivery_id = ?",
            )
            .bind(delivery_id)
            .fetch_optional(pool)
            .await?;
            let (old_status, cust_name) = row
                .map(|r| {
                    (
                        r.get::<String, _>("delivery_status"),
                        r.get::<Option<String>, _>("customer_name")
                            .unwrap_or_else(|| "unknown".to_string()),
                    )
                })
                .unwrap_or_else(|| ("unknown".to_string(), "unknown".to_string()));
            let display_new = if new_status == "in_transit" {
                "out_for_delivery"
            } else {
                new_status
            };
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!(
                    "Advance delivery {} from '{}' → '{}'",
                    &delivery_id[..8.min(delivery_id.len())],
                    old_status,
                    display_new
                ),
                fields: vec![
                    ToolPreviewField { label: "Delivery ID".into(), value: delivery_id[..8.min(delivery_id.len())].to_string() },
                    ToolPreviewField { label: "Customer".into(), value: cust_name },
                    ToolPreviewField { label: "Current Status".into(), value: old_status },
                    ToolPreviewField { label: "New Status".into(), value: display_new.into() },
                ],
            })
        }
        "bulk_stock_take" => {
            let items = input
                .get("items")
                .and_then(|v| v.as_array())
                .ok_or_else(|| AppError::Validation("Missing items array".into()))?;
            let lines: Vec<String> = items
                .iter()
                .map(|item| {
                    let pid = item
                        .get("product_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?");
                    let qty = item
                        .get("new_quantity")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    format!("{}: {} units", &pid[..8.min(pid.len())], qty)
                })
                .collect();
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Bulk stock take: {} products", items.len()),
                fields: vec![ToolPreviewField {
                    label: "Changes".into(),
                    value: lines.join("; "),
                }],
            })
        }
        // ── Category dry-runs ─────────────────────────────────────────────────
        "create_category" => {
            let name = input.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let order = input.get("sort_order").and_then(|v| v.as_i64()).unwrap_or(0);
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Create category '{}'", name),
                fields: vec![
                    ToolPreviewField { label: "Name".into(), value: name.into() },
                    ToolPreviewField { label: "Sort order".into(), value: order.to_string() },
                ],
            })
        }
        "update_category" => {
            let category_id = input.get("category_id").and_then(|v| v.as_str()).unwrap_or("?");
            let row = sqlx::query("SELECT name FROM categories WHERE category_id = ?")
                .bind(category_id).fetch_optional(pool).await?;
            let old_name = row.map(|r| r.get::<String,_>("name")).unwrap_or_else(|| "?".into());
            let name = input.get("name").and_then(|v| v.as_str());
            let active = input.get("is_active").and_then(|v| v.as_bool());
            let mut fields = vec![ToolPreviewField { label: "Category".into(), value: old_name.clone() }];
            if let Some(n) = name { fields.push(ToolPreviewField { label: "New name".into(), value: n.into() }); }
            if let Some(a) = active { fields.push(ToolPreviewField { label: "Active".into(), value: (if a { "Yes" } else { "No" }).into() }); }
            Ok(ToolPreview { tool_name: tool_name.into(), description: format!("Update category '{}'", old_name), fields })
        }
        // ── User dry-runs ─────────────────────────────────────────────────────
        "create_user" => {
            let display = input.get("display_name").and_then(|v| v.as_str()).unwrap_or("?");
            let username = input.get("username").and_then(|v| v.as_str()).unwrap_or("?");
            let role_id = input.get("role_id").and_then(|v| v.as_str()).unwrap_or("?");
            let role_name: Option<String> = sqlx::query_scalar("SELECT name FROM roles WHERE role_id = ?")
                .bind(role_id).fetch_optional(pool).await?.flatten();
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Create staff account '{}'", display),
                fields: vec![
                    ToolPreviewField { label: "Name".into(), value: display.into() },
                    ToolPreviewField { label: "Username".into(), value: username.into() },
                    ToolPreviewField { label: "Role".into(), value: role_name.unwrap_or_else(|| role_id.into()) },
                ],
            })
        }
        "update_user" => {
            let user_id = input.get("user_id").and_then(|v| v.as_str()).unwrap_or("?");
            let row = sqlx::query("SELECT display_name, is_active FROM users WHERE user_id = ?")
                .bind(user_id).fetch_optional(pool).await?
                .ok_or_else(|| AppError::NotFound("User not found".into()))?;
            let old_name: String = row.get("display_name");
            let _old_active: bool = row.get("is_active");
            let name = input.get("display_name").and_then(|v| v.as_str());
            let active = input.get("is_active").and_then(|v| v.as_bool());
            let mut fields = vec![ToolPreviewField { label: "User".into(), value: old_name.clone() }];
            if let Some(n) = name { fields.push(ToolPreviewField { label: "New name".into(), value: n.into() }); }
            if let Some(a) = active { fields.push(ToolPreviewField { label: "Active".into(), value: (if a { "Yes" } else { "No" }).into() }); }
            Ok(ToolPreview { tool_name: tool_name.into(), description: format!("Update user '{}'", old_name), fields })
        }
        // ── Tax rule dry-runs ─────────────────────────────────────────────────
        "create_tax_rule" => {
            let name = input.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let bp = input.get("rate_basis_points").and_then(|v| v.as_i64()).unwrap_or(0);
            let inclusive = input.get("inclusive").and_then(|v| v.as_bool()).unwrap_or(true);
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Create tax rule '{}'", name),
                fields: vec![
                    ToolPreviewField { label: "Name".into(), value: name.into() },
                    ToolPreviewField { label: "Rate".into(), value: format!("{} bp", bp) },
                    ToolPreviewField { label: "Type".into(), value: (if inclusive { "Inclusive" } else { "Exclusive" }).into() },
                ],
            })
        }
        "update_tax_rule" => {
            let tax_rule_id = input.get("tax_rule_id").and_then(|v| v.as_str()).unwrap_or("?");
            let row = sqlx::query("SELECT name, rate_basis_points FROM tax_rules WHERE tax_rule_id = ?")
                .bind(tax_rule_id).fetch_optional(pool).await?
                .ok_or_else(|| AppError::NotFound("Tax rule not found".into()))?;
            let old_name: String = row.get("name");
            let old_bp: i64 = row.get("rate_basis_points");
            let name = input.get("name").and_then(|v| v.as_str());
            let bp_val = input.get("rate_basis_points").and_then(|v| v.as_i64());
            let inclusive = input.get("inclusive").and_then(|v| v.as_bool());
            let active = input.get("is_active").and_then(|v| v.as_bool());
            let mut fields = vec![
                ToolPreviewField { label: "Tax Rule".into(), value: format!("{} ({} bp)", old_name, old_bp) },
            ];
            if let Some(n) = name { fields.push(ToolPreviewField { label: "New name".into(), value: n.into() }); }
            if let Some(b) = bp_val { fields.push(ToolPreviewField { label: "New rate".into(), value: format!("{} bp", b) }); }
            if let Some(i) = inclusive { fields.push(ToolPreviewField { label: "Inclusive".into(), value: (if i {"Yes"} else {"No"}).into() }); }
            if let Some(a) = active { fields.push(ToolPreviewField { label: "Active".into(), value: (if a {"Yes"} else {"No"}).into() }); }
            Ok(ToolPreview { tool_name: tool_name.into(), description: format!("Update tax rule '{}'", old_name), fields })
        }
        // ── Holistic product update dry-run ────────────────────────────────────
        "update_product_full" => {
            let product_id = input.get("product_id").and_then(|v| v.as_str()).unwrap_or("?");
            let p = product_repo::get_product_by_id(pool, product_id).await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let mut changes: Vec<String> = Vec::new();
            if let Some(n) = input.get("name").and_then(|v| v.as_str()) { if n != p.product.name { changes.push(format!("Name: {} → {}", p.product.name, n)); } }
            if let Some(v) = input.get("price_minor").and_then(|v| v.as_i64()) { if v != p.price_minor { changes.push(format!("Price: BHD {} → BHD {}", fmt(p.price_minor), fmt(v))); } }
            if let Some(v) = input.get("is_active").and_then(|v| v.as_bool()) { if v != p.product.is_active { changes.push(format!("Active: {} → {}", p.product.is_active, v)); } }
            if let Some(v) = input.get("track_inventory").and_then(|v| v.as_bool()) { if v != p.product.track_inventory { changes.push(format!("Track inventory: {} → {}", p.product.track_inventory, v)); } }
            if changes.is_empty() { changes.push("No changes detected".into()); }
            Ok(ToolPreview { tool_name: tool_name.into(), description: format!("Update product '{}'", p.product.name), fields: vec![ToolPreviewField { label: "Changes".into(), value: changes.join("; ") }] })
        }
        // ── Store settings dry-run ─────────────────────────────────────────────
        "update_store_settings" => {
            let r = sqlx::query("SELECT name FROM branches WHERE is_active=1 LIMIT 1")
                .fetch_optional(pool).await?
                .map(|r| r.get::<String,_>("name")).unwrap_or_else(|| "?".into());
            let mut changes = vec![];
            for key in &["name", "address", "phone", "tax_number", "cr_number", "receipt_header", "receipt_footer", "timezone"] {
                if let Some(v) = input.get(*key).and_then(|v| v.as_str()) { changes.push(format!("{} = {}", key, v)); }
            }
            Ok(ToolPreview { tool_name: tool_name.into(), description: format!("Update store '{}' settings", r), fields: vec![ToolPreviewField { label: "Changes".into(), value: if changes.is_empty() { "(none)".into() } else { changes.join(", ") } }] })
        }
        // ── Business rules dry-run ─────────────────────────────────────────────
        "update_business_rules" => {
            let mut changes = vec![];
            for key in &["allow_negative_stock", "require_discount_reason", "cashier_can_discount", "auto_print_receipt"] {
                if let Some(v) = input.get(*key).and_then(|v| v.as_bool()) { changes.push(format!("{} = {}", key, v)); }
            }
            Ok(ToolPreview { tool_name: tool_name.into(), description: "Update business rules".into(), fields: vec![ToolPreviewField { label: "Setting".into(), value: if changes.is_empty() { "(none)".into() } else { changes.join(", ") } }] })
        }
        // ── Delivery dry-runs ──────────────────────────────────────────────────
        "confirm_delivery_payment" => {
            let delivery_id = input.get("delivery_id").and_then(|v| v.as_str()).unwrap_or("?");
            let row = sqlx::query("SELECT delivery_status, payment_status, amount_minor FROM delivery_orders WHERE delivery_id = ?")
                .bind(delivery_id).fetch_optional(pool).await?
                .ok_or_else(|| AppError::NotFound("Delivery not found".into()))?;
            let amt: i64 = row.get("amount_minor");
            let ps: String = row.get("payment_status");
            Ok(ToolPreview { tool_name: tool_name.into(), description: format!("Confirm payment for delivery {}", &delivery_id[..8.min(delivery_id.len())]), fields: vec![
                ToolPreviewField { label: "Amount".into(), value: format!("BHD {}", fmt(amt)) },
                ToolPreviewField { label: "Status".into(), value: format!("{} → paid", ps) },
            ]})
        }
        "cancel_delivery" => {
            let delivery_id = input.get("delivery_id").and_then(|v| v.as_str()).unwrap_or("?");
            let row = sqlx::query("SELECT delivery_status FROM delivery_orders WHERE delivery_id = ?")
                .bind(delivery_id).fetch_optional(pool).await?
                .ok_or_else(|| AppError::NotFound("Delivery not found".into()))?;
            let status: String = row.get("delivery_status");
            Ok(ToolPreview { tool_name: tool_name.into(), description: format!("Cancel delivery {}", &delivery_id[..8.min(delivery_id.len())]), fields: vec![
                ToolPreviewField { label: "Current status".into(), value: status },
                ToolPreviewField { label: "New status".into(), value: "cancelled".into() },
            ]})
        }
        // ── Sync repair dry-runs ──────────────────────────────────────────────
        "sync_reset_stuck" => {
            let mut stuck = 0i64;
            for table in crate::commands::sync_commands::SYNC_TABLES {
                let n: i64 = sqlx::query_scalar(
                    &format!("SELECT COUNT(*) FROM {table} WHERE sync_status='pending' AND sync_attempts>=10"),
                ).fetch_one(pool).await.unwrap_or(0);
                stuck += n;
            }
            Ok(ToolPreview { tool_name: tool_name.into(),
                description: format!("Reset {stuck} stuck rows across all tables back to pending with 0 attempts"),
                fields: vec![ToolPreviewField { label: "Stuck rows".into(), value: stuck.to_string() }] })
        }
        "sync_queue_retry" => {
            let event_id = input.get("event_id").and_then(|v| v.as_str()).unwrap_or("");
            Ok(ToolPreview { tool_name: tool_name.into(),
                description: format!("Retry sync event: {event_id}"),
                fields: vec![ToolPreviewField { label: "Event".into(), value: event_id.to_string() }] })
        }
        "sync_queue_dismiss" => {
            let event_id = input.get("event_id").and_then(|v| v.as_str()).unwrap_or("");
            Ok(ToolPreview { tool_name: tool_name.into(),
                description: format!("Dismiss sync event: {event_id}"),
                fields: vec![ToolPreviewField { label: "Event".into(), value: event_id.to_string() }] })
        }
        "void_sale" => {
            let receipt = input.get("receipt_number").and_then(|v| v.as_str()).unwrap_or("");
            let reason = input.get("reason").and_then(|v| v.as_str()).unwrap_or("");
            Ok(ToolPreview { tool_name: tool_name.into(),
                description: format!("Void sale {receipt}: {reason}"),
                fields: vec![
                    ToolPreviewField { label: "Receipt".into(), value: receipt.to_string() },
                    ToolPreviewField { label: "Reason".into(), value: reason.to_string() },
                ] })
        }
        "delete_customer" => {
            let cid = input.get("customer_id").and_then(|v| v.as_str()).unwrap_or("");
            let name: Option<String> = sqlx::query_scalar("SELECT name FROM customers WHERE customer_id=?").bind(cid).fetch_optional(pool).await?.flatten();
            Ok(ToolPreview { tool_name: tool_name.into(),
                description: format!("Delete customer: {}", name.as_deref().unwrap_or(cid)),
                fields: vec![ToolPreviewField { label: "Customer".into(), value: name.unwrap_or_else(|| cid.to_string()) }] })
        }
        "set_device_active" => {
            let did = input.get("device_id").and_then(|v| v.as_str()).unwrap_or("");
            let active = input.get("is_active").and_then(|v| v.as_bool()).unwrap_or(true);
            Ok(ToolPreview { tool_name: tool_name.into(),
                description: format!("Set device {did} active={active}"),
                fields: vec![ToolPreviewField { label: "Device".into(), value: did.to_string() }] })
        }
        "receive_stock" => {
            let pid = input.get("product_id").and_then(|v| v.as_str()).unwrap_or("");
            let qty = input.get("quantity").and_then(|v| v.as_str()).unwrap_or("0");
            let pname: Option<String> = sqlx::query_scalar("SELECT name FROM products WHERE product_id=?").bind(pid).fetch_optional(pool).await?.flatten();
            Ok(ToolPreview { tool_name: tool_name.into(),
                description: format!("Receive {qty} of {}", pname.as_deref().unwrap_or(pid)),
                fields: vec![ToolPreviewField { label: "Product".into(), value: pname.unwrap_or_else(|| pid.to_string()) }] })
        }
        "add_loyalty_points" => {
            let cid = input.get("customer_id").and_then(|v| v.as_str()).unwrap_or("");
            let pts = input.get("points").and_then(|v| v.as_i64()).unwrap_or(0);
            Ok(ToolPreview { tool_name: tool_name.into(),
                description: format!("Add {pts} loyalty points to customer"),
                fields: vec![ToolPreviewField { label: "Points".into(), value: pts.to_string() }] })
        }
        "bulk_update_prices" => {
            let count = input.get("updates").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
            Ok(ToolPreview { tool_name: tool_name.into(),
                description: format!("Bulk update prices for {count} products"),
                fields: vec![ToolPreviewField { label: "Products".into(), value: count.to_string() }] })
        }
        // ── Backup dry-run ─────────────────────────────────────────────────────
        "backup_database" => {
            Ok(ToolPreview { tool_name: tool_name.into(), description: "Create full database backup".into(), fields: vec![ToolPreviewField { label: "Action".into(), value: "Backup to app data directory".into() }] })
        }
        name => crate::ai::tools_write_ext::dry_run(pool, name, input, currency_exp).await,
    }
}

// ── Mutation executor ─────────────────────────────────────────────────────────

pub struct MutationResult {
    pub description: String,
    pub undo_snapshot_json: String,
    pub rollback_tool: String,
    pub rollback_input_json: String,
    pub entity_type: String,
    pub entity_id: String,
}

/// Look up the active branch_id from the database (read-only queries only need branch).
async fn active_branch_id(pool: &SqlitePool) -> crate::errors::AppResult<String> {
    sqlx::query_scalar(
        "SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| crate::errors::AppError::NotFound("No active branch configured — complete store setup first".into()))
}

/// Look up the active device_id and branch_id from the database.
/// Returns an error if either is missing (setup not complete).
async fn active_device_branch(pool: &SqlitePool) -> crate::errors::AppResult<(String, String)> {
    let device_id: Option<String> = sqlx::query_scalar(
        "SELECT device_id FROM devices WHERE is_active = 1 ORDER BY device_code LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let branch_id: Option<String> = sqlx::query_scalar(
        "SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    match (device_id, branch_id) {
        (Some(d), Some(b)) => Ok((d, b)),
        _ => Err(crate::errors::AppError::NotFound(
            "No active device or branch — complete store setup first".into(),
        )),
    }
}

/// Guard rail: reject obviously invalid AI-generated mutation inputs before
/// they touch the database. This is a safety net — the LLM should never
/// generate these values, but if it does, we catch it here.
fn validate_mutation_input(tool_name: &str, input: &Value) -> AppResult<()> {
    match tool_name {
        "update_product_price" | "create_product" => {
            let price = input.get("new_price_minor").or_else(|| input.get("price_minor"));
            if let Some(p) = price.and_then(|v| v.as_i64()) {
                if p <= 0 {
                    return Err(AppError::Validation("Price must be positive (minor units > 0)".into()));
                }
                if p > 100_000_000 {
                    return Err(AppError::Validation("Price exceeds maximum (100M minor units)".into()));
                }
            }
            if tool_name == "create_product" {
                let name = input.get("name").and_then(|v| v.as_str()).unwrap_or("");
                if name.trim().is_empty() {
                    return Err(AppError::Validation("Product name cannot be empty".into()));
                }
                if name.len() > 200 {
                    return Err(AppError::Validation("Product name too long (max 200 chars)".into()));
                }
            }
        }
        "update_product_name" => {
            let name = input.get("new_name").and_then(|v| v.as_str()).unwrap_or("");
            if name.trim().is_empty() {
                return Err(AppError::Validation("Product name cannot be empty".into()));
            }
            if name.len() > 200 {
                return Err(AppError::Validation("Product name too long (max 200 chars)".into()));
            }
        }
        "set_product_active" => {
            let _is_active = input.get("is_active").and_then(|v| v.as_bool())
                .ok_or_else(|| AppError::Validation("is_active must be a boolean".into()))?;
        }
        "adjust_stock" => {
            let delta = input.get("quantity_delta").and_then(|v| v.as_f64()).unwrap_or(0.0);
            if delta == 0.0 {
                return Err(AppError::Validation("quantity_delta cannot be zero".into()));
            }
            if delta.abs() > 10_000_000.0 {
                return Err(AppError::Validation("quantity_delta exceeds maximum (±10M)".into()));
            }
        }
        "stock_take" | "bulk_stock_take" => {
            let qty = input.get("new_quantity").and_then(|v| v.as_f64()).unwrap_or(-1.0);
            if qty < 0.0 {
                return Err(AppError::Validation("new_quantity cannot be negative".into()));
            }
            if qty > 10_000_000.0 {
                return Err(AppError::Validation("new_quantity exceeds maximum (10M)".into()));
            }
        }
        "update_reorder_point" => {
            let rp = input.get("reorder_point").and_then(|v| v.as_f64()).unwrap_or(-1.0);
            if rp < 0.0 {
                return Err(AppError::Validation("reorder_point cannot be negative".into()));
            }
            if rp > 1_000_000.0 {
                return Err(AppError::Validation("reorder_point exceeds maximum (1M)".into()));
            }
        }
        "create_customer" | "update_customer" => {
            let name = input.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name.trim().is_empty() {
                return Err(AppError::Validation("Customer name cannot be empty".into()));
            }
            if name.len() > 200 {
                return Err(AppError::Validation("Customer name too long (max 200 chars)".into()));
            }
        }
        "advance_delivery_status" => {
            let status = input.get("new_status").and_then(|v| v.as_str()).unwrap_or("");
            if !matches!(status, "in_transit" | "delivered" | "cancelled") {
                return Err(AppError::Validation(format!("Invalid delivery status: '{}'. Must be in_transit | delivered | cancelled", status)));
            }
        }
        other => {
            // MEDIUM #12: A mutation tool in MUTATION_TOOLS but with no validation rule
            // here means AI input goes unvalidated. Log a warning so this never silently
            // slips through — the integrity test (mutation_tools_list_is_complete) catches
            // missing registrations but not missing validation rules.
            if MUTATION_TOOLS.contains(&other) {
                tracing::warn!(
                    "validate_mutation_input: tool '{}' is in MUTATION_TOOLS but has no \
                     validation rule — add one to prevent unvalidated AI mutations",
                    other
                );
            }
        }
    }
    Ok(())
}

pub async fn execute_mutation(
    pool: &SqlitePool,
    tool_name: &str,
    input: &Value,
    currency_exp: u32,
) -> AppResult<MutationResult> {
    // ── Input validation gate (prevents nonsensical AI-generated values) ────
    validate_mutation_input(tool_name, input)?;

    let fmt = |n: i64| money::format_minor(n, currency_exp);

    match tool_name {
        "update_product_price" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let new_price = input
                .get("new_price_minor")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| AppError::Validation("Missing new_price_minor".into()))?;

            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let old_price = p.price_minor;

            // Expire current active price and insert new one
            let now = chrono::Utc::now().to_rfc3339();
            let new_price_id = ulid::Ulid::new().to_string();

            sqlx::query(
                "UPDATE product_prices SET effective_to = ?, sync_status = 'pending'
                 WHERE product_id = ? AND branch_id IS NULL AND price_type = 'selling'
                   AND effective_to IS NULL",
            )
            .bind(&now)
            .bind(product_id)
            .execute(pool)
            .await?;

            sqlx::query(
                "INSERT INTO product_prices (price_id, product_id, branch_id, price_type, price_minor,
                 currency, effective_from, effective_to, created_by_user_id, created_at)
                 VALUES (?, ?, NULL, 'selling', ?, 'BHD', ?, NULL, 'AI_ADMIN', ?)"
            )
            .bind(&new_price_id)
            .bind(product_id)
            .bind(new_price)
            .bind(&now)
            .bind(&now)
            .execute(pool)
            .await?;

            // Write audit log
            write_audit(
                pool,
                "AI_ADMIN",
                "product_price_update",
                product_id,
                &json!({"from": old_price, "to": new_price}),
            )
            .await?;

            // sync_status='pending' is set by column DEFAULT — sync worker picks it up (effective_to changed on old price, but product itself didn't change — just the price)
            // The product entity sync is handled by the price entry above.

            Ok(MutationResult {
                description: format!(
                    "Price of '{}' changed from BHD {} to BHD {}",
                    p.product.name,
                    fmt(old_price),
                    fmt(new_price)
                ),
                undo_snapshot_json: json!({ "price_minor": old_price }).to_string(),
                rollback_tool: "update_product_price".into(),
                rollback_input_json: json!({
                    "product_id": product_id, "new_price_minor": old_price
                })
                .to_string(),
                entity_type: "product_price".into(),
                entity_id: product_id.into(),
            })
        }
        "set_product_active" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let is_active = input
                .get("is_active")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| AppError::Validation("Missing is_active".into()))?;

            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let old_active = p.product.is_active;

            sqlx::query(
                "UPDATE products SET is_active = ?, updated_at = ?, version = version + 1, sync_status = 'pending' WHERE product_id = ?",
            )
            .bind(is_active as i64)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(product_id)
            .execute(pool)
            .await?;

            write_audit(
                pool,
                "AI_ADMIN",
                "product_status_change",
                product_id,
                &json!({"from": old_active, "to": is_active}),
            )
            .await?;

            // sync_status='pending' is set by column DEFAULT — sync worker picks it up

            Ok(MutationResult {
                description: format!(
                    "Product '{}' {}",
                    p.product.name,
                    if is_active { "enabled" } else { "disabled" }
                ),
                undo_snapshot_json: json!({ "is_active": old_active }).to_string(),
                rollback_tool: "set_product_active".into(),
                rollback_input_json: json!({
                    "product_id": product_id, "is_active": old_active
                })
                .to_string(),
                entity_type: "product".into(),
                entity_id: product_id.into(),
            })
        }
        "update_product_name" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let new_name = input
                .get("new_name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing new_name".into()))?;

            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let old_name = p.product.name.clone();

            sqlx::query("UPDATE products SET name = ?, updated_at = ?, version = version + 1, sync_status = 'pending' WHERE product_id = ?")
                .bind(new_name)
                .bind(chrono::Utc::now().to_rfc3339())
                .bind(product_id)
                .execute(pool)
                .await?;

            write_audit(
                pool,
                "AI_ADMIN",
                "product_rename",
                product_id,
                &json!({"from": &old_name, "to": new_name}),
            )
            .await?;

            // sync_status='pending' is set by column DEFAULT — sync worker picks it up

            Ok(MutationResult {
                description: format!("Product renamed from '{}' to '{}'", old_name, new_name),
                undo_snapshot_json: json!({ "name": &old_name }).to_string(),
                rollback_tool: "update_product_name".into(),
                rollback_input_json: json!({
                    "product_id": product_id, "new_name": &old_name
                })
                .to_string(),
                entity_type: "product".into(),
                entity_id: product_id.into(),
            })
        }
        "adjust_stock" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let delta = input
                .get("quantity_delta")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| AppError::Validation("Missing quantity_delta".into()))?;
            let notes = input.get("notes").and_then(|v| v.as_str());
            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;

            let (dv_id, br_id) = active_device_branch(pool).await?;
            let result = movements::manual_adjust(
                pool, product_id, delta, notes, "AI_ADMIN", None, &br_id, &dv_id,
            )
            .await?;
            let new_qty = result.quantity_on_hand.clone();

            write_audit(
                pool,
                "AI_ADMIN",
                "stock.adjustment",
                product_id,
                &json!({ "delta": delta, "new_qty": &new_qty, "notes": notes }),
            )
            .await?;

            Ok(MutationResult {
                description: format!(
                    "Stock of '{}' adjusted by {:+} → now {}",
                    p.product.name, delta, new_qty
                ),
                undo_snapshot_json: json!({ "quantity_delta": -delta }).to_string(),
                rollback_tool: "adjust_stock".into(),
                rollback_input_json: json!({
                    "product_id": product_id,
                    "quantity_delta": -delta,
                    "notes": "Undo previous adjustment",
                })
                .to_string(),
                entity_type: "stock_level".into(),
                entity_id: product_id.into(),
            })
        }
        "stock_take" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let new_quantity = input
                .get("new_quantity")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| AppError::Validation("Missing new_quantity".into()))?;
            let notes = input.get("notes").and_then(|v| v.as_str());
            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;

            // Get old qty for undo
            let levels = stock_repo::get_all_levels(pool, &active_branch_id(pool).await?).await?;
            let old_qty: f64 = levels
                .iter()
                .find(|s| s.product_id == product_id)
                .and_then(|s| s.quantity_on_hand.parse().ok())
                .unwrap_or(0.0);

            let (dv_id, br_id) = active_device_branch(pool).await?;
            movements::stock_take(
                pool,
                product_id,
                new_quantity,
                notes,
                "AI_ADMIN",
                None,
                &br_id,
                &dv_id,
            )
            .await?;

            write_audit(
                pool,
                "AI_ADMIN",
                "stock.stock_take",
                product_id,
                &json!({ "old_qty": old_qty, "new_qty": new_quantity, "notes": notes }),
            )
            .await?;

            Ok(MutationResult {
                description: format!(
                    "Stock take for '{}': counted {} (was {})",
                    p.product.name, new_quantity, old_qty
                ),
                undo_snapshot_json: json!({ "new_quantity": old_qty }).to_string(),
                rollback_tool: "stock_take".into(),
                rollback_input_json: json!({
                    "product_id": product_id,
                    "new_quantity": old_qty,
                    "notes": "Undo stock take",
                })
                .to_string(),
                entity_type: "stock_level".into(),
                entity_id: product_id.into(),
            })
        }
        "create_product" => {
            let name = input
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing name".into()))?;
            let price_minor = input
                .get("price_minor")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| AppError::Validation("Missing price_minor".into()))?;
            let category_id = input
                .get("category_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing category_id".into()))?;
            let sku = input.get("sku").and_then(|v| v.as_str());
            let barcode = input.get("barcode").and_then(|v| v.as_str());

            let now = chrono::Utc::now().to_rfc3339();
            let product_id = ulid::Ulid::new().to_string();
            let price_id = ulid::Ulid::new().to_string();

            sqlx::query(
                "INSERT INTO products
                   (product_id, category_id, name, sku, barcode,
                    track_inventory, allow_decimal_quantity, is_active,
                    currency, version, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, 0, 0, 1, 'BHD', 1, ?, ?)",
            )
            .bind(&product_id)
            .bind(category_id)
            .bind(name)
            .bind(sku)
            .bind(barcode)
            .bind(&now)
            .bind(&now)
            .execute(pool)
            .await?;

            sqlx::query(
                "INSERT INTO product_prices
                   (price_id, product_id, branch_id, price_type, price_minor,
                    currency, effective_from, effective_to, created_by_user_id, created_at)
                 VALUES (?, ?, NULL, 'selling', ?, 'BHD', ?, NULL, 'AI_ADMIN', ?)",
            )
            .bind(&price_id)
            .bind(&product_id)
            .bind(price_minor)
            .bind(&now)
            .bind(&now)
            .execute(pool)
            .await?;

            write_audit(
                pool,
                "AI_ADMIN",
                "product.created",
                &product_id,
                &json!({ "name": name, "price_minor": price_minor, "category_id": category_id }),
            )
            .await?;

            Ok(MutationResult {
                description: format!("Created product '{}' at BHD {}", name, fmt(price_minor)),
                undo_snapshot_json: json!({ "product_id": &product_id }).to_string(),
                rollback_tool: "set_product_active".into(),
                rollback_input_json: json!({
                    "product_id": &product_id, "is_active": false
                })
                .to_string(),
                entity_type: "product".into(),
                entity_id: product_id,
            })
        }
        "update_reorder_point" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let new_point = input
                .get("reorder_point")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| AppError::Validation("Missing reorder_point".into()))?;
            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let levels = stock_repo::get_all_levels(pool, &active_branch_id(pool).await?).await?;
            let old_point: f64 = levels
                .iter()
                .find(|s| s.product_id == product_id)
                .map(|s| s.reorder_point as f64)
                .unwrap_or(0.0);

            sqlx::query(
                "UPDATE products SET reorder_point = ?, updated_at = ?, sync_status = 'pending' WHERE product_id = ?",
            )
            .bind(new_point)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(product_id)
            .execute(pool)
            .await?;

            write_audit(
                pool,
                "AI_ADMIN",
                "stock.reorder_point_update",
                product_id,
                &json!({ "from": old_point, "to": new_point }),
            )
            .await?;

            Ok(MutationResult {
                description: format!(
                    "Reorder point for '{}' changed from {} to {}",
                    p.product.name, old_point, new_point
                ),
                undo_snapshot_json: json!({ "reorder_point": old_point }).to_string(),
                rollback_tool: "update_reorder_point".into(),
                rollback_input_json: json!({
                    "product_id": product_id, "reorder_point": old_point
                })
                .to_string(),
                entity_type: "stock_level".into(),
                entity_id: product_id.into(),
            })
        }
        // ── Customer mutations ────────────────────────────────────────────────
        "create_customer" => {
            let name = input
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing name".into()))?;
            let phone = input.get("phone").and_then(|v| v.as_str());
            let email = input.get("email").and_then(|v| v.as_str());
            let notes = input.get("notes").and_then(|v| v.as_str());
            let branch_id = active_branch_id(pool).await?;
            let customer_id = ulid::Ulid::new().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query(
                "INSERT INTO customers (customer_id, branch_id, name, phone, email, notes, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&customer_id)
            .bind(&branch_id)
            .bind(name)
            .bind(phone)
            .bind(email)
            .bind(notes)
            .bind(&now)
            .bind(&now)
            .execute(pool)
            .await?;
            write_audit(
                pool,
                "AI_ADMIN",
                "customer.create",
                &customer_id,
                &json!({ "name": name, "phone": phone, "email": email }),
            )
            .await?;
            Ok(MutationResult {
                description: format!("Customer '{}' created (ID: {})", name, &customer_id[..8]),
                undo_snapshot_json: json!({ "customer_id": customer_id }).to_string(),
                rollback_tool: "delete_customer".into(),
                rollback_input_json: json!({ "customer_id": customer_id }).to_string(),
                entity_type: "customer".into(),
                entity_id: customer_id,
            })
        }
        "update_customer" => {
            let customer_id = input
                .get("customer_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing customer_id".into()))?;
            let row = sqlx::query(
                "SELECT name, phone, email, notes FROM customers WHERE customer_id = ?",
            )
            .bind(customer_id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Customer not found".into()))?;
            let old_name: String = row.get("name");
            let old_phone: Option<String> = row.get("phone");
            let old_email: Option<String> = row.get("email");
            let old_notes: Option<String> = row.get("notes");

            let new_name = input.get("name").and_then(|v| v.as_str()).unwrap_or(&old_name);
            let new_phone = input.get("phone").and_then(|v| v.as_str());
            let new_email = input.get("email").and_then(|v| v.as_str());
            let new_notes = input.get("notes").and_then(|v| v.as_str());
            sqlx::query(
                "UPDATE customers SET name = ?, phone = COALESCE(?, phone),
                  email = COALESCE(?, email), notes = COALESCE(?, notes),
                  updated_at = ?, sync_status = 'pending'
                  WHERE customer_id = ?",
            )
            .bind(new_name)
            .bind(new_phone)
            .bind(new_email)
            .bind(new_notes)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(customer_id)
            .execute(pool)
            .await?;
            write_audit(
                pool,
                "AI_ADMIN",
                "customer.update",
                customer_id,
                &json!({ "name": new_name }),
            )
            .await?;
            Ok(MutationResult {
                description: format!("Customer '{}' updated to '{}'", old_name, new_name),
                undo_snapshot_json: json!({
                    "name": old_name, "phone": old_phone, "email": old_email, "notes": old_notes
                })
                .to_string(),
                rollback_tool: "update_customer".into(),
                rollback_input_json: json!({
                    "customer_id": customer_id,
                    "name": old_name,
                    "phone": old_phone,
                    "email": old_email,
                    "notes": old_notes
                })
                .to_string(),
                entity_type: "customer".into(),
                entity_id: customer_id.into(),
            })
        }
        // ── Delivery mutations ────────────────────────────────────────────────
        "advance_delivery_status" => {
            let delivery_id = input
                .get("delivery_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing delivery_id".into()))?;
            let new_status_input = input
                .get("new_status")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing new_status".into()))?;
            // Map AI-facing "in_transit" → DB column value "dispatched"
            let db_new_status = if new_status_input == "in_transit" {
                "dispatched"
            } else {
                new_status_input
            };
            let row = sqlx::query(
                "SELECT delivery_status FROM delivery_orders WHERE delivery_id = ?",
            )
            .bind(delivery_id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Delivery not found".into()))?;
            let old_status: String = row.get("delivery_status");
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query(
                "UPDATE delivery_orders SET delivery_status = ?, updated_at = ?, sync_status = 'pending', version = version + 1 WHERE delivery_id = ?",
            )
            .bind(db_new_status)
            .bind(&now)
            .bind(delivery_id)
            .execute(pool)
            .await?;
            write_audit(
                pool,
                "AI_ADMIN",
                "delivery.status_advance",
                delivery_id,
                &json!({ "from": old_status, "to": db_new_status }),
            )
            .await?;
            // Build rollback: use original DB status (old_status already is DB value)
            let rollback_input = if old_status == "out_for_delivery" {
                json!({ "delivery_id": delivery_id, "new_status": "in_transit" })
            } else {
                json!({ "delivery_id": delivery_id, "new_status": old_status })
            };
            Ok(MutationResult {
                description: format!(
                    "Delivery {} status: '{}' → '{}'",
                    &delivery_id[..8.min(delivery_id.len())],
                    old_status,
                    db_new_status
                ),
                undo_snapshot_json: json!({ "delivery_status": old_status }).to_string(),
                rollback_tool: "advance_delivery_status".into(),
                rollback_input_json: rollback_input.to_string(),
                entity_type: "delivery_order".into(),
                entity_id: delivery_id.into(),
            })
        }
        // ── Bulk stock take ───────────────────────────────────────────────────
        "bulk_stock_take" => {
            let items = input
                .get("items")
                .and_then(|v| v.as_array())
                .ok_or_else(|| AppError::Validation("Missing items array".into()))?
                .clone();
            let branch_id = active_branch_id(pool).await?;
            let now = chrono::Utc::now().to_rfc3339();
            let mut undo_items: Vec<Value> = Vec::new();
            let mut results: Vec<String> = Vec::new();

            for item in &items {
                let product_id = item
                    .get("product_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let new_qty = item
                    .get("new_quantity")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);

                // Fetch old quantity for audit snapshot (outside transaction)
                let old_qty_read: Option<String> = sqlx::query_scalar(
                    "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?",
                )
                .bind(product_id)
                .bind(&branch_id)
                .fetch_optional(pool)
                .await?
                .flatten();
                let old_qty_str = old_qty_read.as_deref().unwrap_or("0");
                let old_qty: f64 = old_qty_str.parse().unwrap_or(0.0);

                // Transaction: read current qty → compute delta → upsert → movement
                let (device_id, _) = active_device_branch(pool).await?;
                let mut tx = pool.begin().await?;

                let old_in_tx: Option<String> = sqlx::query_scalar(
                    "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?",
                )
                .bind(product_id)
                .bind(&branch_id)
                .fetch_optional(&mut *tx)
                .await?
                .flatten();
                let old_dec: f64 = old_in_tx.as_deref().unwrap_or("0").parse().unwrap_or(0.0);
                let delta = new_qty - old_dec;
                let new_qty_str = format!("{new_qty}");
                let delta_str = format!("{delta}");

                // Upsert stock level
                let stock_level_id = format!("SL-{}-{}", product_id, branch_id);
                sqlx::query(
                    "INSERT INTO stock_levels (stock_level_id, product_id, branch_id, quantity_on_hand, created_at, updated_at, last_movement_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?)
                     ON CONFLICT(product_id, branch_id)
                     DO UPDATE SET quantity_on_hand = excluded.quantity_on_hand,
                                   updated_at = excluded.updated_at,
                                   last_movement_at = excluded.last_movement_at,
                                   sync_status = 'pending'",
                )
                .bind(&stock_level_id)
                .bind(product_id)
                .bind(&branch_id)
                .bind(&new_qty_str)
                .bind(&now)
                .bind(&now)
                .bind(&now)
                .execute(&mut *tx)
                .await?;

                // Record stock movement
                let movement_id = ulid::Ulid::new().to_string();
                sqlx::query(
                    "INSERT INTO stock_movements
                     (movement_id, product_id, branch_id, device_id, origin_device_id,
                      movement_type, quantity_delta, quantity_after,
                      reference_type, notes, created_by_user_id, created_at, sync_status)
                     VALUES (?,?,?,?,?,'stock_take',?,?,'ai_action',NULL,'AI_ADMIN',?,'pending')",
                )
                .bind(&movement_id)
                .bind(product_id)
                .bind(&branch_id)
                .bind(&device_id)
                .bind(&device_id)
                .bind(&delta_str)
                .bind(&new_qty_str)
                .bind(&now)
                .execute(&mut *tx)
                .await?;

                tx.commit().await?;

                write_audit(
                    pool,
                    "AI_ADMIN",
                    "stock.bulk_take",
                    product_id,
                    &json!({ "from": old_qty, "to": new_qty }),
                )
                .await?;

                undo_items.push(json!({ "product_id": product_id, "new_quantity": old_qty }));
                results.push(format!("{}: {} → {}", &product_id[..8.min(product_id.len())], old_qty, new_qty));
            }

            Ok(MutationResult {
                description: format!("Bulk stock take applied: {}", results.join(", ")),
                undo_snapshot_json: json!({ "items": undo_items }).to_string(),
                rollback_tool: "bulk_stock_take".into(),
                rollback_input_json: json!({ "items": undo_items }).to_string(),
                entity_type: "stock_level".into(),
                entity_id: "bulk".into(),
            })
        }
        // ── Category executions ─────────────────────────────────────────────────
        "create_category" => {
            let name = input.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let sort = input.get("sort_order").and_then(|v| v.as_i64()).unwrap_or(0);
            let category_id = ulid::Ulid::new().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("INSERT INTO categories (category_id, name, sort_order, is_active, created_at, updated_at) VALUES (?, ?, ?, 1, ?, ?)")
                .bind(&category_id).bind(name).bind(sort).bind(&now).bind(&now).execute(pool).await?;
            write_audit(pool, "AI_ADMIN", "category.create", &category_id, &json!({"name":name,"sort_order":sort})).await?;
            Ok(MutationResult {
                description: format!("Category '{}' created", name),
                undo_snapshot_json: json!({"category_id":&category_id}).to_string(),
                rollback_tool: "update_category".into(),
                rollback_input_json: json!({"category_id":&category_id,"is_active":false}).to_string(),
                entity_type: "category".into(), entity_id: category_id,
            })
        }
        "update_category" => {
            let category_id = input.get("category_id").and_then(|v| v.as_str()).unwrap_or("");
            let row = sqlx::query("SELECT name, sort_order, is_active FROM categories WHERE category_id = ?")
                .bind(category_id).fetch_optional(pool).await?
                .ok_or_else(|| AppError::NotFound("Category not found".into()))?;
            let old_name: String = row.get("name");
            let old_sort: i64 = row.get("sort_order");
            let old_active: bool = row.get("is_active");
            let name = input.get("name").and_then(|v| v.as_str()).unwrap_or(&old_name);
            let sort = input.get("sort_order").and_then(|v| v.as_i64()).unwrap_or(old_sort);
            let active_val = input.get("is_active").and_then(|v| v.as_bool()).unwrap_or(old_active);
            sqlx::query("UPDATE categories SET name=?, sort_order=?, is_active=?, updated_at=?, version = version + 1, sync_status = 'pending' WHERE category_id=?")
                .bind(name).bind(sort).bind(active_val as i64).bind(chrono::Utc::now().to_rfc3339()).bind(category_id).execute(pool).await?;
            write_audit(pool, "AI_ADMIN", "category.update", category_id, &json!({"name":name,"is_active":active_val})).await?;
            Ok(MutationResult {
                description: format!("Category '{}' updated", name),
                undo_snapshot_json: json!({"name":old_name,"sort_order":old_sort,"is_active":old_active}).to_string(),
                rollback_tool: "update_category".into(),
                rollback_input_json: json!({"category_id":category_id,"name":old_name,"sort_order":old_sort,"is_active":old_active}).to_string(),
                entity_type: "category".into(), entity_id: category_id.into(),
            })
        }
        // ── User executions ─────────────────────────────────────────────────────
        "create_user" => {
            let display = input.get("display_name").and_then(|v| v.as_str()).unwrap_or("");
            let username = input.get("username").and_then(|v| v.as_str()).unwrap_or("");
            let pin = input.get("pin").and_then(|v| v.as_str()).unwrap_or("");
            let role_id = input.get("role_id").and_then(|v| v.as_str()).unwrap_or("");
            if pin.len() < 4 { return Err(AppError::Validation("PIN must be 4+ digits".into())); }
            let user_id = ulid::Ulid::new().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            let pin_hash = crate::db::repositories::auth_repo::hash_pin(pin)?;
            let branch_id = active_branch_id(pool).await?;
            sqlx::query("INSERT INTO users (user_id, branch_id, display_name, username, pin_hash, role_id, is_active, created_at, updated_at) VALUES (?,?,?,?,?,?,1,?,?)")
                .bind(&user_id).bind(&branch_id).bind(display).bind(username).bind(&pin_hash).bind(role_id).bind(&now).bind(&now).execute(pool).await?;
            write_audit(pool, "AI_ADMIN", "user.create", &user_id, &json!({"display_name":display,"username":username})).await?;
            Ok(MutationResult {
                description: format!("Staff '{}' (@{}) created", display, username),
                undo_snapshot_json: json!({"user_id":&user_id}).to_string(),
                rollback_tool: "update_user".into(),
                rollback_input_json: json!({"user_id":&user_id,"is_active":false}).to_string(),
                entity_type: "user".into(), entity_id: user_id,
            })
        }
        "update_user" => {
            let user_id = input.get("user_id").and_then(|v| v.as_str()).unwrap_or("");
            let row = sqlx::query("SELECT display_name, role_id, is_active FROM users WHERE user_id = ?")
                .bind(user_id).fetch_optional(pool).await?
                .ok_or_else(|| AppError::NotFound("User not found".into()))?;
            let old_name: String = row.get("display_name");
            let old_role: String = row.get("role_id");
            let old_active: bool = row.get("is_active");
            // S-01: parameterized binds — never interpolate AI-supplied values into SQL.
            let mut sets: Vec<&str> = Vec::new();
            let mut binds: Vec<SqlBind> = Vec::new();
            let mut undo = serde_json::Map::new();
            undo.insert("user_id".into(), json!(user_id));
            if let Some(n) = input.get("display_name").and_then(|v| v.as_str()) { sets.push("display_name = ?"); binds.push(SqlBind::S(n.to_string())); undo.insert("display_name".into(), json!(old_name)); }
            if let Some(r) = input.get("role_id").and_then(|v| v.as_str()) { sets.push("role_id = ?"); binds.push(SqlBind::S(r.to_string())); undo.insert("role_id".into(), json!(old_role)); }
            if let Some(a) = input.get("is_active").and_then(|v| v.as_bool()) { sets.push("is_active = ?"); binds.push(SqlBind::I(if a {1} else {0})); undo.insert("is_active".into(), json!(old_active)); }
            if let Some(pin_val) = input.get("pin").and_then(|v| v.as_str()) {
                if pin_val.len() < 4 { return Err(AppError::Validation("PIN must be 4+ digits".into())); }
                let pin_hash = crate::db::repositories::auth_repo::hash_pin(pin_val)?;
                sets.push("pin_hash = ?"); binds.push(SqlBind::S(pin_hash));
            }
            if !sets.is_empty() {
                let sql = format!("UPDATE users SET {}, sync_status = 'pending' WHERE user_id = ?", sets.join(", "));
                let mut q = sqlx::query(&sql);
                for b in &binds { q = b.apply(q); }
                q.bind(user_id).execute(pool).await?;
            }
            write_audit(pool, "AI_ADMIN", "user.update", user_id, &json!({})).await?;
            Ok(MutationResult {
                description: format!("User '{}' updated", old_name),
                undo_snapshot_json: serde_json::Value::Object(undo.clone()).to_string(),
                rollback_tool: "update_user".into(),
                rollback_input_json: serde_json::Value::Object(undo).to_string(),
                entity_type: "user".into(), entity_id: user_id.into(),
            })
        }
        // ── Tax rule executions ─────────────────────────────────────────────────
        "create_tax_rule" => {
            let name = input.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let bp = input.get("rate_basis_points").and_then(|v| v.as_i64()).unwrap_or(0);
            let inclusive = input.get("inclusive").and_then(|v| v.as_bool()).unwrap_or(true);
            let tax_rule_id = ulid::Ulid::new().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("INSERT INTO tax_rules (tax_rule_id, name, rate_basis_points, inclusive, is_active, effective_from, created_at, updated_at) VALUES (?,?,?,?,1,?,?,?)")
                .bind(&tax_rule_id).bind(name).bind(bp).bind(inclusive).bind(&now).bind(&now).bind(&now).execute(pool).await?;
            write_audit(pool, "AI_ADMIN", "tax_rule.create", &tax_rule_id, &json!({"name":name,"rate_basis_points":bp})).await?;
            Ok(MutationResult {
                description: format!("Tax rule '{}' created ({} bp {})", name, bp, if inclusive {"inclusive"} else {"exclusive"}),
                undo_snapshot_json: json!({"tax_rule_id":&tax_rule_id}).to_string(),
                rollback_tool: "update_tax_rule".into(),
                rollback_input_json: json!({"tax_rule_id":&tax_rule_id,"is_active":false}).to_string(),
                entity_type: "tax_rule".into(), entity_id: tax_rule_id,
            })
        }
        "update_tax_rule" => {
            let tax_rule_id = input.get("tax_rule_id").and_then(|v| v.as_str()).unwrap_or("");
            let row = sqlx::query("SELECT name, rate_basis_points, inclusive, is_active FROM tax_rules WHERE tax_rule_id = ?")
                .bind(tax_rule_id).fetch_optional(pool).await?
                .ok_or_else(|| AppError::NotFound("Tax rule not found".into()))?;
            let old_name: String = row.get("name");
            let old_bp: i64 = row.get("rate_basis_points");
            let old_inclusive: bool = row.get("inclusive");
            let old_active: bool = row.get("is_active");
            let name = input.get("name").and_then(|v| v.as_str()).unwrap_or(&old_name);
            let bp = input.get("rate_basis_points").and_then(|v| v.as_i64()).unwrap_or(old_bp);
            let inc = input.get("inclusive").and_then(|v| v.as_bool()).unwrap_or(old_inclusive);
            let active_val = input.get("is_active").and_then(|v| v.as_bool()).unwrap_or(old_active);
            sqlx::query("UPDATE tax_rules SET name=?, rate_basis_points=?, inclusive=?, is_active=?, updated_at=?, sync_status = 'pending' WHERE tax_rule_id=?")
                .bind(name).bind(bp).bind(inc).bind(active_val as i64).bind(chrono::Utc::now().to_rfc3339()).bind(tax_rule_id).execute(pool).await?;
            write_audit(pool, "AI_ADMIN", "tax_rule.update", tax_rule_id, &json!({"name":name,"rate_basis_points":bp})).await?;
            Ok(MutationResult {
                description: format!("Tax rule '{}' updated", name),
                undo_snapshot_json: json!({"name":old_name,"rate_basis_points":old_bp,"inclusive":old_inclusive,"is_active":old_active}).to_string(),
                rollback_tool: "update_tax_rule".into(),
                rollback_input_json: json!({"tax_rule_id":tax_rule_id,"name":old_name,"rate_basis_points":old_bp,"inclusive":old_inclusive,"is_active":old_active}).to_string(),
                entity_type: "tax_rule".into(), entity_id: tax_rule_id.into(),
            })
        }
        // ── Holistic product update ────────────────────────────────────────────
        "update_product_full" => {
            let product_id = input.get("product_id").and_then(|v| v.as_str()).unwrap_or("");
            let p = product_repo::get_product_by_id(pool, product_id).await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let name = input.get("name").and_then(|v| v.as_str()).unwrap_or(&p.product.name);
            let cat_id = input.get("category_id").and_then(|v| v.as_str()).unwrap_or(&p.product.category_id);
            let sku = input.get("sku").and_then(|v| v.as_str());
            let barcode = input.get("barcode").and_then(|v| v.as_str());
            let price = input.get("price_minor").and_then(|v| v.as_i64());
            let tax = input.get("tax_rule_id").and_then(|v| v.as_str());
            let track = input.get("track_inventory").and_then(|v| v.as_bool()).unwrap_or(p.product.track_inventory);
            let decimal = input.get("allow_decimal_quantity").and_then(|v| v.as_bool()).unwrap_or(p.product.allow_decimal_quantity);
            let rp = input.get("reorder_point").and_then(|v| v.as_f64());
            let active = input.get("is_active").and_then(|v| v.as_bool()).unwrap_or(p.product.is_active);
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("UPDATE products SET name=?,category_id=?,sku=COALESCE(?,sku),barcode=COALESCE(?,barcode),tax_rule_id=COALESCE(?,tax_rule_id),track_inventory=?,allow_decimal_quantity=?,is_active=?,version=version+1,updated_at=?, sync_status = 'pending' WHERE product_id=?")
                .bind(name).bind(cat_id).bind(sku).bind(barcode).bind(tax).bind(track as i64).bind(decimal as i64).bind(active as i64).bind(&now).bind(product_id).execute(pool).await?;
            if let Some(rp_val) = rp {
                sqlx::query("UPDATE products SET reorder_point = ?, updated_at = ?, sync_status = 'pending' WHERE product_id = ?")
                    .bind(rp_val as i64).bind(&now).bind(product_id).execute(pool).await?;
            }
            if let Some(new_price) = price {
                sqlx::query("UPDATE product_prices SET effective_to=?, sync_status = 'pending' WHERE product_id=? AND price_type='selling' AND effective_to IS NULL").bind(&now).bind(product_id).execute(pool).await?;
                let pid = ulid::Ulid::new().to_string();
                sqlx::query("INSERT INTO product_prices (price_id,product_id,branch_id,price_type,price_minor,currency,effective_from,created_by_user_id,created_at) VALUES (?,?,NULL,'selling',?,'BHD',?,'AI_ADMIN',?)")
                    .bind(&pid).bind(product_id).bind(new_price).bind(&now).bind(&now).execute(pool).await?;
            }
            write_audit(pool, "AI_ADMIN", "product.update_full", product_id, &json!({"name":name})).await?;
            Ok(MutationResult {
                description: format!("Product '{}' fully updated", name),
                undo_snapshot_json: json!({"product_id":product_id}).to_string(),
                rollback_tool: "update_product_full".into(),
                rollback_input_json: json!({"product_id":product_id,"name":p.product.name,"category_id":p.product.category_id,"track_inventory":p.product.track_inventory,"is_active":p.product.is_active}).to_string(),
                entity_type: "product".into(), entity_id: product_id.into(),
            })
        }
        // ── Store settings ──────────────────────────────────────────────────────
        "update_store_settings" => {
            let row = sqlx::query("SELECT name, timezone, address, phone, tax_number, cr_number, receipt_header, receipt_footer FROM branches WHERE is_active=1 LIMIT 1")
                .fetch_optional(pool).await?.ok_or_else(|| AppError::NotFound("No active branch".into()))?;
            let _old_name: String = row.get("name");
            // S-01: parameterized binds for store-settings update.
            let mut updates: Vec<String> = Vec::new();
            let mut binds: Vec<SqlBind> = Vec::new();
            let mut undo_map = serde_json::Map::new();
            for (key, old_val) in [("name", row.get::<String,_>("name")), ("address", row.get::<Option<String>,_>("address").unwrap_or_default()), ("phone", row.get::<Option<String>,_>("phone").unwrap_or_default()), ("tax_number", row.get::<Option<String>,_>("tax_number").unwrap_or_default()), ("cr_number", row.get::<Option<String>,_>("cr_number").unwrap_or_default()), ("receipt_header", row.get::<Option<String>,_>("receipt_header").unwrap_or_default()), ("receipt_footer", row.get::<Option<String>,_>("receipt_footer").unwrap_or_default()), ("timezone", row.get::<Option<String>,_>("timezone").unwrap_or("Asia/Bahrain".into()))].iter() {
                if let Some(v) = input.get(*key).and_then(|v| v.as_str()) { updates.push(format!("{} = ?", key)); binds.push(SqlBind::S(v.to_string())); undo_map.insert(key.to_string(), json!(old_val)); }
            }
            if !updates.is_empty() {
                let sql = format!("UPDATE branches SET {} WHERE is_active=1", updates.join(", "));
                let mut q = sqlx::query(&sql);
                for b in &binds { q = b.apply(q); }
                q.execute(pool).await?;
            }
            write_audit(pool, "AI_ADMIN", "store_settings.update", "branch", &json!({})).await?;
            Ok(MutationResult {
                description: format!("Store settings updated"),
                undo_snapshot_json: serde_json::Value::Object(undo_map.clone()).to_string(),
                rollback_tool: "update_store_settings".into(),
                rollback_input_json: serde_json::Value::Object(undo_map).to_string(),
                entity_type: "branch".into(), entity_id: "active".into(),
            })
        }
        // ── Business rules ──────────────────────────────────────────────────────
        "update_business_rules" => {
            for (key, api_name) in [("flag_allow_negative_stock", "allow_negative_stock"), ("flag_require_discount_reason", "require_discount_reason"), ("flag_cashier_can_discount", "cashier_can_discount"), ("flag_auto_print_receipt", "auto_print_receipt")].iter() {
                if let Some(v) = input.get(*api_name).and_then(|v| v.as_bool()) {
                    let val = if v { "1" } else { "0" };
                    sqlx::query("INSERT INTO app_config (key, value, updated_at) VALUES (?, ?, ?) ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at")
                        .bind(key).bind(val).bind(chrono::Utc::now().to_rfc3339()).execute(pool).await?;
                }
            }
            write_audit(pool, "AI_ADMIN", "business_rules.update", "rules", &json!({})).await?;
            Ok(MutationResult {
                description: "Business rules updated".into(),
                undo_snapshot_json: "{}".into(), rollback_tool: "_no_undo".into(), rollback_input_json: "{}".into(),
                entity_type: "app_config".into(), entity_id: "flags".into(),
            })
        }
        // ── Delivery payment/cancel ────────────────────────────────────────────
        "confirm_delivery_payment" => {
            let delivery_id = input.get("delivery_id").and_then(|v| v.as_str()).unwrap_or("");
            let row = sqlx::query("SELECT payment_status FROM delivery_orders WHERE delivery_id = ?")
                .bind(delivery_id).fetch_optional(pool).await?
                .ok_or_else(|| AppError::NotFound("Delivery not found".into()))?;
            let old_payment: String = row.get("payment_status");
            sqlx::query("UPDATE delivery_orders SET payment_status='paid', updated_at=?, sync_status = 'pending', version = version + 1 WHERE delivery_id=?")
                .bind(chrono::Utc::now().to_rfc3339()).bind(delivery_id).execute(pool).await?;
            write_audit(pool, "AI_ADMIN", "delivery.payment_confirmed", delivery_id, &json!({})).await?;
            Ok(MutationResult {
                description: format!("Payment confirmed for delivery {}", &delivery_id[..8.min(delivery_id.len())]),
                undo_snapshot_json: json!({"payment_status":old_payment}).to_string(),
                rollback_tool: "confirm_delivery_payment".into(),
                rollback_input_json: json!({"delivery_id":delivery_id}).to_string(),
                entity_type: "delivery_order".into(), entity_id: delivery_id.into(),
            })
        }
        "cancel_delivery" => {
            let delivery_id = input.get("delivery_id").and_then(|v| v.as_str()).unwrap_or("");
            let row = sqlx::query("SELECT delivery_status FROM delivery_orders WHERE delivery_id = ?")
                .bind(delivery_id).fetch_optional(pool).await?
                .ok_or_else(|| AppError::NotFound("Delivery not found".into()))?;
            let old_status: String = row.get("delivery_status");
            sqlx::query("UPDATE delivery_orders SET delivery_status='cancelled', updated_at=?, sync_status = 'pending', version = version + 1 WHERE delivery_id=?")
                .bind(chrono::Utc::now().to_rfc3339()).bind(delivery_id).execute(pool).await?;
            write_audit(pool, "AI_ADMIN", "delivery.cancelled", delivery_id, &json!({})).await?;
            Ok(MutationResult {
                description: format!("Delivery {} cancelled", &delivery_id[..8.min(delivery_id.len())]),
                undo_snapshot_json: json!({"delivery_status":old_status}).to_string(),
                rollback_tool: "advance_delivery_status".into(),
                rollback_input_json: json!({"delivery_id":delivery_id,"new_status":if old_status=="out_for_delivery"{"in_transit"}else{old_status.as_str()}}).to_string(),
                entity_type: "delivery_order".into(), entity_id: delivery_id.into(),
            })
        }
        // ── Sync repair executors ───────────────────────────────────────────
        "sync_reset_stuck" => {
            let mut total = 0u32;
            for table in crate::commands::sync_commands::SYNC_TABLES {
                let rows = sqlx::query(
                    &format!("UPDATE {table} SET sync_attempts = 0 WHERE sync_status = 'pending' AND sync_attempts >= 10"),
                ).execute(pool).await?.rows_affected();
                total += rows as u32;
            }
            write_audit(pool, "AI_ADMIN", "sync.reset_stuck", "sync", &json!({"reset":total})).await?;
            Ok(MutationResult {
                description: format!("Reset {total} stuck rows — sync worker will retry on next cycle"),
                undo_snapshot_json: "{}".into(), rollback_tool: "_no_undo".into(), rollback_input_json: "{}".into(),
                entity_type: "sync".into(), entity_id: "reset_stuck".into(),
            })
        }
        "sync_queue_retry" => {
            let event_id = input.get("event_id").and_then(|v| v.as_str()).unwrap_or("");
            let (table, row_id) = event_id.split_once(':').ok_or_else(|| AppError::Validation("Expected format table:entity_id".into()))?;
            let pk = crate::commands::sync_commands::table_pk(table);
            let sql = format!("UPDATE {table} SET sync_status='pending', sync_attempts=0 WHERE {pk}=?");
            let rows = sqlx::query(&sql).bind(row_id).execute(pool).await?.rows_affected();
            write_audit(pool, "AI_ADMIN", "sync.queue_retry", "sync", &json!({"event":event_id})).await?;
            Ok(MutationResult {
                description: format!("Retried sync event {event_id} ({rows} row reset)"),
                undo_snapshot_json: "{}".into(), rollback_tool: "_no_undo".into(), rollback_input_json: "{}".into(),
                entity_type: "sync".into(), entity_id: event_id.into(),
            })
        }
        "sync_queue_dismiss" => {
            let event_id = input.get("event_id").and_then(|v| v.as_str()).unwrap_or("");
            let (table, row_id) = event_id.split_once(':').ok_or_else(|| AppError::Validation("Expected format table:entity_id".into()))?;
            let pk = crate::commands::sync_commands::table_pk(table);
            let sql = format!("UPDATE {table} SET sync_status='synced', sync_attempts=0 WHERE {pk}=?");
            let rows = sqlx::query(&sql).bind(row_id).execute(pool).await?.rows_affected();
            write_audit(pool, "AI_ADMIN", "sync.queue_dismiss", "sync", &json!({"event":event_id})).await?;
            Ok(MutationResult {
                description: format!("Dismissed sync event {event_id} ({rows} row dismissed)"),
                undo_snapshot_json: "{}".into(), rollback_tool: "_no_undo".into(), rollback_input_json: "{}".into(),
                entity_type: "sync".into(), entity_id: event_id.into(),
            })
        }
        "void_sale" => {
            let receipt = input.get("receipt_number").and_then(|v| v.as_str()).unwrap_or("");
            let reason = input.get("reason").and_then(|v| v.as_str()).unwrap_or("");
            if reason.trim().is_empty() { return Err(AppError::Validation("Reason is required for void".into())); }
            let sale_id: Option<String> = sqlx::query_scalar("SELECT sale_id FROM sales WHERE receipt_number=? AND status='completed'").bind(receipt).fetch_optional(pool).await?.flatten();
            let sale_id = sale_id.ok_or_else(|| AppError::NotFound(format!("Sale {receipt} not found or already voided")))?;
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("UPDATE sales SET status='voided', updated_at=?, sync_status='pending' WHERE sale_id=?")
                .bind(&now).bind(&sale_id).execute(pool).await?;
            sqlx::query("UPDATE sale_items SET voided=1 WHERE sale_id=?")
                .bind(&sale_id).execute(pool).await?;
            write_audit(pool, "AI_ADMIN", "sale.voided", "sale", &json!({"sale_id":sale_id,"reason":reason})).await?;
            Ok(MutationResult {
                description: format!("Voided sale {receipt}: {reason}"),
                undo_snapshot_json: json!({"sale_id":sale_id,"receipt":receipt}).to_string(),
                rollback_tool: "_no_undo".into(), rollback_input_json: "{}".into(),
                entity_type: "sale".into(), entity_id: sale_id,
            })
        }
        "delete_customer" => {
            let cid = input.get("customer_id").and_then(|v| v.as_str()).unwrap_or("");
            let name: Option<String> = sqlx::query_scalar("SELECT name FROM customers WHERE customer_id=?").bind(cid).fetch_optional(pool).await?.flatten();
            let name = name.ok_or_else(|| AppError::NotFound("Customer not found".into()))?;
            let snapshot = json!({"customer_id": cid, "name": name});
            sqlx::query("DELETE FROM customers WHERE customer_id=?").bind(cid).execute(pool).await?;
            write_audit(pool, "AI_ADMIN", "customer.deleted", "customer", &snapshot).await?;
            Ok(MutationResult {
                description: format!("Deleted customer: {name}"),
                undo_snapshot_json: snapshot.to_string(),
                rollback_tool: "create_customer".into(), rollback_input_json: snapshot.to_string(),
                entity_type: "customer".into(), entity_id: cid.into(),
            })
        }
        "set_device_active" => {
            let did = input.get("device_id").and_then(|v| v.as_str()).unwrap_or("");
            let active = input.get("is_active").and_then(|v| v.as_bool()).unwrap_or(true);
            let name: Option<String> = sqlx::query_scalar("SELECT name FROM devices WHERE device_id=?").bind(did).fetch_optional(pool).await?.flatten();
            let name = name.unwrap_or_else(|| did.to_string());
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("UPDATE devices SET is_active=?, updated_at=?, sync_status='pending' WHERE device_id=?")
                .bind(active as i64).bind(&now).bind(did).execute(pool).await?;
            write_audit(pool, "AI_ADMIN", "device.toggle", "device", &json!({"device_id":did,"active":active})).await?;
            Ok(MutationResult {
                description: format!("Device '{name}' set to active={active}"),
                undo_snapshot_json: json!({"device_id":did,"is_active":!active}).to_string(),
                rollback_tool: "set_device_active".into(), rollback_input_json: json!({"device_id":did,"is_active":!active}).to_string(),
                entity_type: "device".into(), entity_id: did.into(),
            })
        }
        "receive_stock" => {
            let pid = input.get("product_id").and_then(|v| v.as_str()).unwrap_or("");
            let qty = input.get("quantity").and_then(|v| v.as_str()).unwrap_or("0");
            let notes = input.get("notes").and_then(|v| v.as_str()).unwrap_or("");
            let pname: Option<String> = sqlx::query_scalar("SELECT name FROM products WHERE product_id=?").bind(pid).fetch_optional(pool).await?.flatten();
            let _ = pname.ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let branch_id: String = sqlx::query_scalar("SELECT branch_id FROM branches WHERE is_active=1 LIMIT 1").fetch_one(pool).await?;
            let now = chrono::Utc::now().to_rfc3339();
            let level_id = format!("SL-{}-{}", pid, branch_id);
            sqlx::query("INSERT INTO stock_levels (stock_level_id,product_id,branch_id,quantity_on_hand,last_movement_at,created_at,updated_at) VALUES (?,?,?,?,?,?,?) ON CONFLICT(product_id,branch_id) DO UPDATE SET quantity_on_hand = CAST(CAST(stock_levels.quantity_on_hand AS REAL) + CAST(? AS REAL) AS TEXT), last_movement_at=?, updated_at=?, sync_status='pending'")
                .bind(&level_id).bind(pid).bind(&branch_id).bind(qty).bind(&now).bind(&now).bind(&now).bind(qty).bind(&now).bind(&now).execute(pool).await?;
            let mid = ulid::Ulid::new().to_string();
            let qty_after: String = sqlx::query_scalar("SELECT quantity_on_hand FROM stock_levels WHERE product_id=? AND branch_id=?").bind(pid).bind(&branch_id).fetch_one(pool).await?;
            sqlx::query("INSERT INTO stock_movements (movement_id,product_id,branch_id,device_id,origin_device_id,movement_type,quantity_delta,quantity_after,reference_type,notes,created_by_user_id,created_at) VALUES (?,?,?,?,(SELECT device_id FROM devices WHERE is_active=1 LIMIT 1),'receive',?,?,'receive',?,'AI_ADMIN',?)")
                .bind(&mid).bind(pid).bind(&branch_id).bind(&branch_id).bind(qty).bind(&qty_after).bind(notes).bind(&now).execute(pool).await?;
            write_audit(pool, "AI_ADMIN", "stock.receive", "product", &json!({"product_id":pid,"qty":qty,"notes":notes})).await?;
            Ok(MutationResult {
                description: format!("Received {qty} of product {pid}"),
                undo_snapshot_json: json!({"product_id":pid,"qty":qty}).to_string(),
                rollback_tool: "adjust_stock".into(), rollback_input_json: json!({"product_id":pid,"delta":format!("-{}",qty)}).to_string(),
                entity_type: "stock".into(), entity_id: mid,
            })
        }
        "add_loyalty_points" => {
            let cid = input.get("customer_id").and_then(|v| v.as_str()).unwrap_or("");
            let pts = input.get("points").and_then(|v| v.as_i64()).unwrap_or(0);
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("UPDATE customers SET loyalty_points = loyalty_points + ?, updated_at = ?, sync_status = 'pending' WHERE customer_id = ?")
                .bind(pts).bind(&now).bind(cid).execute(pool).await?;
            let new_total: i64 = sqlx::query_scalar("SELECT loyalty_points FROM customers WHERE customer_id=?").bind(cid).fetch_one(pool).await?;
            write_audit(pool, "AI_ADMIN", "customer.loyalty", "customer", &json!({"customer_id":cid,"added":pts,"total":new_total})).await?;
            Ok(MutationResult {
                description: format!("Added {pts} loyalty points, new total: {new_total}"),
                undo_snapshot_json: json!({"customer_id":cid,"points":-pts}).to_string(),
                rollback_tool: "add_loyalty_points".into(), rollback_input_json: json!({"customer_id":cid,"points":-pts}).to_string(),
                entity_type: "customer".into(), entity_id: cid.into(),
            })
        }
        "bulk_update_prices" => {
            let updates = input.get("updates").and_then(|v| v.as_array()).ok_or_else(|| AppError::Validation("updates array required".into()))?;
            let now = chrono::Utc::now().to_rfc3339();
            let mut updated = 0;
            for item in updates {
                let pid = item.get("product_id").and_then(|v| v.as_str()).unwrap_or("");
                let price = item.get("price_minor").and_then(|v| v.as_i64()).unwrap_or(0);
                if pid.is_empty() { continue; }
                sqlx::query("UPDATE product_prices SET effective_to=?, sync_status='pending' WHERE product_id=? AND price_type='selling' AND effective_to IS NULL")
                    .bind(&now).bind(pid).execute(pool).await?;
                let npid = ulid::Ulid::new().to_string();
                sqlx::query("INSERT INTO product_prices (price_id,product_id,branch_id,price_type,price_minor,currency,effective_from,created_by_user_id,created_at) VALUES (?,?,NULL,'selling',?,'BHD',?,'AI_ADMIN',?)")
                    .bind(&npid).bind(pid).bind(price).bind(&now).bind(&now).execute(pool).await?;
                updated += 1;
            }
            write_audit(pool, "AI_ADMIN", "product.bulk_price", "product", &json!({"count":updated})).await?;
            Ok(MutationResult {
                description: format!("Updated prices for {updated} products"),
                undo_snapshot_json: "{}".into(), rollback_tool: "_no_undo".into(), rollback_input_json: "{}".into(),
                entity_type: "product".into(), entity_id: "bulk".into(),
            })
        }
        // ── Database backup ────────────────────────────────────────────────────
        "backup_database" => {
            let app_data = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
            let db_src = format!("{}/ZANPOS/zanpos.db", app_data);
            let backup_dir = format!("{}/ZANPOS/backups", app_data);
            let _ = std::fs::create_dir_all(&backup_dir);
            let ts = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
            let backup_path = format!("{}/zanpos_backup_{}.db", backup_dir, ts);
            sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)").execute(pool).await?;
            std::fs::copy(&db_src, &backup_path)
                .map_err(|e| AppError::Internal(format!("Backup failed: {e}")))?;
            write_audit(pool, "AI_ADMIN", "backup.created", &ts, &json!({"path":&backup_path})).await?;
            Ok(MutationResult {
                description: format!("Database backed up to {}", backup_path),
                undo_snapshot_json: "{}".into(), rollback_tool: "_no_undo".into(), rollback_input_json: "{}".into(),
                entity_type: "backup".into(), entity_id: ts,
            })
        }
        name => crate::ai::tools_write_ext::execute(pool, name, input, currency_exp).await,
    }
}

// ── Undo executor ─────────────────────────────────────────────────────────────

pub async fn execute_undo(
    pool: &SqlitePool,
    rollback_tool: &str,
    rollback_input_json: &str,
    currency_exp: u32,
) -> AppResult<String> {
    let input: Value = serde_json::from_str(rollback_input_json)
        .map_err(|e| AppError::Validation(format!("Invalid rollback input: {}", e)))?;
    let result = execute_mutation(pool, rollback_tool, &input, currency_exp).await?;
    Ok(result.description)
}

// ── Audit helper ──────────────────────────────────────────────────────────────

async fn write_audit(
    pool: &SqlitePool,
    actor_user_id: &str,
    event_type: &str,
    entity_id: &str,
    after: &serde_json::Value,
) -> AppResult<()> {
    let id = ulid::Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let hash = format!("{:x}", md5_simple(&format!("{}{}{}", id, event_type, now)));
    // Do NOT include device_id / origin_device_id / branch_id / previous_hash
    // in the column list — let the schema DEFAULTs apply (origin_device_id is
    // TEXT NOT NULL DEFAULT ''; passing explicit NULL would violate that constraint
    // and return "Something went wrong" to the admin user on every mutation).
    sqlx::query(
        "INSERT INTO audit_logs
         (audit_log_id, event_type, entity_type, entity_id, actor_user_id,
          actor_type, after_json, created_at, hash)
         VALUES (?, ?, 'product', ?, ?, 'ai_agent', ?, ?, ?)",
    )
    .bind(&id)
    .bind(event_type)
    .bind(entity_id)
    .bind(actor_user_id)
    .bind(after.to_string())
    .bind(&now)
    .bind(&hash)
    .execute(pool)
    .await?;
    Ok(())
}

/// SHA-256 truncated to u64. Previously used DefaultHasher (non-deterministic, CWE-327).
/// This is for audit log hash entries — must be stable across Rust versions.
fn md5_simple(s: &str) -> u64 {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    let bytes = h.finalize();
    // Take the first 8 bytes as a u64 (still 64-bit collision resistance for audit chain)
    u64::from_le_bytes(bytes[..8].try_into().unwrap_or([0u8; 8]))
}

// ── PII helpers ───────────────────────────────────────────────────────────────

/// Mask a customer phone number for AI tool responses.
/// Keeps only the last 4 digits visible, e.g. "+973 3XXX X456" or "XXXX 4567".
/// This prevents full phone numbers from being stored in AI conversation history
/// or appearing in logs. The last 4 digits retain enough context to identify
/// the customer in a lookup without exposing the full number (PII-01).
pub fn mask_phone(phone: &str) -> String {
    let digits: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 4 {
        return "XXXXX".to_string();
    }
    let last4 = &digits[digits.len() - 4..];
    format!("XXXX-{}", last4)
}

// ── Integrity tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Every mutation tool definition MUST also appear in MUTATION_TOOLS.
    /// If a tool is missing, it would silently execute without admin confirmation.
    #[test]
    fn mutation_tools_list_is_complete() {
        let defs = all_tool_definitions();
        for d in &defs {
            let looks_like_mutation = d.description.contains("admin confirmation")
                || d.description.contains("CONFIRMATION REQUIRED");
            let is_listed = MUTATION_TOOLS.contains(&d.name.as_str());
            if looks_like_mutation && !is_listed {
                panic!(
                    "Tool '{}' mentions admin confirmation but is NOT in MUTATION_TOOLS. \
                     It would silently mutate data without confirmation. Add it to MUTATION_TOOLS.",
                    d.name
                );
            }
            if !looks_like_mutation && is_listed {
                // Warn (not panic): it's safe, just potentially unnecessary
                eprintln!(
                    "WARNING: Tool '{}' is in MUTATION_TOOLS but does not mention confirmation. \
                     Consider removing it from MUTATION_TOOLS if it's not a mutating tool.",
                    d.name
                );
            }
        }
    }
}
