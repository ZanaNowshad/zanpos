//! Deciding what may be repaired automatically, and what a person has to look at.
//!
//! Parity now finds the exact rows that differ. The obvious next move — copy
//! whichever side looks newer — is the one that must not be made, because "the
//! rows differ" covers two situations that are not alike:
//!
//! * A row exists on one node and not the other. Nothing is in conflict; a
//!   message was lost. Delivering it destroys no information, and delivering it
//!   twice destroys none either, so it can be done unattended.
//! * A row exists on both and the contents disagree. Something was written in
//!   two places. Picking a winner throws the loser away, and for a payment the
//!   loser may be the only record that a customer handed over money.
//!
//! So authority is per table, and it is asked *before* anything is written:
//!
//! ```text
//! DETECT → IDENTIFY diverging PKs → DETERMINE authority → REPAIR → RE-RUN → RECORD
//! ```
//!
//! This module is the DETERMINE step, deliberately separated and pure. It writes
//! nothing. It answers one question — may this be repaired without a person —
//! and every answer is a value that can be tested at its boundaries rather than
//! a branch buried in a repair loop.

use crate::sync_v2::parity::{Divergence, DivergentRow};
use crate::sync_v2::registry::{self, Deletion};
use serde::{Deserialize, Serialize};

/// What may be done about one divergent row.
///
/// Serialize-only: the reasons are static strings this module authors, and a
/// decision arriving from outside would be a decision nothing here made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Resolution {
    /// Deliver the missing row. Safe unattended: nothing is overwritten, and
    /// the tables this applies to are append-only or carry an unambiguous
    /// tombstone.
    Deliver { from: Side, reason: &'static str },
    /// Both sides hold it and disagree. A person decides. Automatic repair is
    /// refused even when one side looks newer, because "newer" is not
    /// "correct" when the older row may be the only evidence of a payment.
    Escalate { reason: &'static str },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Hub,
    Terminal,
}

/// Whether a table's contents may ever be overwritten without review.
///
/// Read off the registry rather than listed again here, so a table added there
/// gets a policy by construction. The default is the cautious one: a table
/// nobody has classified is treated as needing review.
pub fn contents_may_be_auto_repaired(table: &str) -> bool {
    match registry::get(table) {
        // Append-only ledgers. A row that exists on both sides and differs is
        // not a merge candidate — it means one node rewrote history, which is
        // precisely what nobody should paper over.
        Some(entry) => entry.deletion != Deletion::Never && !is_financial(table),
        None => false,
    }
}

/// Tables where a wrong automatic choice loses money or the record of it.
///
/// Named explicitly rather than inferred, because the cost of being wrong here
/// is not symmetric with the cost of asking.
pub fn is_financial(table: &str) -> bool {
    matches!(
        table,
        "sales"
            | "sale_items"
            | "payments"
            | "refunds"
            | "refund_items"
            | "cash_events"
            | "shifts"
            | "product_cost_history"
    )
}

/// Decide what may be done about one divergent row.
pub fn resolve(table: &str, row: &DivergentRow) -> Resolution {
    match row.divergence {
        // A row only one side holds is a delivery, not a conflict. Even for a
        // payment: the node that lacks it has no competing version to lose.
        Divergence::MissingLocally => Resolution::Deliver {
            from: Side::Hub,
            reason: "present on the hub and absent here — a pull that never landed",
        },
        Divergence::MissingOnHub => Resolution::Deliver {
            from: Side::Terminal,
            reason: "present here and absent on the hub — a push still queued or lost",
        },
        Divergence::Different if is_financial(table) => Resolution::Escalate {
            reason: "both nodes hold this financial record with different contents; \
                     the older copy may be the only evidence of a real transaction",
        },
        Divergence::Different if !contents_may_be_auto_repaired(table) => Resolution::Escalate {
            reason: "append-only table with two different versions of the same row, \
                     which means one node rewrote history",
        },
        Divergence::Different => Resolution::Escalate {
            reason: "both nodes hold this row with different contents; last-writer-wins \
                     is row-level, so repairing it would discard whichever field the \
                     losing node edited",
        },
    }
}

/// A reconciliation plan: what would be delivered, and what needs a person.
#[derive(Debug, Clone, Serialize)]
pub struct ReconciliationPlan {
    pub table: String,
    pub deliverable: Vec<PlannedRepair>,
    pub escalated: Vec<PlannedRepair>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlannedRepair {
    pub pk: String,
    pub resolution: Resolution,
}

impl ReconciliationPlan {
    /// True when everything found can be fixed without anyone deciding.
    pub fn is_fully_automatic(&self) -> bool {
        self.escalated.is_empty() && !self.deliverable.is_empty()
    }

    pub fn needs_review(&self) -> bool {
        !self.escalated.is_empty()
    }
}

/// Turn a set of divergent rows into a plan, without touching anything.
///
/// A plan is produced even when every row escalates, because "seventeen rows
/// need review" is the useful answer in that case — not an error, and not
/// silence.
pub fn plan(table: &str, rows: &[DivergentRow]) -> ReconciliationPlan {
    let mut deliverable = Vec::new();
    let mut escalated = Vec::new();
    for row in rows {
        let resolution = resolve(table, row);
        let planned = PlannedRepair {
            pk: row.pk.clone(),
            resolution: resolution.clone(),
        };
        match resolution {
            Resolution::Deliver { .. } => deliverable.push(planned),
            Resolution::Escalate { .. } => escalated.push(planned),
        }
    }
    ReconciliationPlan {
        table: table.to_string(),
        deliverable,
        escalated,
    }
}

/// One line per decision, for the audit record.
///
/// Reconciliation changes data on the strength of a comparison, so what it
/// decided and why has to outlive the run — including the rows it declined to
/// touch, which are the ones somebody will ask about later.
pub fn audit_lines(plan: &ReconciliationPlan) -> Vec<String> {
    let mut lines = Vec::new();
    for repair in plan.deliverable.iter().chain(plan.escalated.iter()) {
        lines.push(match &repair.resolution {
            Resolution::Deliver { from, reason } => format!(
                "{}/{}: deliver from {} — {reason}",
                plan.table,
                repair.pk,
                match from {
                    Side::Hub => "hub",
                    Side::Terminal => "terminal",
                }
            ),
            Resolution::Escalate { reason } => {
                format!("{}/{}: left for review — {reason}", plan.table, repair.pk)
            }
        });
    }
    lines
}

#[cfg(test)]
mod tests;
