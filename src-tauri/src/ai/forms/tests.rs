use super::*;
use serde_json::json;

fn price_form() -> Value {
    json!({
        "title": "Update price",
        "fields": [
            { "name": "barcode", "label": "Product barcode", "type": "barcode", "required": true },
            { "name": "new_price", "label": "New price (BHD)", "type": "money", "required": true }
        ]
    })
}

#[test]
fn a_two_field_price_form_parses_into_the_shape_the_widget_renders() {
    let form = parse_form_spec(&price_form()).unwrap();

    assert_eq!(form.title, "Update price");
    assert_eq!(form.fields.len(), 2);
    assert_eq!(form.fields[0].kind, AiFieldKind::Barcode);
    assert_eq!(form.fields[1].kind, AiFieldKind::Money);
    assert!(form.fields[1].required);
    // Absent optional text is empty, never null — the widget renders it directly.
    assert_eq!(form.fields[0].placeholder, "");
    assert!(form.table.is_none());
}

#[test]
fn an_empty_form_is_rejected_because_there_is_nothing_to_submit() {
    let empty = json!({ "title": "Hmm" });
    assert!(parse_form_spec(&empty).is_err());
}

#[test]
fn a_field_name_that_could_not_survive_the_answer_round_trip_is_rejected() {
    // The answer travels back as "name: value" lines, so a name containing a
    // colon, a space or a newline would arrive as something else entirely.
    for bad in ["new price", "new:price", "new\nprice", "new-price"] {
        let spec = json!({
            "title": "T",
            "fields": [{ "name": bad, "label": "L", "type": "text" }]
        });
        assert!(
            parse_form_spec(&spec).is_err(),
            "accepted unusable field name {bad:?}"
        );
    }
}

#[test]
fn duplicate_names_are_rejected_in_fields_and_in_columns() {
    let fields = json!({
        "title": "T",
        "fields": [
            { "name": "qty", "label": "A", "type": "number" },
            { "name": "QTY", "label": "B", "type": "number" }
        ]
    });
    assert!(parse_form_spec(&fields).is_err());

    let columns = json!({
        "title": "T",
        "table": {
            "columns": [
                { "name": "qty", "label": "A", "type": "number" },
                { "name": "qty", "label": "B", "type": "number" }
            ],
            "rows": []
        }
    });
    assert!(parse_form_spec(&columns).is_err());
}

#[test]
fn a_select_without_options_is_rejected_rather_than_drawn_empty() {
    let field = json!({
        "title": "T",
        "fields": [{ "name": "unit", "label": "Unit", "type": "select" }]
    });
    assert!(parse_form_spec(&field).is_err());

    let column = json!({
        "title": "T",
        "table": {
            "columns": [{ "name": "unit", "label": "Unit", "type": "select" }],
            "rows": []
        }
    });
    assert!(parse_form_spec(&column).is_err());
}

#[test]
fn a_purchase_bill_table_keeps_its_rows_and_normalises_every_cell_to_text() {
    let spec = json!({
        "title": "Purchase bill — Al Noor Trading",
        "note": "Check the two lines with no barcode.",
        "table": {
            "columns": [
                { "name": "barcode", "label": "Barcode", "type": "barcode", "required": true },
                { "name": "name", "label": "Product", "type": "text", "required": true },
                { "name": "qty", "label": "Qty", "type": "integer", "required": true },
                { "name": "unit_cost", "label": "Unit cost", "type": "money", "required": true }
            ],
            "rows": [
                { "barcode": "6281234567890", "name": "Nadec Laban 1L", "qty": 24, "unit_cost": 0.42 },
                { "barcode": "", "name": "Local Dates 500g", "qty": 6, "unit_cost": 1.25 }
            ],
            "allow_add": true,
            "allow_remove": true
        }
    });

    let form = parse_form_spec(&spec).unwrap();
    let table = form.table.unwrap();

    assert_eq!(table.columns.len(), 4);
    assert_eq!(table.rows.len(), 2);
    assert!(table.allow_add && table.allow_remove);
    // Numbers arrive as JSON numbers and leave as the text the operator edits.
    assert_eq!(table.rows[0]["qty"], "24");
    assert_eq!(table.rows[0]["unit_cost"], "0.42");
    // A missing barcode stays empty so the required-cell rule can flag it.
    assert_eq!(table.rows[1]["barcode"], "");
}

/// The whole point of the bill table is correcting what OCR got wrong. A cell
/// under a column the operator cannot see is a cell they cannot correct, and it
/// would never come back in the answer — so the model has to be told, not
/// quietly ignored.
#[test]
fn a_cell_with_no_matching_column_is_rejected_not_dropped() {
    let spec = json!({
        "title": "Bill",
        "table": {
            "columns": [{ "name": "name", "label": "Product", "type": "text" }],
            "rows": [{ "name": "Milk", "vat_rate": "10%" }]
        }
    });
    let error = parse_form_spec(&spec).unwrap_err().to_string();
    assert!(error.contains("vat_rate"), "unhelpful error: {error}");
}

#[test]
fn a_cell_holding_a_nested_object_is_rejected() {
    let spec = json!({
        "title": "Bill",
        "table": {
            "columns": [{ "name": "name", "label": "Product", "type": "text" }],
            "rows": [{ "name": { "en": "Milk" } }]
        }
    });
    assert!(parse_form_spec(&spec).is_err());
}

/// The caps exist so the submit button stays on screen on the 1024x768 till.
/// Truncating instead would leave the model believing it had shown rows the
/// operator never saw and could never correct.
#[test]
fn oversized_specs_are_refused_with_a_correctable_message_rather_than_truncated() {
    let many_fields: Vec<Value> = (0..MAX_FIELDS + 1)
        .map(|i| json!({ "name": format!("f{i}"), "label": "L", "type": "text" }))
        .collect();
    let error = parse_form_spec(&json!({ "title": "T", "fields": many_fields }))
        .unwrap_err()
        .to_string();
    assert!(error.contains("limit is 12"), "unhelpful error: {error}");

    let many_rows: Vec<Value> = (0..MAX_ROWS + 1)
        .map(|i| json!({ "name": i.to_string() }))
        .collect();
    assert!(parse_form_spec(&json!({
        "title": "T",
        "table": {
            "columns": [{ "name": "name", "label": "Product", "type": "text" }],
            "rows": many_rows
        }
    }))
    .is_err());
}

#[test]
fn an_unknown_field_type_names_itself_so_the_model_can_correct_it() {
    let spec = json!({
        "title": "T",
        "fields": [{ "name": "when", "label": "When", "type": "datetime-local" }]
    });
    let error = parse_form_spec(&spec).unwrap_err().to_string();
    assert!(error.contains("datetime-local"), "unhelpful error: {error}");
}

#[test]
fn choice_styles_outside_the_supported_set_fall_back_to_default() {
    let spec = json!({
        "title": "Apply to which branches?",
        "choices": [
            { "value": "this", "label": "This branch", "style": "primary" },
            { "value": "all", "label": "All branches", "style": "rainbow" },
            { "value": "none", "label": "Cancel", "style": "danger" }
        ]
    });
    let form = parse_form_spec(&spec).unwrap();
    assert_eq!(form.choices[0].style, "primary");
    assert_eq!(form.choices[1].style, "default");
    assert_eq!(form.choices[2].style, "danger");
}

/// The acknowledgement is the only thing the model reads back, so it has to
/// carry the stop instruction and the field names — not the whole spec, which
/// would double the token cost of every form.
#[test]
fn the_acknowledgement_lists_the_collected_names_and_tells_the_model_to_stop() {
    let form = parse_form_spec(&price_form()).unwrap();
    let ack: Value = serde_json::from_str(&acknowledgement(&form)).unwrap();

    assert_eq!(ack["status"], "awaiting_user_input");
    assert_eq!(ack["collecting"], json!(["barcode", "new_price"]));
    let instruction = ack["instruction"].as_str().unwrap();
    assert!(instruction.contains("turn ends here"));
    assert!(instruction.contains("do not act on assumed values"));
    assert!(!acknowledgement(&form).contains("Product barcode"));
}

#[test]
fn the_acknowledgement_covers_table_columns_too() {
    let form = parse_form_spec(&json!({
        "title": "Bill",
        "table": {
            "columns": [
                { "name": "barcode", "label": "Barcode", "type": "barcode" },
                { "name": "qty", "label": "Qty", "type": "integer" }
            ],
            "rows": []
        }
    }))
    .unwrap();
    let ack: Value = serde_json::from_str(&acknowledgement(&form)).unwrap();
    assert_eq!(ack["collecting"], json!(["barcode", "qty"]));
}

/// The widget parses this payload straight off the Tauri channel, so the field
/// discriminator has to arrive as `type`, not the Rust member name.
#[test]
fn the_serialised_form_matches_the_typescript_contract() {
    let form = parse_form_spec(&price_form()).unwrap();
    let wire = serde_json::to_value(&form).unwrap();

    assert_eq!(wire["fields"][0]["type"], "barcode");
    assert_eq!(wire["fields"][1]["type"], "money");
    assert_eq!(wire["table"], Value::Null);
    assert_eq!(wire["choices"], json!([]));
}
