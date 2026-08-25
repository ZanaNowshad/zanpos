-- Bahrain competitor prices.
--
-- Four tables, split along one line: what a person decided, versus what a
-- machine observed. The two halves have opposite properties and opposite sync
-- behaviour, and keeping them apart is what makes the trust rules enforceable
-- in SQL rather than in whichever caller remembers.

-- Where prices come from, and whether that source is currently working.
--
-- Device-local, like `product_image_attempts`. A source being unreachable from
-- one terminal's connection says nothing about another's, and syncing a failure
-- count would let one bad link switch a source off for the whole shop.
CREATE TABLE price_sources (
    source_id            TEXT PRIMARY KEY,
    name                 TEXT NOT NULL,
    base_url             TEXT NOT NULL,
    -- Which kinds of product this source is worth asking about, as a JSON
    -- array of route names. The router reads it so a phone is never looked up
    -- in a grocery aggregator, and so adding a source is data, not a rebuild.
    categories_json      TEXT NOT NULL DEFAULT '[]',
    enabled              INTEGER NOT NULL DEFAULT 1,
    -- 'ok' | 'degraded' | 'unsupported_direct_access'
    --
    -- The third is not a failure. LuLu Bahrain returns 403 to every non-browser
    -- client including its own sitemap, so the adapter records what it is and
    -- never fetches. A source that silently returned nothing would look like a
    -- product nobody else sells.
    status               TEXT NOT NULL DEFAULT 'ok',
    status_reason        TEXT,
    -- What to use instead. LuLu grocery prices still arrive, relayed by Akelny.
    fallback_source_id   TEXT,
    last_ok_at           TEXT,
    -- A source that parses nothing several times running has usually been
    -- redesigned. Surfacing that is what stops the comparison quietly emptying.
    consecutive_failures INTEGER NOT NULL DEFAULT 0,
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL
);

-- Which listing on which source is this product of ours.
--
-- The heart of the feature. Sources name products; most publish no barcode, so
-- pairing them is a judgement. `match_method` records whose judgement it was,
-- and only two answers are ever trusted:
--
--   BARCODE_EXACT       the source published a GTIN and it is one of ours
--   OPERATOR_CONFIRMED  a person looked at both and said yes
--   FUZZY_CANDIDATE     a name-and-size guess; shown, never counted
--
-- Synced: a manager who confirms a pairing at the back office has done work
-- every till should inherit, and it is small, human-authored and rarely changed.
CREATE TABLE product_matches (
    match_id            TEXT PRIMARY KEY,
    product_id          TEXT NOT NULL,
    source_id           TEXT NOT NULL,
    -- Whatever that source calls this listing for good: Akelny's UUID, Bahrain
    -- Pharmacy's numeric id. Not the slug where anything better exists — slugs
    -- change when a product is renamed and the pairing would be lost.
    source_product_key  TEXT NOT NULL,
    source_product_name TEXT NOT NULL,
    -- Verbatim, as printed. Pack size is part of a product's identity, so a
    -- confirmed pairing whose listing changes from 4x90g to 2x90g is suspended
    -- rather than quietly re-used against a different product.
    source_pack_size    TEXT,
    source_url          TEXT,
    match_method        TEXT NOT NULL,
    -- 0-100. Recorded for every match so a confirmation decision can be
    -- reviewed later, not just the ones that were rejected.
    confidence_score    INTEGER NOT NULL DEFAULT 0,
    confirmed_by_user_id TEXT,
    confirmed_at        TEXT,
    last_verified_at    TEXT,
    -- 'active' | 'pack_changed' | 'rejected'
    status              TEXT NOT NULL DEFAULT 'active',
    branch_id           TEXT NOT NULL DEFAULT '',
    origin_device_id    TEXT NOT NULL DEFAULT '',
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    deleted_at          TEXT,
    version             INTEGER NOT NULL DEFAULT 1,
    sync_status         TEXT NOT NULL DEFAULT 'pending',
    sync_attempts       INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_product_matches_product ON product_matches(product_id);
CREATE INDEX idx_product_matches_sync_status ON product_matches(sync_status);
-- One live pairing per product per listing. A rejected pairing keeps its row so
-- the same wrong candidate is not offered again next week.
CREATE UNIQUE INDEX idx_product_matches_unique
    ON product_matches(product_id, source_id, source_product_key)
    WHERE deleted_at IS NULL;

-- One price, seen at one shop, at one moment.
--
-- Append-only. Nothing here is ever updated: a price that changed is a new row,
-- which is what turns "what does LuLu charge" into "what has LuLu charged since
-- May" — the part no single lookup can give you.
--
-- Device-local. These are re-derivable by asking again, they accumulate fast,
-- and a terminal that has been offline for a week should not push a stale burst
-- of them at the hub.
CREATE TABLE price_observations (
    observation_id TEXT PRIMARY KEY,
    match_id       TEXT NOT NULL,
    -- Denormalised on purpose. On an aggregator one listing carries several
    -- retailers, so the shop charging this is a property of the observation,
    -- not of the match.
    retailer_name  TEXT NOT NULL,
    price_minor    INTEGER NOT NULL,
    currency       TEXT NOT NULL DEFAULT 'BHD',
    in_stock       INTEGER NOT NULL DEFAULT 1,
    source_url     TEXT,
    observed_at    TEXT NOT NULL
);

CREATE INDEX idx_price_observations_match ON price_observations(match_id, observed_at);

-- Products worth re-checking on a schedule.
--
-- Opt-in, and the only thing the background worker will ever fetch. Crawling
-- 28,010 products against a small third-party site so that one shop can price
-- better is not a trade anyone else agreed to; a watchlist keeps the traffic
-- proportional to the attention the operator is actually paying.
--
-- Synced with the matches: the decision to track something is the same kind of
-- decision as confirming what it is.
CREATE TABLE price_watchlist (
    watch_id         TEXT PRIMARY KEY,
    product_id       TEXT NOT NULL,
    added_by_user_id TEXT NOT NULL,
    branch_id        TEXT NOT NULL DEFAULT '',
    origin_device_id TEXT NOT NULL DEFAULT '',
    -- Device-local scheduling state, deliberately not part of what syncs.
    last_checked_at  TEXT,
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL,
    deleted_at       TEXT,
    version          INTEGER NOT NULL DEFAULT 1,
    sync_status      TEXT NOT NULL DEFAULT 'pending',
    sync_attempts    INTEGER NOT NULL DEFAULT 0
);

CREATE UNIQUE INDEX idx_price_watchlist_product
    ON price_watchlist(product_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_price_watchlist_sync_status ON price_watchlist(sync_status);

-- The sources this ships with. Registered as rows rather than compiled in so an
-- operator can switch one off, and so a new store is a row plus an adapter
-- rather than a rebuild.
INSERT INTO price_sources
    (source_id, name, base_url, categories_json, enabled, status, status_reason,
     fallback_source_id, created_at, updated_at)
VALUES
    ('akelny', 'Akelny', 'https://akelny.net/bh',
     '["food","beverages","household","baby","general"]', 1, 'ok', NULL, NULL,
     datetime('now'), datetime('now')),
    ('bahrain_pharmacy', 'Bahrain Pharmacy', 'https://bahrainpharmacy.com/store',
     '["cosmetics","personal_care","baby","health","household"]', 1, 'ok', NULL, NULL,
     datetime('now'), datetime('now')),
    -- Present so the UI can say why LuLu is missing instead of leaving a manager
    -- to wonder whether the biggest retailer in the country sells the product.
    ('lulu_bh', 'LuLu Hypermarket', 'https://gcc.luluhypermarket.com/en-bh',
     '["food","beverages","cosmetics","personal_care","baby","household","electronics","general"]',
     1, 'unsupported_direct_access',
     'LuLu returns 403 to non-browser clients, including its own sitemap. Its robots.txt permits these paths; the block is on the client, not the path.',
     'akelny', datetime('now'), datetime('now'));
