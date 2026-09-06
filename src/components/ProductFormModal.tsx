import { useEffect, useMemo, useRef, useState } from "react";
import {
  resolveProductImageValue,
  validateImageUrl,
} from "../productImage";
import type { AdminProduct, CategoryRow, ProductBarcodeRow, TaxRuleRow } from "../types";
import { DEVICE } from "../types";
import { formatMoney, parseMoney } from "../money";
import * as cmd from "../tauri/commands";
import { useLanguage } from "../hooks/useLanguage";
import { modalTranslator } from "../i18n/modalStrings";
import { detailTranslator } from "../i18n/detailStrings";
import { productSchema, useSubmitGuard } from "../forms";
import ModalShell from "./modal/ModalShell";
import {
  Field, ModalActions, ModalColumn, ModalColumns, ModalError, ModalSection, ModalToggle,
} from "./modal/ModalParts";
import { ProductBarcodesSection, ProductImageSection } from "./ProductFormSections";
import MarketPricePanel from "./MarketPricePanel";
import type { SessionToken } from "../types";

interface Props {
  mode: "create" | "edit";
  product?: AdminProduct | null;
  prefilledName?: string;
  prefilledBarcode?: string;
  categories: CategoryRow[];
  taxRules: TaxRuleRow[];
  sessionToken: SessionToken;
  onClose: () => void;
  onSaved: () => void;
}

type PendingBarcode = { tempId: string; barcode: string };

export default function ProductFormModal({
  mode, product, prefilledName, prefilledBarcode,
  categories, taxRules, sessionToken, onClose, onSaved,
}: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => modalTranslator(language), [language]);
  const dt = useMemo(() => detailTranslator(language), [language]);
  const [, tryLock, unlock] = useSubmitGuard();
  const exp = DEVICE.currency_exponent;
  const cur = DEVICE.currency;

  const [name, setName] = useState(product?.name ?? prefilledName ?? "");
  const [categoryId, setCategoryId] = useState(product?.category_id ?? categories[0]?.category_id ?? "");
  const [price, setPrice] = useState(product ? formatMoney(product.price_minor, exp) : "");
  const [sku, setSku] = useState(product?.sku ?? "");
  const [barcode, setBarcode] = useState(product?.barcode ?? prefilledBarcode ?? "");
  const [taxRuleId, setTaxRuleId] = useState(product?.tax_rule_id ?? "");
  const [trackInventory, setTrackInventory] = useState(product?.track_inventory ?? true);
  const [allowDecimal, setAllowDecimal] = useState(product?.allow_decimal_quantity ?? false);
  const [reorderPoint, setReorderPoint] = useState(product?.reorder_point ?? 0);
  const [isActive, setIsActive] = useState(product?.is_active ?? true);
  const [imagePath, setImagePath] = useState(product?.image_path ?? "");
  const [imageUrlInput, setImageUrlInput] = useState("");
  const [imageUrlErr, setImageUrlErr] = useState<string | null>(null);
  const [imageSearchLoading, setImageSearchLoading] = useState(false);
  const [imageSearchError, setImageSearchError] = useState<string | null>(null);
  const [imageSearchSource, setImageSearchSource] = useState<string | null>(null);
  const [costPrice, setCostPrice] = useState("");
  const [markupPct, setMarkupPct] = useState("");
  const [extraBarcodes, setExtraBarcodes] = useState<ProductBarcodeRow[]>([]);
  const [pendingBarcodes, setPendingBarcodes] = useState<PendingBarcode[]>([]);
  const [newBarcodeInput, setNewBarcodeInput] = useState("");
  const [barcodeErr, setBarcodeErr] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const newBarcodeRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (mode === "edit" && product) {
      cmd.productBarcodesList(sessionToken, product.product_id).then(setExtraBarcodes).catch(() => {});
    }
  }, [mode, product, sessionToken]);

  async function pickImage() {
    try {
      const p = await cmd.productPickImage();
      if (p) { setImagePath(p); setImageSearchSource(null); setImageSearchError(null); }
    }
    catch { setError(dt("filePickerFailed")); }
  }

  /** Accept a typed image URL. Validated before it is stored so a bad value is
   *  caught here rather than becoming a silently broken image on every screen
   *  that renders this product. */
  function applyImageUrl() {
    const reason = validateImageUrl(imageUrlInput);
    if (reason === "empty") { setImageUrlErr(null); return; }
    if (reason) {
      setImageUrlErr(reason === "scheme" ? t("imageUrlScheme") : t("imageUrlInvalid"));
      return;
    }
    setImageUrlErr(null);
    setImagePath(imageUrlInput.trim());
    setImageSearchSource(null);
    setImageSearchError(null);
    setImageUrlInput("");
  }

  async function fetchProductImage() {
    setImageSearchLoading(true);
    setImageSearchError(null);
    try {
      const categoryName = categories.find(c => c.category_id === categoryId)?.name;
      const result = await cmd.adminSearchProductImage(sessionToken, {
        productName: name.trim(),
        barcode: barcode.trim() || undefined,
        sku: sku.trim() || undefined,
        categoryName,
        currentImageUrl: imagePath.trim() || undefined,
        mode: imagePath.trim() ? "change" : "fetch",
      });
      setImagePath(result.imageUrl);
      setImageSearchSource(`${t("imageFoundVia")} ${result.source}`);
    } catch (e: unknown) {
      const detail = typeof e === "string" ? e : e instanceof Error ? e.message : "";
      setImageSearchError(detail || t("imageSearchFailed"));
    } finally {
      setImageSearchLoading(false);
    }
  }

  function computeSelling(cost: number, pct: string): number {
    const bp = Math.round(parseFloat(pct || "0") * 100);
    if (cost <= 0 || isNaN(bp) || bp < 0) return 0;
    return Math.round(cost * (10000 + bp) / 10000);
  }

  function addPending() {
    const bc = newBarcodeInput.trim();
    if (!bc) return;
    if (pendingBarcodes.some(p => p.barcode === bc)) { setBarcodeErr(t("alreadyInList")); return; }
    setBarcodeErr(null);
    setPendingBarcodes(p => [...p, { tempId: Math.random().toString(36), barcode: bc }]);
    setNewBarcodeInput("");
    newBarcodeRef.current?.focus();
  }

  async function addBarcodeForEdit() {
    const bc = newBarcodeInput.trim();
    if (!bc || !product) return;
    setBarcodeErr(null);
    try {
      const row = await cmd.productBarcodeAdd(sessionToken, product.product_id, bc);
      setExtraBarcodes(p => [...p, row]);
      setNewBarcodeInput("");
      newBarcodeRef.current?.focus();
    } catch (e: unknown) { setBarcodeErr(typeof e === "string" ? e : t("failed")); }
  }

  async function removeBarcodeForEdit(id: string) {
    try {
      await cmd.productBarcodeRemove(sessionToken, id);
      setExtraBarcodes(p => p.filter(b => b.barcode_id !== id));
    } catch (e: unknown) { setBarcodeErr(typeof e === "string" ? e : t("failed")); }
  }

  async function save() {
    if (!tryLock()) return; // Prevent double-submit
    const resolvedImage = resolveProductImageValue(imagePath, imageUrlInput);
    if (resolvedImage.error) {
      setImageUrlErr(resolvedImage.error === "scheme" ? t("imageUrlScheme") : t("imageUrlInvalid"));
      unlock();
      return;
    }
    const priceMinor = parseMoney(price, exp);
    const parsed = productSchema.safeParse({
      name: name.trim(),
      category: categoryId,
      price_minor: priceMinor,
      barcode: barcode.trim() || undefined,
      sku: sku.trim() || undefined,
    });
    if (!parsed.success) {
      setError(parsed.error.issues[0]?.message ?? t("nameCategoryRequired"));
      unlock();
      return;
    }
    setSaving(true); setError(null);
    try {
      if (mode === "create") {
        const created = await cmd.adminCreateProduct({
          category_id: categoryId, name: name.trim(),
          sku: sku.trim() || undefined, barcode: barcode.trim() || undefined,
          tax_rule_id: taxRuleId || undefined, price_minor: priceMinor,
          track_inventory: trackInventory, allow_decimal_quantity: allowDecimal,
          reorder_point: reorderPoint,
          image_path: resolvedImage.value || undefined,
        }, sessionToken);
        for (const pb of pendingBarcodes) {
          try { await cmd.productBarcodeAdd(sessionToken, created.product_id, pb.barcode); } catch { /* best-effort — extra-barcode failures must not block the save */ }
        }
      } else if (product) {
        await cmd.adminUpdateProduct({
          product_id: product.product_id, category_id: categoryId, name: name.trim(),
          sku: sku.trim() || undefined, barcode: barcode.trim() || undefined,
          tax_rule_id: taxRuleId || undefined, price_minor: priceMinor,
          track_inventory: trackInventory, allow_decimal_quantity: allowDecimal,
          reorder_point: reorderPoint, is_active: isActive,
          image_path: resolvedImage.value || undefined,
        }, sessionToken);
      }
      onSaved();
    } catch (e: unknown) { setError(typeof e === "string" ? e : t("saveFailed")); }
    finally { setSaving(false); unlock(); }
  }

  // Union-typed view: TS cannot .map() over ProductBarcodeRow[] | PendingBarcode[] directly.
  const shownBarcodes: (ProductBarcodeRow | PendingBarcode)[] =
    mode === "edit" ? extraBarcodes : pendingBarcodes;

  return (
    <ModalShell
      kicker="Catalogue"
      title={mode === "create" ? t("newProduct") : t("editProduct")}
      subtitle={mode === "create"
        ? "What it is called, what it costs, and how it rings up at the till."
        : name || undefined}
      size="xl"
      onClose={onClose}
      footer={
        <ModalActions note={<ModalError message={error} />}>
          <button type="button" className="btn-secondary" onClick={onClose}>{t("cancel")}</button>
          <button type="button" className="btn-primary" onClick={save} disabled={saving}>
            {saving ? t("saving") : t("saveProduct")}
          </button>
        </ModalActions>
      }
    >
      <ModalColumns>
        <ModalColumn>
      <ModalSection title="Identity" columns={2}>
        <Field label={t("name")} required wide>
          {id => (
            <input
              id={id}
              value={name}
              placeholder={t("productName")}
              onChange={event => setName(event.target.value)}
            />
          )}
        </Field>
        <Field label={t("category")} required>
          {id => (
            <select id={id} value={categoryId} onChange={event => setCategoryId(event.target.value)}>
              <option value="">- {t("select")} -</option>
              {categories.map(c => <option key={c.category_id} value={c.category_id}>{c.name}</option>)}
            </select>
          )}
        </Field>
        <Field label={t("sku")} hint="Your own reference. Not printed on the item.">
          {id => (
            <input
              id={id}
              value={sku}
              placeholder={t("optional")}
              onChange={event => setSku(event.target.value)}
            />
          )}
        </Field>
      </ModalSection>

      {/* Cost, then markup, then price - the order the number is actually
          worked out in. The old form asked for the selling price first and the
          cost two fields later, so the two inputs that compute the third sat on
          opposite sides of the grid. */}
      <ModalSection title="Pricing" hint="Type a cost and a markup and the price follows." columns={3}>
        <Field label={`${t("cost")} (${cur})`}>
          {id => (
            <input
              id={id}
              type="number"
              inputMode="decimal"
              min="0"
              step={Math.pow(10, -exp).toFixed(exp)}
              value={costPrice}
              placeholder={`0.${"0".repeat(exp)}`}
              onChange={event => {
                setCostPrice(event.target.value);
                const next = computeSelling(parseMoney(event.target.value, exp), markupPct);
                if (next > 0) setPrice(formatMoney(next, exp));
              }}
            />
          )}
        </Field>
        <Field label={t("markupPercent")}>
          {id => (
            <input
              id={id}
              type="number"
              inputMode="decimal"
              min="0"
              step="0.1"
              value={markupPct}
              placeholder="0"
              onChange={event => {
                setMarkupPct(event.target.value);
                const next = computeSelling(parseMoney(costPrice, exp), event.target.value);
                if (next > 0) setPrice(formatMoney(next, exp));
              }}
            />
          )}
        </Field>
        <Field label={`${t("price")} (${cur})`} required>
          {id => (
            <input
              id={id}
              type="number"
              inputMode="decimal"
              min="0"
              step={Math.pow(10, -exp).toFixed(exp)}
              value={price}
              placeholder={`0.${"0".repeat(exp)}`}
              onChange={event => setPrice(event.target.value)}
            />
          )}
        </Field>
        <Field label={t("taxRule")} wide>
          {id => (
            <select id={id} value={taxRuleId} onChange={event => setTaxRuleId(event.target.value)}>
              <option value="">{t("none")}</option>
              {taxRules.map(rule => (
                <option key={rule.tax_rule_id} value={rule.tax_rule_id}>{rule.name}</option>
              ))}
            </select>
          )}
        </Field>
      </ModalSection>

      {/* Only once the product exists: the panel is keyed on a product_id, and
          there is nothing to compare a half-typed name against. Placed directly
          under Pricing because that is the box it fills — it sets the field, not
          the price. Saving still goes through the ordinary path with its RBAC,
          confirmation and audit trail. */}
      {mode === "edit" && product && (
        <ModalSection
          title="Market"
          hint="What other shops charge. Nothing here changes your price until you save."
        >
          <MarketPricePanel
            productId={product.product_id}
            sessionToken={sessionToken}
            onUsePrice={minor => setPrice(formatMoney(minor, exp))}
          />
        </ModalSection>
      )}

      <ModalSection title="Stock" columns={2}>
        <ModalToggle
          label={t("trackInventory")}
          hint="Counts this product in and out. Turn off for services and anything sold loose."
          checked={trackInventory}
          onChange={setTrackInventory}
        />
        <ModalToggle
          label={t("decimalQuantity")}
          hint="Allows 0.5 and 1.25 - for anything weighed rather than counted."
          checked={allowDecimal}
          onChange={setAllowDecimal}
        />
        {trackInventory && (
          <Field label={t("reorderPoint")} hint="Below this, the product shows as low stock.">
            {id => (
              <input
                id={id}
                type="number"
                min="0"
                step="1"
                value={reorderPoint}
                onChange={event => setReorderPoint(parseInt(event.target.value) || 0)}
              />
            )}
          </Field>
        )}
      </ModalSection>

        </ModalColumn>
        <ModalColumn>
      <ProductBarcodesSection
        primary={barcode}
        onPrimaryChange={setBarcode}
        rows={shownBarcodes}
        input={newBarcodeInput}
        onInputChange={setNewBarcodeInput}
        onAdd={() => (mode === "create" ? addPending() : void addBarcodeForEdit())}
        onRemove={row => {
          if ("barcode_id" in row) void removeBarcodeForEdit(row.barcode_id);
          else setPendingBarcodes(list => list.filter(x => x.tempId !== row.tempId));
        }}
        error={barcodeErr}
        inputRef={newBarcodeRef}
        labels={{
          barcode: t("barcode"), additional: t("additionalBarcodes"),
          scanOrType: t("scanOrTypeBarcode"), add: t("add"), optional: t("optional"),
        }}
      />

      <ProductImageSection
        productName={name}
        barcode={barcode}
        sku={sku}
        categoryName={categories.find(c => c.category_id === categoryId)?.name}
        imagePath={imagePath}
        urlInput={imageUrlInput}
        onUrlChange={value => { setImageUrlInput(value); setImageUrlErr(null); }}
        onApplyUrl={applyImageUrl}
        onPick={pickImage}
        onRemove={() => { setImagePath(""); setImageSearchSource(null); }}
        urlError={imageUrlErr}
        loading={imageSearchLoading}
        searchError={imageSearchError}
        source={imageSearchSource}
        onSearch={() => { void fetchProductImage(); }}
        labels={{
          image: t("image"), fetchImage: t("fetchImage"), changeImage: t("changeImage"),
          findingImage: t("findingImage"), imageSearchEvidence: t("imageSearchEvidence"),
          noImage: t("noImage"), choose: t("choose"), remove: t("remove"),
          imageUrl: t("imageUrl"), imageUrlHint: t("imageUrlHint"),
          imageUrlPlaceholder: t("imageUrlPlaceholder"), useUrl: t("useUrl"),
          imageSearchRightsHint: t("imageSearchRightsHint"),
        }}
      />

      {mode === "edit" && (
        <ModalSection title="Visibility">
          <ModalToggle
            label={t("active")}
            hint="An inactive product cannot be sold and is hidden from the till, but its history and stock are kept."
            checked={isActive}
            onChange={setIsActive}
          />
        </ModalSection>
      )}
        </ModalColumn>
      </ModalColumns>
    </ModalShell>
  );
}
