//! Interactive forms ZanAI can put in the chat instead of asking in prose.
//!
//! "Price update" used to cost four messages: the model asks which product, the
//! manager types a barcode, the model asks for the new price, the manager types
//! a number. On a till between customers that is four keyboard round-trips for
//! two values. A form collects both at once, in labelled boxes, with the numeric
//! keypad already open on the money field.
//!
//! The spec is written by the model, so everything here treats it as untrusted
//! data. Two properties matter and are enforced rather than hoped for:
//!
//! **A form performs no business action.** `request_input` is a read tool. It
//! renders boxes and returns; submitting sends the operator's answers back as an
//! ordinary user message, which then goes through the same resolution, RBAC and
//! confirmation path as anything else the operator types. There is deliberately
//! no route from a form straight into a mutation — that would be a second
//! execution path to secure, and one is enough.
//!
//! **A form fits on the screen it is drawn on.** The till is a 1024x768 panel
//! and the ZanAI widget is a corner of it. A model that emits forty fields or a
//! six-hundred-row table would push the submit button off the bottom, so the
//! caps below are hard errors the model is told to correct, not silent
//! truncation that would hide rows it believed it had shown.

use crate::errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Caps. Chosen against the smallest surface the form can appear on — the POS
/// widget — rather than the full-page assistant, because a form that only works
/// in the back office is a form the shop floor cannot use.
const MAX_FIELDS: usize = 12;
const MAX_CHOICES: usize = 8;
const MAX_COLUMNS: usize = 8;
/// A delivery note longer than this is a bulk import, not a form to hand-check.
const MAX_ROWS: usize = 60;
const MAX_OPTIONS: usize = 20;
const MAX_TITLE: usize = 120;
const MAX_NOTE: usize = 400;
const MAX_LABEL: usize = 60;
const MAX_VALUE: usize = 200;
const MAX_NAME: usize = 48;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiFieldKind {
    Text,
    Textarea,
    Number,
    Money,
    Integer,
    Barcode,
    Select,
    Date,
    Toggle,
}

impl AiFieldKind {
    fn parse(raw: &str) -> AppResult<Self> {
        Ok(match raw {
            "text" => Self::Text,
            "textarea" => Self::Textarea,
            "number" => Self::Number,
            "money" => Self::Money,
            "integer" => Self::Integer,
            "barcode" => Self::Barcode,
            "select" => Self::Select,
            "date" => Self::Date,
            "toggle" => Self::Toggle,
            other => {
                return Err(AppError::Validation(format!(
                    "request_input: unknown field type '{other}'"
                )))
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiFormOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiFormField {
    pub name: String,
    pub label: String,
    #[serde(rename = "type")]
    pub kind: AiFieldKind,
    pub value: String,
    pub placeholder: String,
    pub help: String,
    pub required: bool,
    pub options: Vec<AiFormOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiFormChoice {
    pub value: String,
    pub label: String,
    pub detail: String,
    /// "primary" draws the recommended answer; "danger" marks the destructive
    /// one so a thumb aiming for "cancel" does not land on "delete".
    pub style: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiFormColumn {
    pub name: String,
    pub label: String,
    #[serde(rename = "type")]
    pub kind: AiFieldKind,
    pub required: bool,
    pub options: Vec<AiFormOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiFormTable {
    pub columns: Vec<AiFormColumn>,
    /// Cells are strings whatever the model sent. A form is a text-entry
    /// surface: the operator types into it and the answer travels back as text,
    /// so carrying JSON number/bool types through would only create two
    /// representations of "24" that have to agree.
    pub rows: Vec<BTreeMap<String, String>>,
    pub row_label: String,
    pub allow_add: bool,
    pub allow_remove: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiForm {
    pub title: String,
    pub note: String,
    pub submit_label: String,
    pub fields: Vec<AiFormField>,
    pub choices: Vec<AiFormChoice>,
    pub table: Option<AiFormTable>,
}

/// Validate and normalise a model-authored form spec.
///
/// Rejects rather than repairs: a spec the model got wrong comes back as a
/// recoverable tool error it can fix on the next step, which is more useful
/// than a silently emptied form the operator has to interpret.
pub fn parse_form_spec(input: &Value) -> AppResult<AiForm> {
    let object = input
        .as_object()
        .ok_or_else(|| AppError::Validation("request_input: input must be an object".into()))?;

    let title = required_text(object.get("title"), "title", MAX_TITLE)?;
    let note = optional_text(object.get("note"), "note", MAX_NOTE)?;
    let submit_label = optional_text(object.get("submit_label"), "submit_label", MAX_LABEL)?;

    let fields = parse_fields(object.get("fields"))?;
    let choices = parse_choices(object.get("choices"))?;
    let table = object
        .get("table")
        .filter(|value| !value.is_null())
        .map(parse_table)
        .transpose()?;

    if fields.is_empty() && choices.is_empty() && table.is_none() {
        return Err(AppError::Validation(
            "request_input: a form needs at least one of fields, choices or table".into(),
        ));
    }

    Ok(AiForm {
        title,
        note,
        submit_label,
        fields,
        choices,
        table,
    })
}

fn parse_fields(raw: Option<&Value>) -> AppResult<Vec<AiFormField>> {
    let items = array_of("fields", raw, MAX_FIELDS)?;
    let mut fields = Vec::with_capacity(items.len());
    let mut seen = Vec::with_capacity(items.len());
    for item in items {
        let object = item.as_object().ok_or_else(|| {
            AppError::Validation("request_input: each field must be an object".into())
        })?;
        let name = parse_name(object.get("name"), &mut seen, "field")?;
        let kind = AiFieldKind::parse(&required_text(object.get("type"), "field type", 32)?)?;
        let options = parse_options(object.get("options"))?;
        if kind == AiFieldKind::Select && options.is_empty() {
            return Err(AppError::Validation(format!(
                "request_input: select field '{name}' needs options"
            )));
        }
        fields.push(AiFormField {
            label: required_text(object.get("label"), "field label", MAX_LABEL)?,
            value: optional_text(object.get("value"), "field value", MAX_VALUE)?,
            placeholder: optional_text(object.get("placeholder"), "placeholder", MAX_LABEL)?,
            help: optional_text(object.get("help"), "help", MAX_LABEL)?,
            required: object
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            name,
            kind,
            options,
        });
    }
    Ok(fields)
}

fn parse_choices(raw: Option<&Value>) -> AppResult<Vec<AiFormChoice>> {
    let items = array_of("choices", raw, MAX_CHOICES)?;
    let mut choices = Vec::with_capacity(items.len());
    for item in items {
        let object = item.as_object().ok_or_else(|| {
            AppError::Validation("request_input: each choice must be an object".into())
        })?;
        let style = optional_text(object.get("style"), "choice style", 16)?;
        choices.push(AiFormChoice {
            value: required_text(object.get("value"), "choice value", MAX_VALUE)?,
            label: required_text(object.get("label"), "choice label", MAX_LABEL)?,
            detail: optional_text(object.get("detail"), "choice detail", MAX_LABEL)?,
            style: match style.as_str() {
                "primary" => "primary".into(),
                "danger" => "danger".into(),
                _ => "default".into(),
            },
        });
    }
    Ok(choices)
}

fn parse_table(raw: &Value) -> AppResult<AiFormTable> {
    let object = raw
        .as_object()
        .ok_or_else(|| AppError::Validation("request_input: table must be an object".into()))?;

    let column_values = array_of("table.columns", object.get("columns"), MAX_COLUMNS)?;
    if column_values.is_empty() {
        return Err(AppError::Validation(
            "request_input: table needs at least one column".into(),
        ));
    }
    let mut columns = Vec::with_capacity(column_values.len());
    let mut seen = Vec::with_capacity(column_values.len());
    for item in column_values {
        let column = item.as_object().ok_or_else(|| {
            AppError::Validation("request_input: each table column must be an object".into())
        })?;
        let name = parse_name(column.get("name"), &mut seen, "column")?;
        let kind = AiFieldKind::parse(&required_text(column.get("type"), "column type", 32)?)?;
        let options = parse_options(column.get("options"))?;
        if kind == AiFieldKind::Select && options.is_empty() {
            return Err(AppError::Validation(format!(
                "request_input: select column '{name}' needs options"
            )));
        }
        columns.push(AiFormColumn {
            label: required_text(column.get("label"), "column label", MAX_LABEL)?,
            required: column
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            name,
            kind,
            options,
        });
    }

    let row_values = array_of("table.rows", object.get("rows"), MAX_ROWS)?;
    let mut rows = Vec::with_capacity(row_values.len());
    for row_value in row_values {
        let row = row_value.as_object().ok_or_else(|| {
            AppError::Validation("request_input: each table row must be an object".into())
        })?;
        let mut cells = BTreeMap::new();
        for (key, value) in row {
            // A cell the operator cannot see is a cell they cannot correct, and
            // the answer only carries declared columns — so an undeclared key is
            // data the model would believe it had shown and never get back.
            if !columns.iter().any(|column| column.name == *key) {
                return Err(AppError::Validation(format!(
                    "request_input: table row has no column named '{key}'"
                )));
            }
            cells.insert(key.clone(), cell_text(value, key)?);
        }
        rows.push(cells);
    }

    Ok(AiFormTable {
        row_label: optional_text(object.get("row_label"), "row_label", MAX_LABEL)?,
        allow_add: object
            .get("allow_add")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        allow_remove: object
            .get("allow_remove")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        columns,
        rows,
    })
}

fn parse_options(raw: Option<&Value>) -> AppResult<Vec<AiFormOption>> {
    let items = array_of("options", raw, MAX_OPTIONS)?;
    items
        .iter()
        .map(|item| {
            let object = item.as_object().ok_or_else(|| {
                AppError::Validation("request_input: each option must be an object".into())
            })?;
            Ok(AiFormOption {
                value: required_text(object.get("value"), "option value", MAX_VALUE)?,
                label: required_text(object.get("label"), "option label", MAX_LABEL)?,
            })
        })
        .collect()
}

fn array_of<'a>(what: &str, raw: Option<&'a Value>, max: usize) -> AppResult<&'a [Value]> {
    let Some(value) = raw.filter(|value| !value.is_null()) else {
        return Ok(&[]);
    };
    let items = value
        .as_array()
        .ok_or_else(|| AppError::Validation(format!("request_input: {what} must be an array")))?;
    if items.len() > max {
        return Err(AppError::Validation(format!(
            "request_input: {what} has {} entries; the limit is {max}. Split the work or ask for the rest afterwards.",
            items.len()
        )));
    }
    Ok(items)
}

/// Machine names carry the answer back to the model, so they have to survive a
/// round-trip through a plain-text message: lowercase, no spaces, no
/// punctuation that could be read as structure.
fn parse_name(raw: Option<&Value>, seen: &mut Vec<String>, what: &str) -> AppResult<String> {
    let name = required_text(raw, &format!("{what} name"), MAX_NAME)?.to_lowercase();
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(AppError::Validation(format!(
            "request_input: {what} name '{name}' may only contain letters, digits and underscores"
        )));
    }
    if seen.contains(&name) {
        return Err(AppError::Validation(format!(
            "request_input: duplicate {what} name '{name}'"
        )));
    }
    seen.push(name.clone());
    Ok(name)
}

fn required_text(raw: Option<&Value>, what: &str, max: usize) -> AppResult<String> {
    let text = raw
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| AppError::Validation(format!("request_input: {what} is required")))?;
    if text.chars().count() > max {
        return Err(AppError::Validation(format!(
            "request_input: {what} exceeds {max} characters"
        )));
    }
    Ok(text.to_string())
}

fn optional_text(raw: Option<&Value>, what: &str, max: usize) -> AppResult<String> {
    match raw.filter(|value| !value.is_null()) {
        None => Ok(String::new()),
        Some(value) => {
            let text = value.as_str().ok_or_else(|| {
                AppError::Validation(format!("request_input: {what} must be a string"))
            })?;
            let text = text.trim();
            if text.chars().count() > max {
                return Err(AppError::Validation(format!(
                    "request_input: {what} exceeds {max} characters"
                )));
            }
            Ok(text.to_string())
        }
    }
}

fn cell_text(value: &Value, key: &str) -> AppResult<String> {
    let text = match value {
        Value::Null => String::new(),
        Value::String(text) => text.trim().to_string(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        _ => {
            return Err(AppError::Validation(format!(
                "request_input: cell '{key}' must be a string, number or boolean"
            )))
        }
    };
    if text.chars().count() > MAX_VALUE {
        return Err(AppError::Validation(format!(
            "request_input: cell '{key}' exceeds {MAX_VALUE} characters"
        )));
    }
    Ok(text)
}

/// What the model is told after the form is on screen.
///
/// The turn ends here whatever this says — the streaming loop stops after a
/// form request precisely so a model that ignores the instruction still cannot
/// act on values it has not been given. The wording exists so the model does not
/// *also* repeat the question in prose above the boxes.
pub fn acknowledgement(form: &AiForm) -> String {
    let mut names: Vec<&str> = form.fields.iter().map(|field| field.name.as_str()).collect();
    if let Some(table) = &form.table {
        names.extend(table.columns.iter().map(|column| column.name.as_str()));
    }
    serde_json::json!({
        "ok": true,
        "status": "awaiting_user_input",
        "collecting": names,
        "instruction": "The form is on screen and your turn ends here. Do not repeat the question in text, \
do not call another tool, and do not act on assumed values. The operator's answers arrive as their next message."
    })
    .to_string()
}

#[cfg(test)]
mod tests;
