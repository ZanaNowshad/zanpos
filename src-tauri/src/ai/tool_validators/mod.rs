use crate::errors::{AppError, AppResult};
use serde_json::{Map, Value};

const MAX_PAYLOAD_BYTES: usize = 64 * 1024;
const MAX_DEPTH: usize = 8;
const MAX_OBJECT_KEYS: usize = 64;
const MAX_ARRAY_ITEMS: usize = 1_000;
const MAX_STRING_CHARS: usize = 4_096;

pub fn validate_global(input: &Value) -> AppResult<()> {
    let object = input
        .as_object()
        .ok_or_else(|| AppError::Validation("Tool input must be a JSON object".into()))?;
    if serde_json::to_vec(input)
        .map_err(|e| AppError::Validation(format!("Invalid tool input: {e}")))?
        .len()
        > MAX_PAYLOAD_BYTES
    {
        return Err(AppError::Validation("Tool input exceeds 64 KiB".into()));
    }
    validate_value_boundaries(input, 0)?;
    if object.len() > MAX_OBJECT_KEYS {
        return Err(AppError::Validation(format!(
            "Tool input has too many fields (max {MAX_OBJECT_KEYS})"
        )));
    }
    Ok(())
}

pub fn validate_schema(input: &Value, schema: &Value) -> AppResult<()> {
    validate_global(input)?;
    validate_against_schema(input, schema, "input")
}

fn validate_value_boundaries(value: &Value, depth: usize) -> AppResult<()> {
    if depth > MAX_DEPTH {
        return Err(AppError::Validation(format!(
            "Tool input nesting exceeds {MAX_DEPTH} levels"
        )));
    }
    match value {
        Value::String(s) if s.chars().count() > MAX_STRING_CHARS => Err(AppError::Validation(
            format!("Tool input string exceeds {MAX_STRING_CHARS} characters"),
        )),
        Value::Array(values) => {
            if values.len() > MAX_ARRAY_ITEMS {
                return Err(AppError::Validation(format!(
                    "Tool input array exceeds {MAX_ARRAY_ITEMS} items"
                )));
            }
            for value in values {
                validate_value_boundaries(value, depth + 1)?;
            }
            Ok(())
        }
        Value::Object(values) => {
            if values.len() > MAX_OBJECT_KEYS {
                return Err(AppError::Validation(format!(
                    "Tool input object exceeds {MAX_OBJECT_KEYS} fields"
                )));
            }
            for (key, value) in values {
                if key.chars().count() > 128 {
                    return Err(AppError::Validation(
                        "Tool input field name exceeds 128 characters".into(),
                    ));
                }
                validate_value_boundaries(value, depth + 1)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn validate_against_schema(value: &Value, schema: &Value, path: &str) -> AppResult<()> {
    if let Some(choices) = schema.get("enum").and_then(Value::as_array) {
        if !choices.contains(value) {
            return invalid(path, "value is not in the allowed enum");
        }
    }

    if let Some(types) = schema.get("type") {
        let valid = match types {
            Value::String(kind) => matches_type(value, kind),
            Value::Array(kinds) => kinds
                .iter()
                .filter_map(Value::as_str)
                .any(|kind| matches_type(value, kind)),
            _ => false,
        };
        if !valid {
            return invalid(path, "value has the wrong JSON type");
        }
    }

    if let Some(text) = value.as_str() {
        let min = schema.get("minLength").and_then(Value::as_u64).unwrap_or(0) as usize;
        let max = schema
            .get("maxLength")
            .and_then(Value::as_u64)
            .map(|v| v as usize)
            .unwrap_or(MAX_STRING_CHARS)
            .min(MAX_STRING_CHARS);
        let len = text.chars().count();
        if len < min || len > max {
            return invalid(path, &format!("string length must be {min}..={max}"));
        }
        if let Some(format) = schema.get("format").and_then(Value::as_str) {
            let valid = match format {
                "email" => text.split_once('@').is_some_and(|(local, domain)| {
                    !local.is_empty() && domain.contains('.') && !domain.contains(' ')
                }),
                "uri" => reqwest::Url::parse(text).is_ok(),
                "date-time" => chrono::DateTime::parse_from_rfc3339(text).is_ok(),
                "date" => chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d").is_ok(),
                _ => false,
            };
            if !valid {
                return invalid(path, &format!("invalid {format} format"));
            }
        }
    }

    if let Some(number) = value.as_f64() {
        if schema
            .get("minimum")
            .and_then(Value::as_f64)
            .is_some_and(|min| number < min)
            || schema
                .get("maximum")
                .and_then(Value::as_f64)
                .is_some_and(|max| number > max)
        {
            return invalid(path, "number is outside the allowed range");
        }
    }

    if let Some(object) = value.as_object() {
        validate_object(object, schema, path)?;
    }
    if let Some(array) = value.as_array() {
        let min = schema.get("minItems").and_then(Value::as_u64).unwrap_or(0) as usize;
        let max = schema
            .get("maxItems")
            .and_then(Value::as_u64)
            .map(|v| v as usize)
            .unwrap_or(MAX_ARRAY_ITEMS)
            .min(MAX_ARRAY_ITEMS);
        if array.len() < min || array.len() > max {
            return invalid(path, &format!("array length must be {min}..={max}"));
        }
        if let Some(items) = schema.get("items") {
            for (index, item) in array.iter().enumerate() {
                validate_against_schema(item, items, &format!("{path}[{index}]"))?;
            }
        }
    }
    Ok(())
}

fn validate_object(object: &Map<String, Value>, schema: &Value, path: &str) -> AppResult<()> {
    let declared_properties = schema.get("properties").and_then(Value::as_object);
    let properties = declared_properties.cloned().unwrap_or_default();
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for field in required.iter().filter_map(Value::as_str) {
            let Some(value) = object.get(field) else {
                return invalid(path, &format!("missing required field '{field}'"));
            };
            if value.as_str().is_some_and(|value| value.trim().is_empty()) {
                return invalid(path, &format!("required field '{field}' cannot be empty"));
            }
        }
    }
    let allow_extra = schema
        .get("additionalProperties")
        .and_then(Value::as_bool)
        .unwrap_or(declared_properties.is_none());
    for (field, value) in object {
        validate_named_field(field, value, path)?;
        let Some(field_schema) = properties.get(field) else {
            if allow_extra {
                continue;
            }
            return invalid(path, &format!("unknown field '{field}'"));
        };
        validate_against_schema(value, field_schema, &format!("{path}.{field}"))?;
    }
    Ok(())
}

fn validate_named_field(field: &str, value: &Value, path: &str) -> AppResult<()> {
    if let Some(text) = value.as_str() {
        let limit = if field == "pin" || field.ends_with("_pin") {
            64
        } else if field == "email" {
            320
        } else if field == "notes" || field == "description" || field == "reason" {
            2_000
        } else if field.ends_with("_id") || field == "barcode" || field == "sku" {
            128
        } else if field == "name" || field.ends_with("_name") {
            200
        } else {
            MAX_STRING_CHARS
        };
        if text.chars().count() > limit {
            return invalid(path, &format!("field '{field}' exceeds {limit} characters"));
        }
    }
    if value
        .as_f64()
        .is_some_and(|number| !number.is_finite() || number.abs() > 1_000_000_000_000_f64)
    {
        return invalid(
            path,
            &format!("field '{field}' exceeds the numeric boundary"),
        );
    }
    if let Some(number) = value.as_f64() {
        let money_like =
            field.contains("price") || field.contains("cost") || field.contains("amount");
        let signed =
            field.contains("delta") || field.contains("change") || field.contains("adjustment");
        if money_like && !signed && number < 0.0 {
            return invalid(path, &format!("field '{field}' cannot be negative"));
        }
        if (field.contains("quantity") || field.ends_with("_qty")) && number.abs() > 10_000_000.0 {
            return invalid(
                path,
                &format!("field '{field}' exceeds the quantity boundary"),
            );
        }
    }
    Ok(())
}

fn matches_type(value: &Value, kind: &str) -> bool {
    match kind {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => false,
    }
}

fn invalid<T>(path: &str, message: &str) -> AppResult<T> {
    Err(AppError::Validation(format!("Invalid {path}: {message}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_boundary_rejects_deep_or_non_object_payloads() {
        assert!(validate_global(&serde_json::json!([1, 2])).is_err());
        let deep = serde_json::json!({"a":{"b":{"c":{"d":{"e":{"f":{"g":{"h":{"i":1}}}}}}}}});
        assert!(validate_global(&deep).is_err());
    }

    #[test]
    fn semantic_boundaries_reject_negative_money_and_oversized_arrays() {
        assert!(validate_schema(
            &serde_json::json!({"price_minor":-1}),
            &serde_json::json!({"type":"object","properties":{"price_minor":{"type":"integer"}}})
        )
        .is_err());
        assert!(validate_schema(&serde_json::json!({"lines":[1,2]}), &serde_json::json!({"type":"object","properties":{"lines":{"type":"array","maxItems":1}}})).is_err());
    }
}
