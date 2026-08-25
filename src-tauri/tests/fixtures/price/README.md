# Captured competitor pages

Real responses, saved so the adapter tests run offline and deterministically. They
are the thing that fails loudly when a source is redesigned — an adapter tested
against hand-written HTML only proves the test author's idea of the markup.

Captured 23 August 2026 with a plain HTTP client identifying itself as ZANPOS. No
headers were spoofed and no access control was worked around: every one of these
paths is permitted by the site's own robots.txt (Bahrain Pharmacy publishes none).

| File | Source | Notes |
|---|---|---|
| `akelny_product.html` | `akelny.net/bh/products/almarai-uht-premium-strawberry-milk` | Full page. Carries `schema.org/Product` with an `AggregateOffer` naming two retailers. |
| `akelny_sitemap.xml` | `akelny.net/product-sitemap/bh-unified-0.xml` | **Trimmed to the first 200 `<url>` entries** — the live file holds 2,379 and the parser does not care how many. |
| `bp_product.html` | `bahrainpharmacy.com/store/product/vaseline-lip-care-mint-lip-balm-4-8-g/` | Full page. Carries `Product` with `gtin`, `brand` and a `priceSpecification`-nested price. |
| `bp_search.html` | `bahrainpharmacy.com/store/?s=vaseline&post_type=product` | **Tail trimmed.** Holds all 16 result links, which is what the discovery regex is tested against. |

Re-capture with `node qa/_fetch-fixtures.mjs` from the repo root when an adapter
needs updating. Expect the numbers in the tests to move when you do — that is the
point of them.
