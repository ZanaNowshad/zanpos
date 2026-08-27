//! Deciding whether a competitor's listing is the same product as ours.
//!
//! This is the load-bearing judgement in the whole feature, and the reason it
//! is a separate module with its own tests. No Bahrain source exposes a barcode
//! — Akelny prints a pack size, Bahrain Pharmacy puts the size in the product
//! name — so ZANPOS's 29,678 canonical GTINs are useless for matching against
//! them. Everything rests on names and pack sizes, which is a guess.
//!
//! So a guess is never allowed to count. Trust is deliberately asymmetric:
//!
//! ```text
//! BarcodeExact       trusted, no human involved   (only when a source ever
//!                                                  publishes a GTIN)
//! OperatorConfirmed  trusted, a person said yes
//! FuzzyCandidate     shown, labelled, and feeds nothing
//! ```
//!
//! The cost of the two mistakes is not symmetric either. Offering a candidate
//! that turns out to be the wrong product wastes a manager ten seconds. Feeding
//! that same guess into a median that a manager then prices against moves real
//! money, silently, and keeps doing it every week until somebody notices the
//! number was never about their product. Only the second is worth engineering
//! against, so the rule is enforced in the SQL that reads observations rather
//! than in whichever caller happens to be asking.

use crate::errors::{AppError, AppResult};
use crate::price_intelligence::pack::PackSize;
use crate::price_intelligence::sources::tokenize;
use crate::price_intelligence::SourceProduct;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use ulid::Ulid;

/// How a product was paired with a listing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchMethod {
    /// A GTIN on both sides agreed. Nothing to second-guess.
    BarcodeExact,
    /// A person looked at the listing and said it was the right product.
    OperatorConfirmed,
    /// Name and pack size scored well enough to be worth showing. Not evidence.
    FuzzyCandidate,
}

impl MatchMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BarcodeExact => "BARCODE_EXACT",
            Self::OperatorConfirmed => "OPERATOR_CONFIRMED",
            Self::FuzzyCandidate => "FUZZY_CANDIDATE",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "BARCODE_EXACT" => Some(Self::BarcodeExact),
            "OPERATOR_CONFIRMED" => Some(Self::OperatorConfirmed),
            "FUZZY_CANDIDATE" => Some(Self::FuzzyCandidate),
            _ => None,
        }
    }

    /// Whether observations recorded against this match may be quoted as a
    /// price. The single place that question is answered.
    pub fn is_trusted(self) -> bool {
        matches!(self, Self::BarcodeExact | Self::OperatorConfirmed)
    }
}

/// SQL fragment selecting only matches whose prices may be quoted.
///
/// Written once and used by every read, because the invariant has to hold in
/// the query. A caller that forgets to filter is exactly the bug this feature
/// cannot afford, and a caller cannot forget a `WHERE` clause it never writes.
pub const TRUSTED_MATCH_SQL: &str =
    "match_method IN ('BARCODE_EXACT', 'OPERATOR_CONFIRMED') AND status = 'active'";

/// A listing offered to an operator as possibly-the-same-product.
///
/// Deserialisable because confirming one sends it back across the IPC boundary
/// verbatim: the pairing is stored against the listing the operator actually
/// looked at, not one re-fetched in between and possibly changed underneath
/// them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub source_id: String,
    pub source_product_key: String,
    pub name: String,
    pub pack_text: Option<String>,
    pub url: String,
    /// 0–100. Shown so a manager can see the difference between "almost
    /// certainly" and "possibly", rather than being handed a flat list.
    pub confidence: i64,
    pub offers: Vec<crate::price_intelligence::SourceOffer>,
}

/// Score a listing against the product being priced.
///
/// Deliberately conservative. The score decides what is worth an operator's
/// attention, never what is true — so a high score still produces a candidate
/// and still feeds nothing until somebody confirms it.
pub fn score(our_name: &str, our_pack: Option<&PackSize>, listing: &SourceProduct) -> i64 {
    let ours = tokenize(our_name);
    let theirs = tokenize(&listing.name);
    if ours.is_empty() || theirs.is_empty() {
        return 0;
    }

    let overlap = ours.iter().filter(|word| theirs.contains(word)).count();
    if overlap == 0 {
        return 0;
    }
    // Proportion of *our* words found in theirs. A listing with a long name is
    // not penalised for describing itself thoroughly.
    let mut confidence = (overlap * 100 / ours.len()) as i64;

    // Pack size is part of product identity, not a detail. Four 90g bars and
    // two 90g bars are different products at different prices, and their names
    // are usually identical — so when both sides state a size and the sizes
    // disagree, that is not a weak signal to subtract a few points for. It is
    // the answer. A near-perfect name match is exactly the case where this
    // matters: without disqualification, "Dove Beauty Bar 4x90g" scores 100 on
    // name against the 2x90g listing and gets offered as the same product.
    // `comparable_to`, not `==`: it allows a couple of percent, so "160ml" and
    // "158ml" read as the same tin rather than as different products, while a
    // 2x90g against a 4x90g still fails.
    match (our_pack, listing.pack.as_ref()) {
        (Some(ours), Some(theirs)) if ours.comparable_to(theirs) => confidence += 15,
        (Some(_), Some(_)) => return 0,
        // One side silent: no evidence either way, so no adjustment. Most
        // sources print a size only sometimes, and treating absence as
        // disagreement would hide real matches.
        _ => {}
    }

    // A brand that agrees is worth more than another matching word.
    if let Some(brand) = listing.brand.as_deref() {
        if tokenize(brand).iter().any(|word| ours.contains(word)) {
            confidence += 10;
        }
    }

    confidence.clamp(0, 100)
}

/// Whether a listing carries a barcode we also carry.
///
/// This is the one tier that needs no human. A GTIN is a global identifier for
/// a specific product in a specific pack, so two sides agreeing on one is not a
/// similarity score — it is the same item. Rare against Bahrain sources, which
/// mostly print a pack size and no barcode at all, but free when it happens and
/// it removes a confirmation the operator would otherwise have to make.
///
/// Compared on digits only: sources print GTINs with spaces and hyphens, and a
/// leading zero is dropped as often as it is kept.
pub fn barcode_match(our_barcodes: &[String], listing: &SourceProduct) -> bool {
    let Some(theirs) = listing.gtin.as_deref() else {
        return false;
    };
    let digits = |s: &str| -> String { s.chars().filter(char::is_ascii_digit).collect() };
    let theirs = digits(theirs);
    if theirs.len() < 8 {
        return false;
    }
    our_barcodes
        .iter()
        .map(|ours| digits(ours))
        .any(|ours| ours.len() >= 8 && (ours == theirs || ours.trim_start_matches('0') == theirs.trim_start_matches('0')))
}

/// Below this a listing is not worth an operator's attention.
///
/// Showing everything is the same as showing nothing: a manager scanning ten
/// near-misses stops reading, and the one real match is missed among them.
pub const MIN_CONFIDENCE: i64 = 40;

/// Rank listings for one product, best first, keeping only plausible ones.
pub fn rank(
    our_name: &str,
    our_pack: Option<&PackSize>,
    listings: Vec<SourceProduct>,
) -> Vec<Candidate> {
    let mut scored: Vec<Candidate> = listings
        .into_iter()
        .filter_map(|listing| {
            let confidence = score(our_name, our_pack, &listing);
            (confidence >= MIN_CONFIDENCE).then(|| Candidate {
                source_id: listing.source_id.to_string(),
                source_product_key: listing.key.clone(),
                name: listing.name.clone(),
                pack_text: listing.pack_text.clone(),
                url: listing.url.clone(),
                confidence,
                offers: listing.offers,
            })
        })
        .collect();
    scored.sort_by(|a, b| b.confidence.cmp(&a.confidence).then(a.name.cmp(&b.name)));
    scored
}

/// A stored pairing between one of our products and one source listing.
#[derive(Debug, Clone, Serialize)]
pub struct StoredMatch {
    pub match_id: String,
    pub product_id: String,
    pub source_id: String,
    pub source_product_key: String,
    pub source_product_name: String,
    pub source_pack_size: Option<String>,
    pub source_url: Option<String>,
    pub match_method: String,
    pub status: String,
}

impl StoredMatch {
    pub fn is_trusted(&self) -> bool {
        MatchMethod::parse(&self.match_method).is_some_and(MatchMethod::is_trusted)
            && self.status == "active"
    }
}

fn row_to_match(row: &sqlx::sqlite::SqliteRow) -> StoredMatch {
    StoredMatch {
        match_id: row.get("match_id"),
        product_id: row.get("product_id"),
        source_id: row.get("source_id"),
        source_product_key: row.get("source_product_key"),
        source_product_name: row.get("source_product_name"),
        source_pack_size: row.get("source_pack_size"),
        source_url: row.get("source_url"),
        match_method: row.get("match_method"),
        status: row.get("status"),
    }
}

/// Every pairing recorded for a product, whatever its state.
pub async fn for_product(pool: &SqlitePool, product_id: &str) -> AppResult<Vec<StoredMatch>> {
    let rows = sqlx::query(
        "SELECT match_id, product_id, source_id, source_product_key, source_product_name,
                source_pack_size, source_url, match_method, status
           FROM product_matches
          WHERE product_id = ? AND deleted_at IS NULL
          ORDER BY source_id",
    )
    .bind(product_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(row_to_match).collect())
}

/// Record that an operator confirmed a listing is the right product.
///
/// From here on this pairing is exact: later refreshes fetch the same listing
/// key and no scoring is involved.
pub async fn confirm(
    pool: &SqlitePool,
    product_id: &str,
    branch_id: &str,
    actor_user_id: &str,
    candidate: &Candidate,
) -> AppResult<String> {
    write_match(
        pool,
        product_id,
        branch_id,
        Some(actor_user_id),
        candidate,
        MatchMethod::OperatorConfirmed,
    )
    .await
}

/// The one statement that files a pairing, whichever tier settled it.
///
/// Both callers write the same row shape; only the method and whether a person
/// is named differ. Two copies of an upsert whose conflict target has to match a
/// partial index exactly is two chances to get that wrong.
async fn write_match(
    pool: &SqlitePool,
    product_id: &str,
    branch_id: &str,
    actor_user_id: Option<&str>,
    candidate: &Candidate,
    method: MatchMethod,
) -> AppResult<String> {
    let now = chrono::Utc::now().to_rfc3339();
    let match_id = Ulid::new().to_string();

    sqlx::query(
        "INSERT INTO product_matches
            (match_id, product_id, source_id, source_product_key, source_product_name,
             source_pack_size, source_url, match_method, confidence_score,
             confirmed_by_user_id, confirmed_at, last_verified_at, status,
             branch_id, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'active', ?, ?, ?)
         ON CONFLICT(product_id, source_id, source_product_key)
             WHERE deleted_at IS NULL
         DO UPDATE SET
             source_product_name = excluded.source_product_name,
             source_pack_size    = excluded.source_pack_size,
             source_url          = excluded.source_url,
             match_method        = excluded.match_method,
             confidence_score    = excluded.confidence_score,
             confirmed_by_user_id = excluded.confirmed_by_user_id,
             confirmed_at        = excluded.confirmed_at,
             last_verified_at    = excluded.last_verified_at,
             status              = 'active',
             updated_at          = excluded.updated_at,
             sync_status         = 'pending'",
    )
    .bind(&match_id)
    .bind(product_id)
    .bind(&candidate.source_id)
    .bind(&candidate.source_product_key)
    .bind(&candidate.name)
    .bind(&candidate.pack_text)
    .bind(&candidate.url)
    .bind(method.as_str())
    .bind(candidate.confidence)
    .bind(actor_user_id)
    .bind(&now)
    .bind(&now)
    .bind(branch_id)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(match_id)
}

/// File a pairing settled by a barcode. No operator involved, because a GTIN
/// agreeing on both sides is not a judgement anybody needs to make.
pub async fn confirm_by_barcode(
    pool: &SqlitePool,
    product_id: &str,
    branch_id: &str,
    candidate: &Candidate,
) -> AppResult<String> {
    write_match(
        pool,
        product_id,
        branch_id,
        None,
        candidate,
        MatchMethod::BarcodeExact,
    )
    .await
}

/// Mark a pairing as wrong. It is not deleted: without a record, the next
/// refresh would score the same listing highly and offer it again.
pub async fn reject(pool: &SqlitePool, match_id: &str) -> AppResult<()> {
    let affected = sqlx::query(
        "UPDATE product_matches
            SET status = 'rejected', updated_at = ?, sync_status = 'pending'
          WHERE match_id = ? AND deleted_at IS NULL",
    )
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(match_id)
    .execute(pool)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(format!("No such match: {match_id}")));
    }
    Ok(())
}

/// Suspend a confirmed match whose listing has changed pack size.
///
/// A confirmation is a statement about a specific product, and pack size is
/// part of what makes it that product. When a listing goes from `4 x 90g` to
/// `2 x 90g` the operator confirmed something that is no longer on the page, so
/// the pairing stops being evidence until somebody looks again. Silently
/// carrying it forward would quote half a pack's price as if it were the whole.
pub async fn suspend_if_pack_changed(
    pool: &SqlitePool,
    stored: &StoredMatch,
    listing: &SourceProduct,
) -> AppResult<bool> {
    let (Some(was), Some(now)) = (stored.source_pack_size.as_deref(), listing.pack_text.as_deref())
    else {
        return Ok(false);
    };
    if was.trim().eq_ignore_ascii_case(now.trim()) {
        return Ok(false);
    }

    sqlx::query(
        "UPDATE product_matches
            SET status = 'pack_changed', source_pack_size = ?, updated_at = ?,
                sync_status = 'pending'
          WHERE match_id = ?",
    )
    .bind(now)
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(&stored.match_id)
    .execute(pool)
    .await?;
    Ok(true)
}

#[cfg(test)]
mod tests;
