-- One definition of "what this product sells for right now", in the database.
--
-- Selling prices live in `product_prices` and nowhere else, and a product can
-- hold many rows: the price it used to have, the price it has, and a price
-- scheduled to start next week. Picking the right one takes four conditions and
-- a tie-break, and fourteen different queries across the POS, the back office
-- and the assistant each had their own idea of how many of those to apply.
--
-- The ones that dropped `effective_from` treated a price scheduled for next week
-- as today's price, so the assistant quoted a figure the till would not charge.
-- The ones that dropped the tie-break returned whichever of two open rows the
-- join reached first — and in a list query a LEFT JOIN matching two rows returns
-- the product twice.
--
-- A view is the form of this that every reader can use without composing SQL
-- strings: `JOIN v_current_selling_price` instead of `JOIN product_prices`, and
-- the question is answered the same way everywhere.
--
-- `datetime()` on both sides is load-bearing, not decoration. The column holds
-- two formats — Rust writes RFC3339 (`2026-08-31T09:00:00Z`), the importer
-- writes `datetime('now')` (space-separated) — and compared as text, `T` (0x54)
-- ranks above a space (0x20). A raw `<=` gets the answer wrong wherever the two
-- formats meet. `datetime()` normalises both.
CREATE VIEW v_current_selling_price AS
SELECT
    pp.price_id,
    pp.product_id,
    pp.price_minor,
    pp.currency,
    pp.effective_from
FROM product_prices pp
WHERE pp.price_id = (
    SELECT candidate.price_id
    FROM product_prices candidate
    WHERE candidate.product_id = pp.product_id
      AND candidate.branch_id IS NULL
      AND candidate.price_type = 'selling'
      AND datetime(candidate.effective_from) <= datetime('now')
      AND (candidate.effective_to IS NULL
           OR datetime(candidate.effective_to) > datetime('now'))
    ORDER BY datetime(candidate.effective_from) DESC, candidate.price_id DESC
    LIMIT 1
);
