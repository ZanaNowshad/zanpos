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
  /* A page-sized tail shaped like the store's actual imported catalogue:
     28,032 products, mostly long-category general goods with 13-digit
     barcodes and no stock row. Sixteen short rows never put any pressure on
     the table's column algorithm, so the product-name column always won the
     slack in QA — and collapsed to the width of its thumbnail against real
     data, leaving the barcode as the only thing on the row. */
  /* Two rows the import got wrong. A catalogue this size always has some, and
     without them the "(no product name)" fallback was unreachable in QA. */
  { barcode: "6941057409991", name: "", category: "General Merchandise", cost: 700, price: 1500, stock: 0 },
  { barcode: "6941057409992", name: "   ", category: "Stationery & Office", cost: 300, price: 500, stock: 0 },
  ...Array.from({ length: 82 }, (_, i) => ({
    /* Mixed identifier lengths, because they are mixed in the real catalogue:
       13-digit EAN alongside short internal codes like C011567. */
    barcode: i % 5 === 2 ? `C0${String(11000 + i)}` : `694105740${String(1000 + i).padStart(4, "0")}`,
    /* Real product names run long and start with numbers — "10 Inch Dinner
       Plate", "1000Amp Booster Cable" — so the name column has to be the one
       that gets the width. */
    name: [
      "10 Inch Dinner Plate with Wide Rim",
      "1000Amp Booster Cable Heavy Duty Set",
      "10 Colour Flame Candle Assorted Pack",
      "1 oz White Bowls Disposable (50 pack)",
    ][i % 4] + ` — variant ${i + 1}`,
    /* Long, multi-word categories. These wrapped to four lines and tripled
       every row's height before the column was bounded. */
    category: [
      "Air Fresheners & Home Fragrance",
      "Chocolate & Confectionery",
      "Tableware & Drinkware",
      "Electronics & Electrical",
      "Stationery & Office",
      "General Merchandise",
    ][i % 6],
    cost: 500 + i * 25,
    price: 900 + i * 30,
    stock: 0,
  })),
].map((p, i) => {
  /* Every saved view has to be reachable in QA, or a view is only ever tested
     in the case that returns rows. A loose item sold by weight has no barcode;
     a delisted line stays in the catalogue but inactive. */
  const noBarcode = i % 19 === 4;
  const inactive = i % 23 === 7;
  return {
    product_id: `prd_${i + 1}`,
    category_id: `cat_${p.category.toLowerCase()}`,
    category_name: p.category,
    name: p.name,
    sku: p.barcode,
    barcode: noBarcode ? null : p.barcode,
    barcodes: noBarcode ? [] : [p.barcode],
    track_inventory: true,
    allow_decimal_quantity: false,
    is_active: !inactive,
    tax_rule_id: null,
    tax_rule_name: null,
    price_minor: p.price,
    reorder_point: 10,
    image_path: null,
    // Extra fields consumed by list/table views.
    cost_minor: p.cost,
    stock_qty: p.stock,
    updated_at: new Date(Date.now() - i * 3_600_000).toISOString(),
  };
});

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

/**
 * A real store's category list, not a handful of samples.
 *
 * Volume is part of the fixture's job. A five-row mock keeps every list short
 * enough to fit the viewport, so nothing ever scrolls and layout defects that
 * only appear once content overflows — toolbars sliding under content panels,
 * nested scrollbars — cannot reproduce. The names are the real ones from the
 * store's catalogue so column widths are exercised honestly too.
 */
export const CATEGORY_NAMES = [
  "Cosmetics & Makeup", "Cleaning Supplies", "Art Supplies", "Laundry Care",
  "Hair Care", "Miscellaneous", "Chocolate & Candy", "Nuts & Dried Fruits",
  "Tableware", "Candles & Home Fragrance", "Dairy", "Beverages", "Grocery",
  "Bakery", "Household", "Frozen Foods", "Canned & Jarred", "Baby Care",
  "Personal Hygiene", "Paper Goods", "Pet Supplies", "Spices & Seasoning",
  "Rice & Pasta", "Cooking Oils", "Tea & Coffee", "Biscuits & Wafers",
  "Soft Drinks", "Water", "Juices", "Health & Wellness",
];

export const CATEGORIES = CATEGORY_NAMES.map((name, i) => ({
  category_id: `cat_${name.toLowerCase().replace(/[^a-z0-9]+/g, "_")}`,
  name,
  /* Shapes match admin_list_categories. `is_active` was missing entirely, so
     every category rendered as Inactive — a state the real query cannot
     produce for a live catalogue. A few empty and a few archived, because both
     are what a manager is scanning this list to find. */
  product_count: i % 7 === 3 ? 0 : (i * 13) % 240 + 1,
  is_active: i % 11 !== 5,
  parent_category_id: null,
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

/**
 * WhatsApp's own directory, as the till sees it.
 *
 * Deliberately overlapping and deliberately messy, because the merge in
 * paymentContacts.ts is only worth testing against a directory that behaves
 * like a real one: Fatima is both a saved customer and a WhatsApp contact and
 * must appear once; Mariam has chatted but was never saved; the shop's own
 * group is a chat that is not a person and must never be offered as a receipt
 * destination.
 */
export const WA_CONTACTS = [
  { id: "97336001122@s.whatsapp.net", name: "Fatima" },
  { id: "97336004455@s.whatsapp.net", name: "Hassan Al Mannai" },
  { id: "97339887766@s.whatsapp.net", name: "Mariam Al Kooheji" },
  { id: "97333221100@s.whatsapp.net", name: "Ali Ebrahim" },
];

export const WA_MESSAGES = [
  { id: "wam_1", chat_jid: "97339887766@s.whatsapp.net", chat_name: "Mariam Al Kooheji",
    is_group: false, sender_jid: "97339887766@s.whatsapp.net", sender_name: "Mariam Al Kooheji",
    body: "Do you have Almarai laban 2L?", ts: Math.floor(Date.now() / 1000) - 40 * 60,
    read: false, media_type: null },
  { id: "wam_2", chat_jid: "97336001122@s.whatsapp.net", chat_name: "Fatima",
    is_group: false, sender_jid: "97336001122@s.whatsapp.net", sender_name: "Fatima",
    body: "Same delivery address please", ts: Math.floor(Date.now() / 1000) - 5 * 3600,
    read: true, media_type: null },
  { id: "wam_3", chat_jid: "120363001122334455@g.us", chat_name: "Amwaj Staff",
    is_group: true, sender_jid: "97333221100@s.whatsapp.net", sender_name: "Ali Ebrahim",
    body: "Stock arriving at 6", ts: Math.floor(Date.now() / 1000) - 2 * 3600,
    read: true, media_type: null },
];

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
