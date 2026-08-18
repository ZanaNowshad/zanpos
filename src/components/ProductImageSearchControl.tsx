import { ImagePlus, LoaderCircle, RefreshCw } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { productImageSrc, shouldShowProductImage } from "../productImage";

interface Props {
  productName: string;
  barcode?: string | null;
  sku?: string | null;
  categoryName?: string | null;
  imagePath?: string | null;
  fetchLabel: string;
  changeLabel: string;
  searchingLabel: string;
  evidenceLabel: string;
  noImageLabel: string;
  source?: string | null;
  error?: string | null;
  loading?: boolean;
  compact?: boolean;
  showPreview?: boolean;
  onSearch: () => void;
}

/** Shared catalogue/form control so the same image-search behavior is visible
 * wherever a product can be created or edited. */
export function ProductImageSearchControl({
  productName,
  barcode,
  sku,
  categoryName,
  imagePath,
  fetchLabel,
  changeLabel,
  searchingLabel,
  evidenceLabel,
  noImageLabel,
  source,
  error,
  loading = false,
  compact = false,
  showPreview = true,
  onSearch,
}: Props) {
  const [failedImagePath, setFailedImagePath] = useState<string | null>(null);
  useEffect(() => setFailedImagePath(null), [imagePath]);

  const hasImage = shouldShowProductImage(imagePath ?? "", failedImagePath);
  const evidence = useMemo(
    () => [barcode?.trim(), productName.trim(), categoryName?.trim(), sku?.trim()].filter(Boolean).join(" · "),
    [barcode, productName, categoryName, sku],
  );
  const canSearch = Boolean(productName.trim() && barcode?.trim());
  const actionLabel = hasImage || Boolean(imagePath?.trim()) ? changeLabel : fetchLabel;

  return (
    <div className={`product-image-search${compact ? " product-image-search--compact" : ""}`}>
      <div className="product-image-search__proof">
        {showPreview && (hasImage ? (
          <img
            className="product-image-search__thumbnail"
            src={productImageSrc(imagePath ?? "") ?? ""}
            alt={productName || noImageLabel}
            onError={() => setFailedImagePath(imagePath?.trim() ?? null)}
          />
        ) : (
          <div className="product-image-search__placeholder" aria-label={noImageLabel}>
            <ImagePlus size={compact ? 18 : 24} aria-hidden="true" />
          </div>
        ))}
        <button
          className="btn-secondary product-image-search__action"
          type="button"
          onClick={onSearch}
          disabled={!canSearch || loading}
          aria-label={`${actionLabel}${productName ? `: ${productName}` : ""}`}
        >
          {/* The label is wrapped so a narrow table can hide it and leave the
              icon; `aria-label` above carries the meaning either way. */}
          {loading ? (
            <><LoaderCircle className="product-image-search__spinner" size={15} aria-hidden="true" /><span className="zp-action-label">{searchingLabel}</span></>
          ) : hasImage || Boolean(imagePath?.trim()) ? (
            <><RefreshCw size={15} aria-hidden="true" /><span className="zp-action-label">{changeLabel}</span></>
          ) : (
            <><ImagePlus size={15} aria-hidden="true" /><span className="zp-action-label">{fetchLabel}</span></>
          )}
        </button>
      </div>
      {!compact && evidence && (
        <div className="product-image-search__evidence">
          <span>{evidenceLabel}</span>
          <strong>{evidence}</strong>
        </div>
      )}
      {!compact && source && <div className="product-image-search__source">{source}</div>}
      {error && <div className="product-image-search__error" role="alert">{error}</div>}
    </div>
  );
}
