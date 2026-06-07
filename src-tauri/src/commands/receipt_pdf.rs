/// PDF receipt generator — uses printpdf with built-in Helvetica Type1 fonts.
/// No external font files are required.  A5 portrait (148 × 210 mm).
use printpdf::*;
use std::io::BufWriter;

// ─── Input types (deserialized from Tauri command) ────────────────────────────

#[derive(Debug, serde::Deserialize)]
pub struct ReceiptItemInput {
    pub product_name: String,
    pub quantity: String,
    pub unit_price_minor: i64,
    pub line_total_minor: i64,
}

#[derive(Debug, serde::Deserialize)]
pub struct PaymentSummaryInput {
    pub method: String,
    pub amount_minor: i64,
    /// Change given to customer on cash payments (mirrors SaleResult.payments[].change_minor).
    pub change_minor: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
pub struct WhatsAppReceiptPdfInput {
    pub to: String,
    pub receipt_number: String,
    pub branch_name: String,
    pub branch_phone: Option<String>,
    pub cashier_name: String,
    pub sold_at: String,
    pub currency: String,
    pub currency_exponent: i32,
    pub items: Vec<ReceiptItemInput>,
    pub net_total_minor: i64,
    pub tax_total_minor: i64,
    pub discount_total_minor: i64,
    pub payments: Vec<PaymentSummaryInput>,
    pub caption: Option<String>,
    pub address_text: Option<String>,
    pub house_number: Option<String>,
    pub area: Option<String>,
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn fmt_money(minor: i64, exp: i32) -> String {
    if exp == 0 {
        return minor.to_string();
    }
    let divisor = 10_i64.pow(exp as u32);
    let whole = minor / divisor;
    let frac = (minor % divisor).abs();
    format!("{}.{:0>width$}", whole, frac, width = exp as usize)
}

/// Replace non-ASCII chars with '?' — required for Type1/Helvetica PDF fonts.
fn ascii(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii() { c } else { '?' })
        .collect()
}

// ─── PDF generator ────────────────────────────────────────────────────────────

/// Build an A5 portrait PDF receipt and return the raw bytes.
pub fn generate_receipt_pdf(inp: &WhatsAppReceiptPdfInput) -> Result<Vec<u8>, String> {
    let (doc, page1, layer1) = PdfDocument::new(
        format!("Receipt {}", inp.receipt_number),
        Mm(148.0_f32),
        Mm(210.0_f32),
        "Layer 1",
    );
    let layer = doc.get_page(page1).get_layer(layer1);

    let font = doc
        .add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|e| format!("Font error: {e}"))?;
    let font_b = doc
        .add_builtin_font(BuiltinFont::HelveticaBold)
        .map_err(|e| format!("Font error: {e}"))?;

    let mx: f32 = 10.0;   // left margin
    let rx: f32 = 108.0;  // right column start (amounts)
    let qx: f32 = 82.0;   // qty column
    let exp = inp.currency_exponent;
    let cur = &inp.currency;
    let mut y: f32 = 198.0;

    // ── Store name ──────────────────────────────────────────────────────────
    layer.use_text(ascii(&inp.branch_name), 14.0, Mm(mx), Mm(y), &font_b);
    y -= 7.0;

    if let Some(ph) = &inp.branch_phone {
        if !ph.is_empty() {
            layer.use_text(ascii(ph), 8.5, Mm(mx), Mm(y), &font);
            y -= 5.5;
        }
    }

    layer.use_text("--------------------------------------------------", 6.0, Mm(mx), Mm(y), &font);
    y -= 4.5;

    // ── RECEIPT header ──────────────────────────────────────────────────────
    layer.use_text("RECEIPT", 13.0, Mm(mx), Mm(y), &font_b);
    y -= 6.0;

    layer.use_text(
        format!("No:      {}", ascii(&inp.receipt_number)),
        8.5, Mm(mx), Mm(y), &font,
    );
    y -= 5.0;

    // Format sold_at: "2025-06-07T14:30:00..." → "2025-06-07 14:30"
    let date_part = inp.sold_at.split('T').next().unwrap_or(&inp.sold_at);
    let time_part = inp
        .sold_at
        .split('T')
        .nth(1)
        .and_then(|t| t.get(..5))
        .unwrap_or("");
    let date_str = if time_part.is_empty() {
        date_part.to_string()
    } else {
        format!("{} {}", date_part, time_part)
    };
    layer.use_text(
        format!("Date:    {}", date_str),
        8.5, Mm(mx), Mm(y), &font,
    );
    y -= 5.0;

    layer.use_text(
        format!("Cashier: {}", ascii(&inp.cashier_name)),
        8.5, Mm(mx), Mm(y), &font,
    );
    y -= 5.0;

    layer.use_text("--------------------------------------------------", 6.0, Mm(mx), Mm(y), &font);
    y -= 5.0;

    // ── Items header ────────────────────────────────────────────────────────
    layer.use_text("Item", 8.5, Mm(mx), Mm(y), &font_b);
    layer.use_text("Qty", 8.5, Mm(qx), Mm(y), &font_b);
    layer.use_text("Amount", 8.5, Mm(rx), Mm(y), &font_b);
    y -= 4.5;
    layer.use_text("--------------------------------------------------", 6.0, Mm(mx), Mm(y), &font);
    y -= 4.5;

    // ── Items ───────────────────────────────────────────────────────────────
    for item in &inp.items {
        if y < 32.0 {
            layer.use_text("(more items...)", 7.5, Mm(mx), Mm(y), &font);
            y -= 4.5;
            break;
        }
        let name = ascii(&item.product_name);
        let name_s = if name.len() > 37 {
            format!("{}..", &name[..35])
        } else {
            name
        };

        let qty_raw = &item.quantity;
        let qty_s = if qty_raw.contains('.') {
            qty_raw.trim_end_matches('0').trim_end_matches('.')
        } else {
            qty_raw.as_str()
        };

        layer.use_text(name_s, 8.5, Mm(mx), Mm(y), &font);
        layer.use_text(qty_s.to_string(), 8.5, Mm(qx), Mm(y), &font);
        layer.use_text(
            format!("{} {}", cur, fmt_money(item.line_total_minor, exp)),
            8.5, Mm(rx), Mm(y), &font,
        );
        y -= 5.0;
    }

    // ── Totals ──────────────────────────────────────────────────────────────
    layer.use_text("--------------------------------------------------", 6.0, Mm(mx), Mm(y), &font);
    y -= 5.0;

    if inp.discount_total_minor > 0 {
        let gross = inp.net_total_minor + inp.discount_total_minor;
        layer.use_text("Subtotal:", 8.5, Mm(mx), Mm(y), &font);
        layer.use_text(
            format!("{} {}", cur, fmt_money(gross, exp)),
            8.5, Mm(rx), Mm(y), &font,
        );
        y -= 5.0;

        layer.use_text("Discount:", 8.5, Mm(mx), Mm(y), &font);
        layer.use_text(
            format!("- {} {}", cur, fmt_money(inp.discount_total_minor, exp)),
            8.5, Mm(rx), Mm(y), &font,
        );
        y -= 5.0;
    }

    if inp.tax_total_minor > 0 {
        layer.use_text("Tax:", 8.5, Mm(mx), Mm(y), &font);
        layer.use_text(
            format!("{} {}", cur, fmt_money(inp.tax_total_minor, exp)),
            8.5, Mm(rx), Mm(y), &font,
        );
        y -= 5.0;
    }

    layer.use_text("TOTAL:", 11.0, Mm(mx), Mm(y), &font_b);
    layer.use_text(
        format!("{} {}", cur, fmt_money(inp.net_total_minor, exp)),
        11.0, Mm(rx), Mm(y), &font_b,
    );
    y -= 7.0;

    // ── Payments ────────────────────────────────────────────────────────────
    layer.use_text("--------------------------------------------------", 6.0, Mm(mx), Mm(y), &font);
    y -= 5.0;

    for payment in &inp.payments {
        if y < 22.0 { break; }
        let label = match payment.method.as_str() {
            "cash"   => "Cash",
            "card"   => "Card",
            "wallet" => "Wallet",
            _        => "Payment",
        };
        layer.use_text(
            format!("{}: {} {}", label, cur, fmt_money(payment.amount_minor, exp)),
            8.5, Mm(mx), Mm(y), &font,
        );
        y -= 5.0;

        // Change line (cash payments)
        if let Some(change) = payment.change_minor {
            if change > 0 && y >= 22.0 {
                layer.use_text(
                    format!("  Change: {} {}", cur, fmt_money(change, exp)),
                    8.5, Mm(mx), Mm(y), &font,
                );
                y -= 5.0;
            }
        }
    }

    // ── Delivery address ────────────────────────────────────────────────────
    let addr = inp.address_text.as_deref().unwrap_or("");
    if !addr.is_empty() && y > 30.0 {
        layer.use_text("--------------------------------------------------", 6.0, Mm(mx), Mm(y), &font);
        y -= 4.5;
        layer.use_text("Delivery Address:", 8.5, Mm(mx), Mm(y), &font_b);
        y -= 5.0;

        if y > 22.0 {
            layer.use_text(ascii(addr), 8.5, Mm(mx), Mm(y), &font);
            y -= 5.0;
        }
        if let Some(h) = &inp.house_number {
            if !h.is_empty() && y > 22.0 {
                layer.use_text(format!("Bldg/House: {}", ascii(h)), 8.5, Mm(mx), Mm(y), &font);
                y -= 5.0;
            }
        }
        if let Some(a) = &inp.area {
            if !a.is_empty() && y > 22.0 {
                layer.use_text(format!("Area: {}", ascii(a)), 8.5, Mm(mx), Mm(y), &font);
                y -= 5.0;
            }
        }
    }

    // ── Footer ──────────────────────────────────────────────────────────────
    if y > 18.0 {
        layer.use_text("--------------------------------------------------", 6.0, Mm(mx), Mm(y), &font);
        y -= 5.0;
    }
    if y > 14.0 {
        layer.use_text("Thank you for your purchase!", 9.0, Mm(mx), Mm(y), &font);
    }

    // ── Serialize ───────────────────────────────────────────────────────────
    drop(layer);
    drop(font);
    drop(font_b);

    let mut buf = BufWriter::new(Vec::new());
    doc.save(&mut buf)
        .map_err(|e| format!("PDF save error: {e}"))?;
    buf.into_inner().map_err(|e| format!("PDF buffer error: {e}"))
}
