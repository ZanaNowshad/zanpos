//! How a free-text customer search is turned into SQL.
//!
//! Split out of `customer_commands.rs` for the 500-line rule, and because the
//! predicate is worth reading on its own: it is the difference between a
//! customer of a year being recognised at the counter and being entered again
//! as a stranger.

/// The stored `phone` with every separator taken out, as a SQL expression.
///
/// Customers are saved the way they were dictated — `+973 3600 1122`,
/// `36001122`, `+97336001122` — and a cashier at the till types the eight
/// digits with none of it. `phone LIKE '%36001122%'` misses every formatted
/// row, so at the counter a saved customer looked like a new one and the
/// receipt went out unattached to their account. Stripping in the comparison
/// rather than at write time means existing rows are found without a migration
/// over the whole table.
const PHONE_DIGITS_SQL: &str = "REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(\
    IFNULL(phone, ''), ' ', ''), '-', ''), '(', ''), ')', ''), '+', ''), '/', '')";

/// Rows matching a free-text search: name, email, phone as written, and — only
/// when the query contains digits — phone with its punctuation removed.
///
/// The digit clause is added rather than always present, and that is not an
/// optimisation. A first attempt kept one code path and bound a sentinel
/// pattern when there were no digits to search for; SQLite truncated it to an
/// empty string, `IFNULL(phone, '') LIKE ''` matched every customer with no
/// phone number, and a search scoped to one branch started returning another
/// branch's roster. The existing cross-branch test caught it. Not asking the
/// question is the only answer that cannot be matched by accident.
///
/// Plain `?` placeholders bound in order, like the rest of this file's callers.
pub fn customer_search_where(has_digits: bool) -> String {
    if has_digits {
        format!(
            "(name LIKE ? OR phone LIKE ? OR email LIKE ? OR {} LIKE ?)",
            PHONE_DIGITS_SQL,
        )
    } else {
        "(name LIKE ? OR phone LIKE ? OR email LIKE ?)".to_string()
    }
}

/// What was typed, as a LIKE pattern, and its digits alone when it has any.
pub fn customer_search_patterns(search: &str) -> (String, Option<String>) {
    let trimmed = search.trim();
    let digits: String = trimmed.chars().filter(|c| c.is_ascii_digit()).collect();
    let digit_pattern = if digits.is_empty() {
        None
    } else {
        Some(format!("%{}%", digits))
    };
    (format!("%{}%", trimmed), digit_pattern)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_with_no_digits_asks_no_digit_question() {
        let (pattern, digits) = customer_search_patterns("Fatima");
        assert_eq!(pattern, "%Fatima%");
        assert_eq!(digits, None);
        assert!(!customer_search_where(false).contains("REPLACE"));
    }

    #[test]
    fn a_typed_number_keeps_only_its_digits() {
        let (_, digits) = customer_search_patterns("+973 3600 1122");
        assert_eq!(digits.as_deref(), Some("%97336001122%"));
    }

    #[test]
    fn the_digit_clause_binds_exactly_one_extra_pattern() {
        // Four placeholders with digits, three without. A mismatch here binds
        // the branch id into a LIKE and silently drops the branch filter.
        assert_eq!(customer_search_where(true).matches('?').count(), 4);
        assert_eq!(customer_search_where(false).matches('?').count(), 3);
    }
}
