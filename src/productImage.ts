import { convertFileSrc } from "@tauri-apps/api/core";

/**
 * A product image is stored in one field, `image_path`, which may hold either a
 * local file path chosen through the picker or a remote URL typed by the user.
 * The two need different treatment at render time: a local path has to go
 * through Tauri's asset protocol, and a URL must be passed straight through —
 * running a URL through `convertFileSrc` mangles it into an unloadable string.
 */

/** Only http(s) is accepted. `data:` and `blob:` would let a pasted string
 *  smuggle arbitrary bytes into the catalogue, and `file:` would read from disk
 *  outside the picker's control — neither belongs in a field a manager types
 *  into. Anything else is treated as a local path. */
export function isRemoteImage(value: string): boolean {
  const trimmed = value.trim();
  if (!/^https?:\/\//i.test(trimmed)) return false;
  try {
    const url = new URL(trimmed);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}

/** Resolve a stored `image_path` to something an `<img src>` can load. */
export function productImageSrc(value: string | null | undefined): string | null {
  const trimmed = value?.trim();
  if (!trimmed) return null;
  return isRemoteImage(trimmed) ? trimmed : convertFileSrc(trimmed);
}

export type ImageUrlError = "empty" | "scheme" | "malformed" | null;
export type ProductImageValue = {
  value: string;
  error: Exclude<ImageUrlError, "empty" | null>;
} | {
  value: string;
  error: null;
};

/**
 * Validate a URL a user typed. Returns the reason it was rejected so the caller
 * can show a specific message rather than a generic "invalid".
 */
export function validateImageUrl(raw: string): ImageUrlError {
  const trimmed = raw.trim();
  if (!trimmed) return "empty";
  let url: URL;
  try {
    url = new URL(trimmed);
  } catch {
    return "malformed";
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") return "scheme";
  if (!url.hostname) return "malformed";
  return null;
}

/** Resolve the value that should be saved by the product form. The pending URL
 * is read again at submit time because a click on Save can run before React has
 * committed the state update triggered by the URL input's blur event. */
export function resolveProductImageValue(
  selectedValue: string,
  pendingUrl: string,
): ProductImageValue {
  const selected = selectedValue.trim();
  const pending = pendingUrl.trim();
  if (!pending) return { value: selected, error: null };

  const error = validateImageUrl(pending);
  if (error && error !== "empty") return { value: selected, error };
  return { value: pending, error: null };
}

/** A failed preview only suppresses the URL/path that actually failed. Choosing
 * a replacement makes the preview eligible to load immediately. */
export function shouldShowProductImage(
  value: string | null | undefined,
  failedValue: string | null,
): boolean {
  const current = value?.trim();
  return Boolean(current && current !== failedValue);
}
