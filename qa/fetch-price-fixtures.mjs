// One-off: capture real pages so the adapter tests run offline and deterministically.
import { writeFileSync } from "node:fs";

const UA = "ZANPOS/2.0 (+retail price comparison for a single Bahrain shop; contact zanabal.nowshad@gmail.com)";
const targets = [
  ["akelny_sitemap.xml",  "https://akelny.net/product-sitemap/bh-unified-0.xml"],
  ["akelny_product.html", "https://akelny.net/bh/products/almarai-uht-premium-strawberry-milk"],
  ["bp_search.html",      "https://bahrainpharmacy.com/store/?s=vaseline&post_type=product"],
  ["bp_product.html",     "https://bahrainpharmacy.com/store/product/vaseline-lip-care-mint-lip-balm-4-8-g/"],
];

for (const [name, url] of targets) {
  try {
    const r = await fetch(url, { headers: { "user-agent": UA, accept: "text/html,application/xhtml+xml,application/xml" } });
    const body = await r.text();
    console.log(`${r.status}  ${body.length.toString().padStart(8)}  ${name}  ${url}`);
    if (r.ok) writeFileSync(`src-tauri/tests/fixtures/price/${name}`, body);
  } catch (e) {
    console.log(`ERR   ${name}: ${e.message}`);
  }
  await new Promise(r => setTimeout(r, 1500));
}
