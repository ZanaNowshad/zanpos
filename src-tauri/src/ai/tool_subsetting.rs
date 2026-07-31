use crate::ai::client::ToolDef;
use crate::ai::tool_registry::{ToolKind, ToolRegistry};
use crate::errors::{AppError, AppResult};
use sqlx::{Row, SqlitePool};

pub struct ToolSubset {
    pub definitions: Vec<ToolDef>,
    pub applied: bool,
    pub omitted_mutations: usize,
    pub domains: Vec<&'static str>,
}

pub fn subset_for_message(
    definitions: &[ToolDef],
    message: &str,
    enabled: bool,
    widened: bool,
) -> AppResult<ToolSubset> {
    if !enabled || widened {
        return Ok(full_catalogue(definitions));
    }

    let domains = detect_domains(message);
    if domains.is_empty() {
        return Ok(full_catalogue(definitions));
    }

    let registry = ToolRegistry::global()?;
    let mut selected = Vec::with_capacity(definitions.len());
    let mut omitted_mutations = 0;
    for definition in definitions {
        let descriptor = registry.get(&definition.name).ok_or_else(|| {
            AppError::Validation(format!("Missing policy descriptor for {}", definition.name))
        })?;
        let keep = descriptor.kind == ToolKind::Read
            || mutation_domain(&definition.name).is_none_or(|domain| domains.contains(&domain));
        if keep {
            selected.push(definition.clone());
        } else {
            omitted_mutations += 1;
        }
    }

    Ok(ToolSubset {
        applied: omitted_mutations > 0,
        definitions: selected,
        omitted_mutations,
        domains,
    })
}

pub async fn load_enabled(pool: &SqlitePool) -> AppResult<bool> {
    let value: Option<String> =
        sqlx::query("SELECT value FROM app_config WHERE key='feature_ai_tool_subsetting'")
            .fetch_optional(pool)
            .await?
            .map(|row| row.get("value"));
    parse_enabled(value.as_deref())
}

fn parse_enabled(value: Option<&str>) -> AppResult<bool> {
    match value {
        None | Some("0" | "false") => Ok(false),
        Some("1" | "true") => Ok(true),
        Some(other) => Err(AppError::Validation(format!(
            "Invalid feature_ai_tool_subsetting value: {other}"
        ))),
    }
}

fn full_catalogue(definitions: &[ToolDef]) -> ToolSubset {
    ToolSubset {
        definitions: definitions.to_vec(),
        applied: false,
        omitted_mutations: 0,
        domains: Vec::new(),
    }
}

fn detect_domains(message: &str) -> Vec<&'static str> {
    const KEYWORDS: &[(&str, &[&str])] = &[
        (
            "products",
            &[
                "product",
                "price",
                "barcode",
                "category",
                "promotion",
                "منتج",
                "سعر",
                "باركود",
                "فئة",
                "عرض",
            ],
        ),
        (
            "inventory",
            &[
                "stock",
                "inventory",
                "reorder",
                "receive",
                "supplier",
                "purchase",
                "مخزون",
                "جرد",
                "استلام",
                "توريد",
                "مورد",
                "شراء",
            ],
        ),
        (
            "customers",
            &[
                "customer",
                "loyalty",
                "delivery",
                "whatsapp",
                "عميل",
                "عملاء",
                "ولاء",
                "توصيل",
                "واتساب",
            ],
        ),
        (
            "users",
            &["user", "role", "cashier", "مستخدم", "دور", "كاشير"],
        ),
        (
            "operations",
            &[
                "sale",
                "refund",
                "shift",
                "cash",
                "drawer",
                "بيع",
                "استرجاع",
                "وردية",
                "نقد",
                "صندوق",
            ],
        ),
        (
            "settings",
            &[
                "setting",
                "sync",
                "backup",
                "device",
                "terminal",
                "hub",
                "إعداد",
                "مزامنة",
                "نسخ احتياطي",
                "جهاز",
                "طرفية",
            ],
        ),
    ];
    let normalized = message.to_lowercase();
    KEYWORDS
        .iter()
        .filter_map(|(domain, keywords)| {
            keywords
                .iter()
                .any(|keyword| normalized.contains(keyword))
                .then_some(*domain)
        })
        .collect()
}

fn mutation_domain(name: &str) -> Option<&'static str> {
    if contains_any(
        name,
        &[
            "product",
            "category",
            "barcode",
            "price",
            "promotion",
            "discount",
        ],
    ) {
        Some("products")
    } else if contains_any(
        name,
        &["stock", "inventory", "supplier", "purchase", "reorder"],
    ) || name == "receive_stock"
    {
        Some("inventory")
    } else if contains_any(
        name,
        &["customer", "loyalty", "delivery", "whatsapp", "rider"],
    ) {
        Some("customers")
    } else if contains_any(name, &["user", "role", "permission"]) {
        Some("users")
    } else if contains_any(
        name,
        &["sale", "refund", "shift", "cash", "drawer", "tender"],
    ) {
        Some("operations")
    } else if contains_any(
        name,
        &[
            "setting", "sync", "backup", "device", "terminal", "hub", "printer",
        ],
    ) {
        Some("settings")
    } else {
        None
    }
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definitions(names: &[&str]) -> Vec<ToolDef> {
        let catalogue = crate::ai::tools::all_tool_definitions();
        names
            .iter()
            .map(|name| {
                catalogue
                    .iter()
                    .find(|definition| definition.name == *name)
                    .unwrap_or_else(|| panic!("missing test definition {name}"))
                    .clone()
            })
            .collect()
    }

    #[test]
    fn disabled_and_widened_requests_keep_the_full_catalogue() {
        let definitions = definitions(&["get_today_summary", "create_product", "create_refund"]);

        let disabled = subset_for_message(&definitions, "refund a sale", false, false).unwrap();
        let widened = subset_for_message(&definitions, "refund a sale", true, true).unwrap();

        assert_eq!(disabled.definitions.len(), definitions.len());
        assert_eq!(widened.definitions.len(), definitions.len());
        assert!(!disabled.applied);
        assert!(!widened.applied);
    }

    #[test]
    fn clear_sales_intent_keeps_all_reads_and_only_relevant_classified_mutations() {
        let definitions = definitions(&[
            "get_today_summary",
            "list_products",
            "create_refund",
            "create_product",
        ]);

        let subset =
            subset_for_message(&definitions, "Refund yesterday's sale", true, false).unwrap();
        let names: Vec<_> = subset
            .definitions
            .iter()
            .map(|definition| definition.name.as_str())
            .collect();

        assert!(subset.applied);
        assert!(names.contains(&"get_today_summary"));
        assert!(names.contains(&"list_products"));
        assert!(names.contains(&"create_refund"));
        assert!(!names.contains(&"create_product"));
    }

    #[test]
    fn arabic_inventory_intent_is_detected() {
        let definitions = definitions(&["get_today_summary", "receive_stock", "create_customer"]);

        let subset =
            subset_for_message(&definitions, "استلام المخزون من المورد", true, false).unwrap();
        let names: Vec<_> = subset
            .definitions
            .iter()
            .map(|definition| definition.name.as_str())
            .collect();

        assert!(names.contains(&"receive_stock"));
        assert!(!names.contains(&"create_customer"));
        assert_eq!(subset.domains, vec!["inventory"]);
    }

    #[test]
    fn ambiguous_messages_keep_the_full_catalogue() {
        let definitions = definitions(&["get_today_summary", "create_product", "create_refund"]);

        let subset =
            subset_for_message(&definitions, "Help me manage the shop", true, false).unwrap();

        assert!(!subset.applied);
        assert_eq!(subset.definitions.len(), definitions.len());
    }

    #[test]
    fn subsetting_flag_defaults_off_and_rejects_invalid_values() {
        assert!(!parse_enabled(None).unwrap());
        assert!(!parse_enabled(Some("false")).unwrap());
        assert!(parse_enabled(Some("1")).unwrap());
        assert!(parse_enabled(Some("enabled")).is_err());
    }
}
