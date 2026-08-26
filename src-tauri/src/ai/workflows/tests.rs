//! Workflow tests.
//!
//! Split out to keep `workflows.rs` under the 500-line rule — the same shape
//! `intent_engine` and `tool_registry` already use.

mod behaviour {
    use crate::ai::workflows::*;

    #[test]
    fn typed_barcode_workflows_lookup_external_product_info_before_requesting_details() {
        let ghost = get_workflow("ghost_barcode").unwrap();
        let whatsapp = get_workflow("whatsapp_message").unwrap();
        let typed_whatsapp = &whatsapp[whatsapp.find("TEXT BARCODE").unwrap()..];

        for workflow in [ghost, typed_whatsapp] {
            let local_search = workflow.find("search_products").unwrap();
            let smart_lookup = workflow.find("smart_barcode_lookup").unwrap();
            let fresh_turn = workflow.find("fresh user turn").unwrap();
            let create = workflow.rfind("create_product").unwrap();
            assert!(local_search < smart_lookup);
            assert!(smart_lookup < fresh_turn);
            assert!(fresh_turn < create);
        }
    }

    #[test]
    fn expiry_workflow_delegates_markdown_confirmation_to_runtime_policy() {
        let workflow = get_workflow("expiry_management").unwrap();

        assert!(workflow.contains("report_expiring_stock"));
        assert!(workflow.contains("configured confirmation policy"));
        assert!(!workflow.contains("Never change a price automatically"));
    }

    #[test]
    fn gulf_business_workflows_keep_human_authority_and_dates_explicit() {
        let seasonal = get_workflow("seasonal_demand").unwrap();
        let vat = get_workflow("vat_filing").unwrap();
        let margin = get_workflow("margin_erosion").unwrap();
        let dead = get_workflow("dead_stock_clearance").unwrap();

        assert!(seasonal.contains("Do not infer or hardcode Ramadan dates"));
        assert!(seasonal.contains("A human reviews"));
        assert!(vat.contains("not tax advice"));
        assert!(vat.contains("Do not file"));
        assert!(margin.contains("UNKNOWN COST LINES"));
        assert!(dead.contains("distinct from expiry"));
    }

    #[test]
    fn optional_growth_workflows_remain_advisory_and_policy_governed() {
        assert!(get_workflow("cash_flow_forecast")
            .unwrap()
            .contains("not financial advice"));
        assert!(get_workflow("basket_placement")
            .unwrap()
            .contains("configured confirmation policy"));
        assert!(get_workflow("customer_winback")
            .unwrap()
            .contains("runtime policy enforce outbound-message protection"));
    }

    #[test]
    fn workflow_mutations_delegate_confirmation_to_runtime_policy() {
        let whatsapp = get_workflow("whatsapp_message").unwrap();
        let winback = get_workflow("customer_winback").unwrap();

        assert!(!whatsapp.contains("confirm before executing"));
        assert!(!winback.contains("normal confirmed WhatsApp path"));
        assert!(whatsapp.contains("runtime policy"));
        assert!(winback.contains("runtime policy"));
    }

    #[test]
    fn bulk_workflow_matches_the_implemented_duplicate_skip_contract() {
        let workflow = get_workflow("bulk_operations").unwrap();

        assert!(workflow.contains("Duplicate barcodes are skipped and reported"));
        assert!(!workflow.contains("Duplicate barcodes are NOT rejected"));
    }
}

mod tool_names {
    use crate::ai::workflows::*;

    /// A workflow that names a tool which does not exist teaches the model to
    /// call it.
    ///
    /// This is the same defect that produced "Unknown or unavailable AI tool:
    /// find_products" from the system prompt, found here afterwards by looking
    /// for other places the model is told what to call. Two workflows said
    /// `search_customers`; the tool is `list_customers`. The instruction read
    /// perfectly and pointed at nothing.
    ///
    /// Only verb-prefixed snake_case tokens are checked, and only against the
    /// live catalogue plus the registered operations — workflow prose also
    /// contains ordinary words and Rust method names.
    #[test]
    fn every_tool_a_workflow_tells_the_model_to_call_exists() {
        let catalogue: std::collections::HashSet<String> =
            crate::ai::tools_catalogue::all_tool_definitions()
                .into_iter()
                .map(|definition| definition.name)
                .collect();

        // Tokens that look like tools but are not: workflow identifiers, Rust
        // iterator methods, and prose. Listed rather than pattern-matched so a
        // real miss cannot hide behind a loose rule.
        const NOT_TOOLS: &[&str] = &[
            "get_workflow",
            "find_map",
            "bulk_operations",
            "sync_recovery",
            "sync_stuck",
            "bulk_product_archive",
            "bulk_promotion_apply",
            "bulk_reorder_point_update",
            "bulk_stock_variance_fix",
        ];

        let mut missing: Vec<(String, String)> = Vec::new();
        for name in workflow_names() {
            let Some(body) = get_workflow(name) else {
                continue;
            };
            for word in body.split(|c: char| !(c.is_ascii_lowercase() || c == '_')) {
                let looks_like_a_tool = [
                    "get_", "list_", "find_", "search_", "create_", "update_",
                    "set_", "adjust_", "bulk_", "export_", "check_", "preview_",
                ]
                .iter()
                .any(|prefix| word.starts_with(prefix))
                    && word.len() > 6;
                if !looks_like_a_tool || NOT_TOOLS.contains(&word) {
                    continue;
                }
                if !catalogue.contains(word)
                    && !crate::ai::engine::ops::is_registered_operation(word)
                {
                    missing.push((name.to_string(), word.to_string()));
                }
            }
        }

        assert!(
            missing.is_empty(),
            "workflows tell the model to call tools that do not exist: {missing:?}"
        );
    }
}
