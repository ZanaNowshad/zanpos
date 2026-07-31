import { useCallback, useEffect, useMemo, useState } from "react";
import {
  buildWhatsAppOrder,
  formatMoney,
  lineTotalMinor,
  normalizeQuantity,
  refreshCartLines,
} from "./domain";
import { copy } from "./copy";
import type { CartLine, Catalog, CatalogProduct, Locale } from "./types";
import "./styles.css";

const CART_KEY = "zanpos:cart:v1";
const CATALOG_KEY = "zanpos:catalog:v1";
const DEFAULT_ZANSHOP_URL = (
  import.meta as ImportMeta & { env?: Record<string, string | undefined> }
).env?.VITE_ZANSHOP_URL;

const safeParse = <T,>(raw: string | null, fallback: T): T => {
  try { return raw ? JSON.parse(raw) as T : fallback; } catch { return fallback; }
};

function ProductCard({ product, locale, currency, decimals, onAdd }: {
  product: CatalogProduct; locale: Locale; currency: string; decimals: number;
  onAdd: (product: CatalogProduct) => void;
}) {
  const t = copy(locale);
  const name = product.name[locale];
  return (
    <article className={`product-card ${product.available ? "" : "is-unavailable"}`}>
      <div className="product-visual">
        {product.imageUrl
          ? <img src={product.imageUrl} alt="" loading="lazy" />
          : <span aria-hidden="true">{name.slice(0, 1)}</span>}
        {!product.available && <span className="sold-tag">{t.unavailable}</span>}
      </div>
      <div className="product-copy">
        <p className="product-category">{product.name[locale === "en" ? "ar" : "en"]}</p>
        <h2>{name}</h2>
        {product.description && <p className="description">{product.description[locale]}</p>}
        <div className="price-row">
          <strong>{formatMoney(product.priceMinor, currency, decimals, locale)}</strong>
          <button
            type="button"
            disabled={!product.available}
            aria-label={`${t.add} ${name}`}
            onClick={() => onAdd(product)}
          >
            <span aria-hidden="true">＋</span> {t.add}
          </button>
        </div>
      </div>
    </article>
  );
}

function QuantityInput({ line, decimals, label, onQuantity }: {
  line: CartLine; decimals: number; label: string; onQuantity: (id: string, quantity: number) => void;
}) {
  const [draft, setDraft] = useState(String(line.quantity));
  useEffect(() => setDraft(String(line.quantity)), [line.quantity]);
  return (
    <label>
      <span className="sr-only">{label} {line.name}</span>
      <input
        aria-label={`${label} ${line.name}`}
        inputMode="decimal"
        value={draft}
        onChange={(event) => {
          const next = event.target.value;
          if (!/^\d*(?:\.\d{0,3})?$/.test(next)) return;
          setDraft(next);
          if (next !== "" && !next.endsWith(".")) onQuantity(line.id, normalizeQuantity(next, decimals));
        }}
        onBlur={() => {
          const normalized = normalizeQuantity(draft, decimals);
          if (normalized) setDraft(String(normalized));
          else onQuantity(line.id, 0);
        }}
      />
    </label>
  );
}

function CartRibbon({ lines, catalog, locale, onQuantity, onRemove }: {
  lines: CartLine[]; catalog: Catalog; locale: Locale;
  onQuantity: (id: string, quantity: number) => void; onRemove: (id: string) => void;
}) {
  const t = copy(locale);
  const [open, setOpen] = useState(false);
  const currentLines = useMemo(
    () => refreshCartLines(lines, catalog.products, locale),
    [catalog.products, lines, locale],
  );
  const total = currentLines.reduce((sum, line) => sum + lineTotalMinor(line), 0);
  const orderId = useMemo(() => crypto.randomUUID(), [lines.length > 0]);
  const message = currentLines.length ? buildWhatsAppOrder({
    orderId, currency: catalog.currency.code, decimals: catalog.currency.decimals, locale, lines: currentLines,
  }) : "";
  const href = `https://wa.me/${catalog.store.phone.replace(/\D/g, "")}?text=${encodeURIComponent(message)}`;
  return (
    <aside className={`cart-ribbon ${open ? "is-open" : ""}`} aria-label={t.cart}>
      <button className="ribbon-handle" onClick={() => setOpen((value) => !value)} aria-expanded={open}>
        <span>{t.cart}</span>
        <b>{currentLines.length}</b>
      </button>
      <div className="cart-body">
        {currentLines.length === 0 ? (
          <div className="cart-empty"><strong>{t.cartEmpty}</strong><span>{t.cartHint}</span></div>
        ) : currentLines.map((line) => {
          const product = catalog.products.find(({ id }) => id === line.id);
          return (
            <div className="cart-line" key={line.id}>
              <div><strong>{line.name}</strong><small>{formatMoney(lineTotalMinor(line), catalog.currency.code, catalog.currency.decimals, locale)}</small></div>
              <QuantityInput line={line} decimals={product?.quantityDecimals ?? 0} label={t.quantity} onQuantity={onQuantity} />
              <button className="remove" onClick={() => onRemove(line.id)} aria-label={`${t.remove} ${line.name}`}>×</button>
            </div>
          );
        })}
        <div className="cart-total"><span>{t.total}</span><strong>{formatMoney(total, catalog.currency.code, catalog.currency.decimals, locale)}</strong></div>
        <a className={`whatsapp ${currentLines.length ? "" : "disabled"}`} href={currentLines.length ? href : undefined} target="_blank" rel="noreferrer">
          <span aria-hidden="true">↗</span> {t.checkout}
        </a>
      </div>
    </aside>
  );
}

function configuredZanShopUrl(value: string | undefined): string | null {
  if (!value?.trim()) return null;
  try {
    const url = new URL(value.trim());
    return url.protocol === "https:" ? url.toString() : null;
  } catch {
    return null;
  }
}

export function App({ zanShopUrl = DEFAULT_ZANSHOP_URL }: {
  zanShopUrl?: string;
} = {}) {
  const [locale, setLocale] = useState<Locale>("en");
  const [catalog, setCatalog] = useState<Catalog | null>(null);
  const [status, setStatus] = useState<"loading" | "ready" | "error" | "stale">("loading");
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState("all");
  const [copied, setCopied] = useState(false);
  const [cart, setCart] = useState<CartLine[]>(() => safeParse(localStorage.getItem(CART_KEY), []));
  const t = copy(locale);
  const poweredByUrl = configuredZanShopUrl(zanShopUrl);

  const load = useCallback(async () => {
    setStatus("loading");
    try {
      const response = await fetch("/api/catalog");
      if (!response.ok) throw new Error("catalog");
      const next = await response.json() as Catalog;
      setCatalog(next);
      localStorage.setItem(CATALOG_KEY, JSON.stringify(next));
      setStatus("ready");
    } catch {
      const cached = safeParse<Catalog | null>(localStorage.getItem(CATALOG_KEY), null);
      if (cached) { setCatalog(cached); setStatus("stale"); } else { setStatus("error"); }
    }
  }, []);

  useEffect(() => { void load(); }, [load]);
  useEffect(() => {
    document.documentElement.lang = locale;
    document.documentElement.dir = locale === "ar" ? "rtl" : "ltr";
  }, [locale]);
  useEffect(() => { localStorage.setItem(CART_KEY, JSON.stringify(cart)); }, [cart]);

  const products = useMemo(() => catalog?.products.filter((product) => {
    const haystack = `${product.name.en} ${product.name.ar}`.toLocaleLowerCase();
    return (category === "all" || product.categoryId === category) && haystack.includes(query.toLocaleLowerCase());
  }) ?? [], [catalog, category, query]);

  const add = (product: CatalogProduct) => setCart((current) => {
    const found = current.find((line) => line.id === product.id);
    return found
      ? current.map((line) => line.id === product.id ? { ...line, quantity: line.quantity + 1 } : line)
      : [...current, { id: product.id, name: product.name[locale], quantity: 1, priceMinor: product.priceMinor }];
  });
  const setQuantity = (id: string, quantity: number) =>
    setCart((current) => quantity ? current.map((line) => line.id === id ? { ...line, quantity } : line) : current.filter((line) => line.id !== id));
  const share = async () => {
    const data = { title: catalog?.store.name[locale] ?? "ZANPOS", url: location.href };
    if (navigator.share) await navigator.share(data);
    else { await navigator.clipboard.writeText(data.url); setCopied(true); setTimeout(() => setCopied(false), 1800); }
  };

  if (status === "loading") return <main className="state-page"><div className="loader" /><p role="status">{t.loading}</p></main>;
  if (!catalog) return <main className="state-page"><p>{t.error}</p><button onClick={() => void load()}>{t.retry}</button></main>;

  return (
    <div className="app-shell">
      <header>
        <div className="brand"><span className="brand-mark">ز</span><div><b>{catalog.store.name[locale]}</b><small>ZANPOS MARKET</small></div></div>
        <div className="header-actions">
          <button className="quiet" onClick={() => void share()}>{copied ? t.copied : t.share}</button>
          <button className="language" onClick={() => setLocale(locale === "en" ? "ar" : "en")}>{t.language}</button>
        </div>
      </header>
      {status === "stale" && <div className="stale-banner" role="status">{t.stale}<button onClick={() => void load()}>{t.retry}</button></div>}
      <main>
        <section className="hero">
          <p>{t.kicker}</p>
          <h1>{catalog.store.tagline?.[locale] ?? t.hero}</h1>
          <div className="search-wrap">
            <span aria-hidden="true">⌕</span>
            <input type="search" aria-label={t.search} placeholder={t.search} value={query} onChange={(event) => setQuery(event.target.value)} />
          </div>
        </section>
        <nav className="categories" aria-label="Categories">
          <button className={category === "all" ? "active" : ""} onClick={() => setCategory("all")}>{t.all}</button>
          {catalog.categories.map((item) => <button key={item.id} className={category === item.id ? "active" : ""} onClick={() => setCategory(item.id)}>{item.name[locale]}</button>)}
        </nav>
        {products.length ? <section className="product-grid">{products.map((product) =>
          <ProductCard key={product.id} product={product} locale={locale} currency={catalog.currency.code} decimals={catalog.currency.decimals} onAdd={add} />,
        )}</section> : <section className="empty-state"><strong>{t.empty}</strong><span>{t.emptyHint}</span></section>}
      </main>
      <footer className="storefront-footer">
        {poweredByUrl
          ? <a href={poweredByUrl} target="_blank" rel="noreferrer">Powered by ZanShop</a>
          : <span>Powered by ZanShop</span>}
      </footer>
      <CartRibbon lines={cart} catalog={catalog} locale={locale} onQuantity={setQuantity} onRemove={(id) => setCart((value) => value.filter((line) => line.id !== id))} />
    </div>
  );
}
