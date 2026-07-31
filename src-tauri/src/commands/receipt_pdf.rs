/// PDF receipt generator — writes a minimal text-only PDF with built-in Helvetica fonts.
/// No external font files are required.  A5 portrait (148 × 210 mm).

// ─── Input types (deserialized from Tauri command) ────────────────────────────

#[derive(Debug, serde::Deserialize)]
pub struct ReceiptItemInput {
    pub product_name: String,
    pub quantity: String,
    /// Reserved for per-item line-price display in future PDF receipts.
    #[allow(dead_code)]
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
    /// Tax/VAT registration number (if configured for the branch).
    /// Shown on the receipt header only when non-empty.
    pub tax_number: Option<String>,
    /// Commercial Registration number (if configured for the branch).
    /// Shown on the receipt header only when non-empty.
    pub cr_number: Option<String>,
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn fmt_money(minor: i64, exp: i32) -> String {
    // Delegate to the canonical integer-only formatter to avoid the negative-amount
    // sign-loss bug (e.g. -500 with exp=3 was emitted as "0.500" instead of "-0.500").
    crate::domain::money::format_minor(minor, exp as u32)
}

/// Replace non-ASCII chars with '?' — required for Type1/Helvetica PDF fonts.
fn ascii(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii() { c } else { '?' })
        .collect()
}

#[derive(Default)]
struct ReceiptLayer {
    stream: String,
}

#[derive(Clone, Copy)]
struct Mm(f32);

#[derive(Clone, Copy)]
enum ReceiptFont {
    Regular,
    Bold,
}

impl ReceiptLayer {
    fn use_text<T: Into<String>>(&mut self, text: T, size: f32, x: Mm, y: Mm, font: &ReceiptFont) {
        let font_name = match font {
            ReceiptFont::Regular => "F1",
            ReceiptFont::Bold => "F2",
        };
        self.stream.push_str(&format!(
            "BT /{} {:.2} Tf 1 0 0 1 {:.2} {:.2} Tm ({}) Tj ET\n",
            font_name,
            size,
            mm_to_pt(x.0),
            mm_to_pt(y.0),
            escape_pdf_text(&text.into())
        ));
    }

    fn into_stream(self) -> String {
        self.stream
    }
}

fn mm_to_pt(mm: f32) -> f32 {
    mm * 72.0 / 25.4
}

fn escape_pdf_text(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '(' => "\\(".chars().collect::<Vec<_>>(),
            ')' => "\\)".chars().collect::<Vec<_>>(),
            '\r' | '\n' => " ".chars().collect::<Vec<_>>(),
            c if c.is_ascii() => vec![c],
            _ => vec!['?'],
        })
        .collect()
}

fn write_pdf_object(out: &mut Vec<u8>, offsets: &mut [usize], id: usize, body: &str) {
    offsets[id] = out.len();
    out.extend_from_slice(format!("{id} 0 obj\n{body}\nendobj\n").as_bytes());
}

fn build_text_pdf(title: &str, width_mm: f32, height_mm: f32, content: &str) -> Vec<u8> {
    let mut out = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = vec![0usize; 8];
    let escaped_title = escape_pdf_text(title);
    let content_bytes = content.as_bytes();

    write_pdf_object(
        &mut out,
        &mut offsets,
        1,
        "<< /Type /Catalog /Pages 2 0 R >>",
    );
    write_pdf_object(
        &mut out,
        &mut offsets,
        2,
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    );
    write_pdf_object(
        &mut out,
        &mut offsets,
        3,
        &format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.2} {:.2}] /Resources << /Font << /F1 4 0 R /F2 5 0 R >> >> /Contents 6 0 R >>",
            mm_to_pt(width_mm),
            mm_to_pt(height_mm)
        ),
    );
    write_pdf_object(
        &mut out,
        &mut offsets,
        4,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    );
    write_pdf_object(
        &mut out,
        &mut offsets,
        5,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>",
    );
    offsets[6] = out.len();
    out.extend_from_slice(
        format!("6 0 obj\n<< /Length {} >>\nstream\n", content_bytes.len()).as_bytes(),
    );
    out.extend_from_slice(content_bytes);
    out.extend_from_slice(b"endstream\nendobj\n");
    write_pdf_object(
        &mut out,
        &mut offsets,
        7,
        &format!("<< /Title ({escaped_title}) /Producer (ZANPOS) >>"),
    );

    let xref_start = out.len();
    out.extend_from_slice(b"xref\n0 8\n0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size 8 /Root 1 0 R /Info 7 0 R >>\nstartxref\n{xref_start}\n%%EOF\n")
            .as_bytes(),
    );
    out
}

// ─── PDF generator ────────────────────────────────────────────────────────────

/// Build an A5 portrait PDF receipt and return the raw bytes.
pub fn generate_receipt_pdf(inp: &WhatsAppReceiptPdfInput) -> Result<Vec<u8>, String> {
    let title = format!("Receipt {}", inp.receipt_number);
    let mut layer = ReceiptLayer::default();
    let font = ReceiptFont::Regular;
    let font_b = ReceiptFont::Bold;

    let mx: f32 = 10.0; // left margin
    let rx: f32 = 108.0; // right column start (amounts)
    let qx: f32 = 82.0; // qty column
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

    layer.use_text(
        "--------------------------------------------------",
        6.0,
        Mm(mx),
        Mm(y),
        &font,
    );
    y -= 4.5;

    // ── RECEIPT header ──────────────────────────────────────────────────────
    layer.use_text("RECEIPT", 13.0, Mm(mx), Mm(y), &font_b);
    y -= 6.0;

    layer.use_text(
        format!("No:      {}", ascii(&inp.receipt_number)),
        8.5,
        Mm(mx),
        Mm(y),
        &font,
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
    layer.use_text(format!("Date:    {}", date_str), 8.5, Mm(mx), Mm(y), &font);
    y -= 5.0;

    layer.use_text(
        format!("Cashier: {}", ascii(&inp.cashier_name)),
        8.5,
        Mm(mx),
        Mm(y),
        &font,
    );
    y -= 5.0;

    // ── Tax / CR numbers (shown only when non-empty) ────────────────────────
    if let Some(tn) = &inp.tax_number {
        if !tn.is_empty() {
            layer.use_text(format!("Tax ID:  {}", ascii(tn)), 8.5, Mm(mx), Mm(y), &font);
            y -= 5.0;
        }
    }
    if let Some(cr) = &inp.cr_number {
        if !cr.is_empty() {
            layer.use_text(format!("CR No:   {}", ascii(cr)), 8.5, Mm(mx), Mm(y), &font);
            y -= 5.0;
        }
    }

    layer.use_text(
        "--------------------------------------------------",
        6.0,
        Mm(mx),
        Mm(y),
        &font,
    );
    y -= 5.0;

    // ── Items header ────────────────────────────────────────────────────────
    layer.use_text("Item", 8.5, Mm(mx), Mm(y), &font_b);
    layer.use_text("Qty", 8.5, Mm(qx), Mm(y), &font_b);
    layer.use_text("Amount", 8.5, Mm(rx), Mm(y), &font_b);
    y -= 4.5;
    layer.use_text(
        "--------------------------------------------------",
        6.0,
        Mm(mx),
        Mm(y),
        &font,
    );
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
            8.5,
            Mm(rx),
            Mm(y),
            &font,
        );
        y -= 5.0;
    }

    // ── Totals ──────────────────────────────────────────────────────────────
    layer.use_text(
        "--------------------------------------------------",
        6.0,
        Mm(mx),
        Mm(y),
        &font,
    );
    y -= 5.0;

    if inp.discount_total_minor > 0 {
        let gross = inp.net_total_minor + inp.discount_total_minor;
        layer.use_text("Subtotal:", 8.5, Mm(mx), Mm(y), &font);
        layer.use_text(
            format!("{} {}", cur, fmt_money(gross, exp)),
            8.5,
            Mm(rx),
            Mm(y),
            &font,
        );
        y -= 5.0;

        layer.use_text("Discount:", 8.5, Mm(mx), Mm(y), &font);
        layer.use_text(
            format!("- {} {}", cur, fmt_money(inp.discount_total_minor, exp)),
            8.5,
            Mm(rx),
            Mm(y),
            &font,
        );
        y -= 5.0;
    }

    if inp.tax_total_minor > 0 {
        layer.use_text("Tax:", 8.5, Mm(mx), Mm(y), &font);
        layer.use_text(
            format!("{} {}", cur, fmt_money(inp.tax_total_minor, exp)),
            8.5,
            Mm(rx),
            Mm(y),
            &font,
        );
        y -= 5.0;
    }

    layer.use_text("TOTAL:", 11.0, Mm(mx), Mm(y), &font_b);
    layer.use_text(
        format!("{} {}", cur, fmt_money(inp.net_total_minor, exp)),
        11.0,
        Mm(rx),
        Mm(y),
        &font_b,
    );
    y -= 7.0;

    // ── Payments ────────────────────────────────────────────────────────────
    layer.use_text(
        "--------------------------------------------------",
        6.0,
        Mm(mx),
        Mm(y),
        &font,
    );
    y -= 5.0;

    for payment in &inp.payments {
        if y < 22.0 {
            break;
        }
        let label = match payment.method.as_str() {
            "cash" => "Cash",
            "card" => "Card",
            "wallet" => "Wallet",
            _ => "Payment",
        };
        layer.use_text(
            format!(
                "{}: {} {}",
                label,
                cur,
                fmt_money(payment.amount_minor, exp)
            ),
            8.5,
            Mm(mx),
            Mm(y),
            &font,
        );
        y -= 5.0;

        // Change line (cash payments)
        if let Some(change) = payment.change_minor {
            if change > 0 && y >= 22.0 {
                layer.use_text(
                    format!("  Change: {} {}", cur, fmt_money(change, exp)),
                    8.5,
                    Mm(mx),
                    Mm(y),
                    &font,
                );
                y -= 5.0;
            }
        }
    }

    // ── Delivery address ────────────────────────────────────────────────────
    let addr = inp.address_text.as_deref().unwrap_or("");
    if !addr.is_empty() && y > 30.0 {
        layer.use_text(
            "--------------------------------------------------",
            6.0,
            Mm(mx),
            Mm(y),
            &font,
        );
        y -= 4.5;
        layer.use_text("Delivery Address:", 8.5, Mm(mx), Mm(y), &font_b);
        y -= 5.0;

        if y > 22.0 {
            layer.use_text(ascii(addr), 8.5, Mm(mx), Mm(y), &font);
            y -= 5.0;
        }
        if let Some(h) = &inp.house_number {
            if !h.is_empty() && y > 22.0 {
                layer.use_text(
                    format!("Bldg/House: {}", ascii(h)),
                    8.5,
                    Mm(mx),
                    Mm(y),
                    &font,
                );
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
        layer.use_text(
            "--------------------------------------------------",
            6.0,
            Mm(mx),
            Mm(y),
            &font,
        );
        y -= 5.0;
    }
    if y > 14.0 {
        layer.use_text("Thank you for your purchase!", 9.0, Mm(mx), Mm(y), &font);
    }

    // ── Serialize ───────────────────────────────────────────────────────────
    Ok(build_text_pdf(&title, 148.0, 210.0, &layer.into_stream()))
}
