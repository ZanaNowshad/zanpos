import { X } from "lucide-react";
import type { RefObject } from "react";
import type { ProductBarcodeRow } from "../types";
import { Field, ModalSection } from "./modal/ModalParts";
import { ProductImageSearchControl } from "./ProductImageSearchControl";

export type PendingBarcode = { tempId: string; barcode: string };

interface BarcodeProps {
  primary: string;
  onPrimaryChange: (value: string) => void;
  rows: (ProductBarcodeRow | PendingBarcode)[];
  input: string;
  onInputChange: (value: string) => void;
  onAdd: () => void;
  onRemove: (row: ProductBarcodeRow | PendingBarcode) => void;
  error: string | null;
  inputRef: RefObject<HTMLInputElement | null>;
  labels: {
    barcode: string; additional: string; scanOrType: string; add: string; optional: string;
  };
}

/**
 * Every code that rings this product up.
 *
 * One section rather than two: the primary barcode used to sit in the middle of
 * the pricing grid and the extra ones five fields further down, so a manager
 * adding a multipack code had no reason to think the two were related. They are
 * the same question asked twice.
 */
export function ProductBarcodesSection({
  primary, onPrimaryChange, rows, input, onInputChange, onAdd, onRemove, error, inputRef, labels,
}: BarcodeProps) {
  return (
    <ModalSection
      title="Barcodes"
      hint="One product, several codes."
    >
      <Field label={`Primary ${labels.barcode.toLowerCase()}`} hint="The code printed on the item itself.">
        {id => (
          <input
            id={id}
            value={primary}
            placeholder={labels.optional}
            onChange={event => onPrimaryChange(event.target.value)}
          />
        )}
      </Field>

      <Field label={labels.additional} error={error}>
        {id => (
          <div className="zprod-barcode-add">
            <input
              id={id}
              ref={inputRef}
              value={input}
              placeholder={labels.scanOrType}
              onChange={event => onInputChange(event.target.value)}
              onKeyDown={event => {
                if (event.key !== "Enter") return;
                event.preventDefault();
                onAdd();
              }}
            />
            <button type="button" className="btn-secondary" onClick={onAdd}>{labels.add}</button>
          </div>
        )}
      </Field>

      {rows.length > 0 && (
        <ul className="zprod-barcode-list">
          {rows.map(row => {
            const saved = "barcode_id" in row;
            return (
              <li key={saved ? row.barcode_id : row.tempId}>
                <code>{row.barcode}</code>
                <button
                  type="button"
                  onClick={() => onRemove(row)}
                  aria-label={`Remove barcode ${row.barcode}`}
                >
                  <X size={14} aria-hidden="true" />
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </ModalSection>
  );
}

interface ImageProps {
  productName: string;
  barcode: string;
  sku: string;
  categoryName?: string;
  imagePath: string;
  urlInput: string;
  onUrlChange: (value: string) => void;
  onApplyUrl: () => void;
  onPick: () => void;
  onRemove: () => void;
  urlError: string | null;
  loading: boolean;
  searchError: string | null;
  source: string | null;
  onSearch: () => void;
  labels: Record<string, string>;
}

/**
 * The picture, and the three ways to get one.
 *
 * Images also arrive on their own now — a background worker fills in anything
 * left without one — so this section says so rather than leaving a manager to
 * wonder whether an empty box means they have to act.
 */
export function ProductImageSection({
  productName, barcode, sku, categoryName, imagePath, urlInput, onUrlChange,
  onApplyUrl, onPick, onRemove, urlError, loading, searchError, source, onSearch, labels,
}: ImageProps) {
  return (
    <ModalSection
      title={labels.image}
      hint="Left empty, one is found automatically within a few minutes."
    >
      <div className="zprod-image">
        <ProductImageSearchControl
          productName={productName}
          barcode={barcode}
          sku={sku}
          categoryName={categoryName}
          imagePath={imagePath}
          fetchLabel={labels.fetchImage}
          changeLabel={labels.changeImage}
          searchingLabel={labels.findingImage}
          evidenceLabel={labels.imageSearchEvidence}
          noImageLabel={labels.noImage}
          loading={loading}
          error={searchError}
          source={source}
          onSearch={onSearch}
        />
        <div className="zprod-image-actions">
          <button type="button" className="btn-secondary" onClick={onPick}>{labels.choose}</button>
          {imagePath && (
            <button type="button" className="btn-secondary" onClick={onRemove}>{labels.remove}</button>
          )}
        </div>
      </div>

      <Field label={labels.imageUrl} error={urlError} hint={labels.imageUrlHint}>
        {id => (
          <div className="zprod-barcode-add">
            <input
              id={id}
              type="url"
              inputMode="url"
              dir="ltr"
              placeholder={labels.imageUrlPlaceholder}
              value={urlInput}
              aria-invalid={urlError ? true : undefined}
              onChange={event => onUrlChange(event.target.value)}
              onKeyDown={event => {
                if (event.key !== "Enter") return;
                event.preventDefault();
                onApplyUrl();
              }}
              onBlur={onApplyUrl}
            />
            <button type="button" className="btn-secondary" onClick={onApplyUrl}>{labels.useUrl}</button>
          </div>
        )}
      </Field>

      {source && <p className="zmodal-field-hint">{labels.imageSearchRightsHint}</p>}
    </ModalSection>
  );
}
