import { useRef, useState } from "react";
import type { AdminProduct } from "../types";
import * as cmd from "../tauri/commands";
import type { SessionToken } from "../types";

export interface BulkImageProgress { done: number; total: number; failed: number }

/**
 * Catalogue image search — one product, or every product on the page that has
 * none.
 *
 * Lifted out of ProductsTab for size, but the seam is a real one: this owns the
 * per-row busy/error state and the bulk run's cancellation, and the component
 * only needs to render them.
 */
export function useProductImageFetch(options: {
  sessionToken: SessionToken;
  /** The page the bulk run works over. */
  products: AdminProduct[];
  failureLabel: string;
  /** Called with the saved URL so the caller can update its own row state. */
  onImageSaved: (productId: string, imageUrl: string) => void;
}) {
  const [imageSearchState, setImageSearchState] = useState<Record<string, {
    loading: boolean;
    error?: string;
  }>>({});
  const [bulkImages, setBulkImages] = useState<BulkImageProgress | null>(null);
  const bulkCancelRef = useRef(false);

/*
 * Fetch images for everything on this page that has none.
 *
 * Page-scoped, not catalogue-scoped: each product is a separate call out to
 * the image search, and firing one per row across a catalogue of thousands
 * would be a long unattended run against someone else's rate limit. The page
 * is the unit the manager is looking at, and the filters above already
 * narrow it.
 *
 * Sequential for the same reason, and stoppable — an operator who sees it
 * picking wrong images should not have to wait for the run to finish.
 */
  async function fetchMissingImages() {
  const targets = options.products.filter(product => !product.image_path?.trim() && product.barcode?.trim());
  if (targets.length === 0) return;

  bulkCancelRef.current = false;
  setBulkImages({ done: 0, total: targets.length, failed: 0 });
  let failed = 0;

  for (const [index, product] of targets.entries()) {
    if (bulkCancelRef.current) break;
    // fetchProductImage records its own per-row error state rather than
    // throwing, so the tally comes from its result, not a catch.
    if (!(await fetchProductImage(product))) failed += 1;
    setBulkImages({ done: index + 1, total: targets.length, failed });
  }

  // Hold the finished tally on screen so the outcome is readable, then clear.
  setTimeout(() => setBulkImages(null), failed > 0 ? 8000 : 2500);
}

/** Resolves true when an image was found and saved. */
  async function fetchProductImage(product: AdminProduct): Promise<boolean> {
  setImageSearchState(current => ({
    ...current,
    [product.product_id]: { loading: true },
  }));
  try {
    const result = await cmd.adminSearchProductImage(options.sessionToken, {
      productName: product.name,
      barcode: product.barcode ?? undefined,
      sku: product.sku ?? undefined,
      categoryName: product.category_name,
      currentImageUrl: product.image_path ?? undefined,
      mode: product.image_path ? "change" : "fetch",
    });
    await cmd.adminSetProductImage(options.sessionToken, product.product_id, result.imageUrl);
      options.onImageSaved(product.product_id, result.imageUrl);
    setImageSearchState(current => ({ ...current, [product.product_id]: { loading: false } }));
    return true;
  } catch (e: unknown) {
    const detail = typeof e === "string" ? e : e instanceof Error ? e.message : "";
    setImageSearchState(current => ({
      ...current,
      [product.product_id]: { loading: false, error: detail || options.failureLabel },
    }));
    return false;
  }
}

  return {
    imageSearchState,
    bulkImages,
    fetchProductImage,
    fetchMissingImages,
    stopBulk: () => { bulkCancelRef.current = true; },
  };
}
