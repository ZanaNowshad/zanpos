use crate::ai::{tool_validators, tools};
use crate::errors::{AppError, AppResult};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Read,
    Mutation,
}

impl ToolKind {
    pub fn is_mutation(self) -> bool {
        self == Self::Mutation
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmation {
    Never,
    AutomaticIfActionUndo,
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustLevel {
    Internal,
    External,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataScope {
    GlobalRead,
    BranchRead,
    BranchMutation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoPolicy {
    None,
    Action,
    Run,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionPath {
    Read,
    Action,
    Run,
}

impl ExecutionPath {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Action => "action",
            Self::Run => "run",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequiredRole {
    Cashier,
    Manager,
}

impl RequiredRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cashier => "cashier",
            Self::Manager => "manager",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl RiskLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

const CASHIER_READ_TOOLS: &[&str] = &[
    // Draws boxes and returns; it can no more change data than a question in
    // prose can, and a cashier asking the till widget for something needs the
    // same "which one did you mean?" affordance a manager gets.
    "request_input",
    "lookup_barcode",
    "search_products",
    "get_product",
    "get_product_detail",
    "get_stock_levels",
    "get_active_shift",
    "get_sync_status",
    "get_whatsapp_status",
];

fn required_role(name: &str, kind: ToolKind) -> RequiredRole {
    if kind == ToolKind::Read && CASHIER_READ_TOOLS.contains(&name) {
        RequiredRole::Cashier
    } else {
        RequiredRole::Manager
    }
}

fn risk_level(name: &str, kind: ToolKind, trust: TrustLevel, undo: UndoPolicy) -> RiskLevel {
    if kind == ToolKind::Read {
        return if trust == TrustLevel::External {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        };
    }
    if matches!(
        name,
        "create_refund"
            | "create_cash_event"
            | "adjust_stock"
            | "bulk_stock_set"
            | "send_whatsapp_delivery_alert"
            | "send_whatsapp_payment_reminder"
            | "send_whatsapp_arrival_notice"
            | "send_whatsapp_to_customer"
            | "send_receipt_via_whatsapp"
            | "delete_product"
            | "delete_customer"
            | "delete_user"
            | "force_close_shift"
            | "void_sale"
    ) {
        RiskLevel::Critical
    } else if undo == UndoPolicy::None {
        RiskLevel::High
    } else {
        RiskLevel::Medium
    }
}

fn undo_policy(name: &str) -> UndoPolicy {
    match name {
        "bulk_price_adjust" | "bulk_stock_set" => UndoPolicy::Run,
        "create_product"
        | "product_create"
        | "update_product_price"
        | "set_product_active"
        | "update_product_name"
        | "adjust_stock"
        | "stock_take"
        | "update_reorder_point"
        | "create_customer"
        | "update_customer"
        | "advance_delivery_status"
        | "bulk_stock_take"
        | "create_category"
        | "update_category"
        | "create_user"
        | "update_user"
        | "create_tax_rule"
        | "update_tax_rule"
        | "update_product_full"
        | "update_store_settings"
        | "confirm_delivery_payment"
        | "cancel_delivery"
        | "create_device"
        | "set_device_active"
        | "receive_stock"
        | "add_loyalty_points"
        | "bulk_update_prices" => UndoPolicy::Action,
        _ => UndoPolicy::None,
    }
}

fn routine_reversible_mutation(name: &str) -> bool {
    matches!(
        name,
        "update_product_price"
            | "update_product_name"
            | "set_product_active"
            | "create_category"
            | "update_category"
            | "create_customer"
            | "update_customer"
            | "create_supplier"
            | "update_supplier"
            | "add_product_barcode"
            | "remove_product_barcode"
            | "create_customer_note"
    )
}

#[derive(Debug, Clone)]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub kind: ToolKind,
    pub confirmation: Confirmation,
    pub feature_key: Option<&'static str>,
    pub required_role: RequiredRole,
    pub risk: RiskLevel,
    pub execution: ExecutionPath,
    #[allow(dead_code)]
    pub trust: TrustLevel,
    #[allow(dead_code)]
    pub scope: DataScope,
    #[allow(dead_code)]
    pub undo: UndoPolicy,
    pub(crate) schema: Value,
}

impl ToolDescriptor {
    pub fn validate(&self, input: &Value) -> AppResult<()> {
        tool_validators::validate_schema(input, &self.schema)
    }
}

pub struct ToolRegistry {
    ordered: Vec<ToolDescriptor>,
    by_name: HashMap<String, usize>,
}

static TOOL_REGISTRY: OnceLock<ToolRegistry> = OnceLock::new();

fn canonical_tool_name(name: &str) -> &str {
    match name {
        "product_create" => "create_product",
        _ => name,
    }
}

impl ToolRegistry {
    pub fn build() -> AppResult<Self> {
        let definitions = crate::ai::tools_catalogue::all_tool_definitions();
        Self::from_definitions(definitions)
    }

    pub fn global() -> AppResult<&'static Self> {
        if let Some(registry) = TOOL_REGISTRY.get() {
            return Ok(registry);
        }
        let validated = Self::build()?;
        Ok(TOOL_REGISTRY.get_or_init(|| validated))
    }

    fn from_definitions(definitions: Vec<crate::ai::client::ToolDef>) -> AppResult<Self> {
        let mut ordered = Vec::with_capacity(definitions.len());
        let mut by_name = HashMap::with_capacity(definitions.len());
        for definition in definitions {
            let bad = definition
                .name
                .bytes()
                .any(|b| !b.is_ascii_alphanumeric() && !matches!(b, b'_' | b'-'));
            if definition.name.is_empty() || bad {
                return Err(AppError::Validation(format!(
                    "AI tool name contains unsupported characters: {}",
                    definition.name
                )));
            }
            if by_name.contains_key(&definition.name) {
                return Err(AppError::Validation(format!(
                    "Duplicate AI tool definition: {}",
                    definition.name
                )));
            }
            let kind = if tools::is_mutation_tool(&definition.name) {
                ToolKind::Mutation
            } else {
                ToolKind::Read
            };
            let trust = if crate::ai::tool_policy::is_external_content_tool(&definition.name) {
                TrustLevel::External
            } else {
                TrustLevel::Internal
            };
            let undo = if kind.is_mutation() {
                undo_policy(&definition.name)
            } else {
                UndoPolicy::None
            };
            let execution = if crate::ai::engine::ops::is_registered_operation(&definition.name) {
                ExecutionPath::Run
            } else if kind.is_mutation() {
                ExecutionPath::Action
            } else {
                ExecutionPath::Read
            };
            let descriptor = ToolDescriptor {
                feature_key: tools::feature_key_for_tool(&definition.name),
                confirmation: if kind.is_mutation()
                    && routine_reversible_mutation(&definition.name)
                    && undo_policy(&definition.name) == UndoPolicy::Action
                {
                    Confirmation::AutomaticIfActionUndo
                } else if kind.is_mutation() {
                    Confirmation::Always
                } else {
                    Confirmation::Never
                },
                name: definition.name.clone(),
                description: definition.description,
                kind,
                required_role: required_role(&definition.name, kind),
                risk: risk_level(&definition.name, kind, trust, undo),
                execution,
                trust,
                scope: if kind.is_mutation() {
                    DataScope::BranchMutation
                } else {
                    DataScope::BranchRead
                },
                undo,
                schema: definition.input_schema,
            };
            by_name.insert(definition.name, ordered.len());
            ordered.push(descriptor);
        }
        for mutation in tools::MUTATION_TOOLS {
            if !by_name.contains_key(canonical_tool_name(mutation)) {
                return Err(AppError::Validation(format!(
                    "Mutation tool has no canonical definition: {mutation}"
                )));
            }
        }
        Ok(Self { ordered, by_name })
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.ordered.len()
    }

    pub fn get(&self, name: &str) -> Option<&ToolDescriptor> {
        self.by_name
            .get(canonical_tool_name(name))
            .map(|index| &self.ordered[*index])
    }

    pub fn iter(&self) -> impl Iterator<Item = &ToolDescriptor> {
        self.ordered.iter()
    }
}

#[cfg(test)]
mod tests;
