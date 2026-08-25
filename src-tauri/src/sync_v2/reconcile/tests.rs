use super::*;

fn row(pk: &str, divergence: Divergence) -> DivergentRow {
    DivergentRow {
        pk: pk.to_string(),
        divergence,
    }
}

/// The rule the whole module exists to enforce. A payment held differently on
/// two nodes is not a merge candidate: the copy that would lose may be the only
/// record that a customer handed over money.
#[test]
fn a_financial_row_that_differs_on_both_sides_is_never_repaired_automatically() {
    for table in [
        "sales",
        "sale_items",
        "payments",
        "refunds",
        "refund_items",
        "cash_events",
        "shifts",
        "product_cost_history",
    ] {
        let resolution = resolve(table, &row("x", Divergence::Different));
        assert!(
            matches!(resolution, Resolution::Escalate { .. }),
            "{table} would have been auto-repaired: {resolution:?}"
        );
        assert!(!contents_may_be_auto_repaired(table), "{table}");
    }
}

/// A row only one side holds is a delivery, not a conflict — including for a
/// payment, because the node that lacks it has no competing version to lose.
/// Refusing these would leave the common case needing a human forever.
#[test]
fn a_row_only_one_side_holds_is_delivered_even_for_money() {
    let missing_here = resolve("payments", &row("pay_1", Divergence::MissingLocally));
    assert_eq!(
        missing_here,
        Resolution::Deliver {
            from: Side::Hub,
            reason: "present on the hub and absent here — a pull that never landed",
        }
    );

    let missing_there = resolve("sales", &row("sale_1", Divergence::MissingOnHub));
    assert!(matches!(
        missing_there,
        Resolution::Deliver { from: Side::Terminal, .. }
    ));
}

/// Catalogue rows differ all the time for dull reasons, but last-writer-wins is
/// row-level: repairing one would discard whichever field the losing node
/// edited. That is a decision, so it is escalated too — just for a different
/// reason, which the message has to say.
#[test]
fn a_catalogue_row_that_differs_is_escalated_with_its_own_reason() {
    let product = resolve("products", &row("prd_1", Divergence::Different));
    let payment = resolve("payments", &row("pay_1", Divergence::Different));

    let (Resolution::Escalate { reason: product_reason }, Resolution::Escalate { reason: payment_reason }) =
        (product, payment)
    else {
        panic!("expected both to escalate");
    };
    assert!(product_reason.contains("last-writer-wins"), "{product_reason}");
    assert!(payment_reason.contains("evidence of a real transaction"), "{payment_reason}");
    assert_ne!(product_reason, payment_reason);
}

/// A table nobody has classified must not inherit permission by accident.
#[test]
fn an_unknown_table_defaults_to_needing_review() {
    assert!(!contents_may_be_auto_repaired("some_new_table"));
    assert!(matches!(
        resolve("some_new_table", &row("x", Divergence::Different)),
        Resolution::Escalate { .. }
    ));
}

#[test]
fn a_plan_separates_what_can_be_delivered_from_what_needs_a_person() {
    let rows = vec![
        row("sale_a", Divergence::MissingLocally),
        row("sale_b", Divergence::MissingOnHub),
        row("sale_c", Divergence::Different),
    ];
    let plan = plan("sales", &rows);

    assert_eq!(plan.deliverable.len(), 2);
    assert_eq!(plan.escalated.len(), 1);
    assert_eq!(plan.escalated[0].pk, "sale_c");
    assert!(plan.needs_review());
    assert!(!plan.is_fully_automatic(), "a conflict must block the automatic path");
}

#[test]
fn a_plan_of_pure_deliveries_may_run_unattended() {
    let plan = plan(
        "sales",
        &[
            row("sale_a", Divergence::MissingLocally),
            row("sale_b", Divergence::MissingLocally),
        ],
    );

    assert!(plan.is_fully_automatic());
    assert!(!plan.needs_review());
}

/// Nothing to do is not the same as everything can be done. An empty plan must
/// not read as "safe to run", or a caller looping on `is_fully_automatic` would
/// treat a clean table as work.
#[test]
fn an_empty_plan_is_not_reported_as_automatically_repairable() {
    let plan = plan("sales", &[]);
    assert!(!plan.is_fully_automatic());
    assert!(!plan.needs_review());
}

/// Reconciliation changes data on the strength of a comparison, so the record
/// has to include what it declined to touch — those are the rows somebody asks
/// about later.
#[test]
fn the_audit_record_covers_the_refusals_as_well_as_the_repairs() {
    let plan = plan(
        "payments",
        &[
            row("pay_1", Divergence::MissingLocally),
            row("pay_2", Divergence::Different),
        ],
    );
    let lines = audit_lines(&plan);

    assert_eq!(lines.len(), 2);
    assert!(lines.iter().any(|l| l.contains("payments/pay_1") && l.contains("deliver from hub")));
    assert!(lines.iter().any(|l| l.contains("payments/pay_2") && l.contains("left for review")));
    // Every line names the row and carries a reason, or the record is useless
    // to the person reading it a week later.
    for line in &lines {
        assert!(line.contains('—'), "no reason recorded: {line}");
    }
}

/// The policy is read off the registry, so a table added there gets one by
/// construction rather than by somebody remembering this file.
#[test]
fn every_registered_table_has_a_defined_policy() {
    for table in crate::sync_v2::registry::TABLES {
        let resolution = resolve(table.name, &row("x", Divergence::Different));
        // Whatever it decides, it must decide something rather than panic or
        // fall through to a permissive default.
        assert!(matches!(
            resolution,
            Resolution::Escalate { .. } | Resolution::Deliver { .. }
        ));
        // And append-only tables are never auto-repairable on content.
        if table.deletion == Deletion::Never {
            assert!(
                !contents_may_be_auto_repaired(table.name),
                "{} is append-only but allows content repair",
                table.name
            );
        }
    }
}
