import React, { useRef, useEffect, useState, useCallback, forwardRef, useImperativeHandle } from "react";
import { ScanBarcode } from "lucide-react";
import { productSearch } from "../tauri/commands";
import type { ProductWithPrice } from "../types";
import { formatMoney } from "../money";
import { DEVICE } from "../types";
import type { SessionToken } from "../types";

export interface BarcodeInputHandle {
  focus: () => void;
  clear: () => void;
  flashSuccess: () => void;
  flashError: () => void;
}

interface Props {
  /** Called when Enter is pressed with no search dropdown open — barcode scan path. */
  onBarcode: (barcode: string, qty?: number) => void;
  /** Called when user clicks (or presses Enter on) a search result. */
  onSelectProduct?: (product: ProductWithPrice) => void;
  onSearch: (query: string) => void;
  onEscape?: () => void;
  disabled?: boolean;
  /** Active user ID for RBAC on product search. */
  sessionToken: SessionToken;
}

/**
 * Unified barcode scanner + product search input.
 *
 * Barcode scanners fire keystrokes very fast and press Enter immediately —
 * the 300ms search debounce never fires, so the dropdown never appears.
 * Human typing triggers the debounce, calls productSearch, and shows a
 * dropdown. Arrow keys navigate; Enter / click selects.
 */
const BarcodeInput = forwardRef<BarcodeInputHandle, Props>(function BarcodeInput(
  { onBarcode, onSelectProduct, onSearch, onEscape, disabled, sessionToken },
  ref,
) {
  const inputRef    = useRef<HTMLInputElement>(null);
  const dropdownRef = useRef<HTMLDivElement>(null);
  const [value, setValue]           = useState("");
  const [scanState, setScanState]   = useState<"" | "success" | "error">("");
  const [results, setResults]       = useState<ProductWithPrice[]>([]);
  const [highlighted, setHighlighted] = useState(-1);
  const debounceTimer  = useRef<ReturnType<typeof setTimeout> | null>(null);
  const flashTimer     = useRef<ReturnType<typeof setTimeout> | null>(null);
  const refocusTimer   = useRef<ReturnType<typeof setTimeout> | null>(null);

  const showDropdown = results.length > 0;

  const clearFlash = useCallback(() => {
    if (flashTimer.current) clearTimeout(flashTimer.current);
    flashTimer.current = setTimeout(() => setScanState(""), 600);
  }, []);

  const closeDropdown = useCallback(() => {
    setResults([]);
    setHighlighted(-1);
  }, []);

  useImperativeHandle(ref, () => ({
    focus: () => inputRef.current?.focus(),
    clear: () => { setValue(""); closeDropdown(); },
    flashSuccess: () => { setScanState("success"); clearFlash(); },
    flashError:   () => { setScanState("error");   clearFlash(); },
  }));

  // Auto-focus on mount; F2 / F3 and window re-focus return cursor here
  useEffect(() => {
    inputRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "F2" || e.key === "F3") {
        e.preventDefault();
        inputRef.current?.focus();
      }
    };

    // When the Tauri app window itself regains focus (e.g. user Alt-Tabs back),
    // put the cursor straight back on the barcode field unless a modal has it.
    const onWindowFocus = () => {
      const active = document.activeElement as HTMLElement | null;
      if (active && (
        active.tagName === "INPUT" || active.tagName === "TEXTAREA" || active.tagName === "SELECT" ||
        active.isContentEditable ||
        active.closest(".modal-overlay, .payment-modal, [role='dialog'], .mig-page, .bulk-modal, .login-screen, .lock-screen")
      )) return;
      inputRef.current?.focus();
    };

    window.addEventListener("keydown", onKey);
    window.addEventListener("focus", onWindowFocus);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("focus", onWindowFocus);
    };
  }, []);

  // Timer cleanup on unmount
  useEffect(() => {
    return () => {
      if (flashTimer.current) clearTimeout(flashTimer.current);
      if (refocusTimer.current) clearTimeout(refocusTimer.current);
      if (debounceTimer.current) clearTimeout(debounceTimer.current);
    };
  }, []);

  // Re-focus when the input transitions from disabled → enabled.
  // Without this, calling .focus() in the scan handler's finally block
  // races with React flushing setLoading(false) to the DOM — the call
  // lands while the <input> is still disabled and silently no-ops.
  // This effect fires AFTER the DOM has been updated, so the input is
  // guaranteed to be enabled before focus() is called.
  useEffect(() => {
    if (!disabled) inputRef.current?.focus();
  }, [disabled]);

  // Aggressive refocus — 100 ms after blur, steal focus back unless a modal
  // or another real input field has taken it (payment, discount, custom-item, etc.)
  const handleBlur = useCallback(() => {
    if (refocusTimer.current) clearTimeout(refocusTimer.current);
    refocusTimer.current = setTimeout(() => {
      if (disabled) return;
      const active = document.activeElement as HTMLElement | null;
      if (active) {
        const tag = active.tagName;
        // Leave alone if another real input/select/textarea has focus
        if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
        if (active.isContentEditable) return;
        // Leave alone if inside any modal overlay or dialog
        if (active.closest(
          ".modal-overlay, .payment-modal, .bo-overlay, [role='dialog'], dialog, " +
          ".mig-page, .bulk-modal, .login-screen, .lock-screen"
        )) return;
      }
      inputRef.current?.focus();
    }, 100);
  }, [disabled]);

  // Cancel pending refocus when we voluntarily re-enter the field
  const handleFocus = useCallback(() => {
    if (refocusTimer.current) clearTimeout(refocusTimer.current);
  }, []);

  // Close dropdown on outside click
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (
        !inputRef.current?.contains(e.target as Node) &&
        !dropdownRef.current?.contains(e.target as Node)
      ) closeDropdown();
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [closeDropdown]);

  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const v = e.target.value;
    setValue(v);
    setScanState("");
    setHighlighted(-1);
    if (debounceTimer.current) clearTimeout(debounceTimer.current);
    if (!v.trim()) { setResults([]); onSearch(""); return; }
    debounceTimer.current = setTimeout(async () => {
      try {
        const found = await productSearch(sessionToken, v.trim());
        setResults(found.slice(0, 8));
      } catch {
        setResults([]);
      }
      onSearch(v.trim());
    }, 300);
  };

  const selectProduct = useCallback((product: ProductWithPrice) => {
    onSelectProduct?.(product);
    setValue("");
    closeDropdown();
    if (debounceTimer.current) clearTimeout(debounceTimer.current);
    onSearch("");
  }, [onSelectProduct, closeDropdown, onSearch]);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    // Dropdown navigation
    if (showDropdown) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setHighlighted(h => Math.min(h + 1, results.length - 1));
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setHighlighted(h => Math.max(h - 1, 0));
        return;
      }
      if (e.key === "Enter") {
        e.preventDefault();
        // Select highlighted row, or first row if none highlighted
        const idx = highlighted >= 0 ? highlighted : 0;
        selectProduct(results[idx]);
        return;
      }
    }

    if (e.key === "Enter") {
      e.preventDefault();
      const raw = value.trim();
      if (!raw) return;
      // Quantity prefix: "3*barcode" or "3x123456"
      const prefixMatch = raw.match(/^(\d+)[*x×](.+)$/i);
      if (prefixMatch) {
        const qty = Math.max(1, Math.min(9999, parseInt(prefixMatch[1], 10)));
        const barcode = prefixMatch[2].trim();
        if (barcode) {
          onBarcode(barcode, qty);
          setValue(""); closeDropdown();
          if (debounceTimer.current) clearTimeout(debounceTimer.current);
          // Keep focus on the input immediately — the async handler in the parent
          // will also call focus() after it resolves, but this fires right away.
          inputRef.current?.focus();
          return;
        }
      }
      onBarcode(raw);
      setValue(""); closeDropdown();
      if (debounceTimer.current) clearTimeout(debounceTimer.current);
      // Re-assert focus immediately so the scanner is ready for the next item.
      inputRef.current?.focus();
    } else if (e.key === "Escape") {
      e.preventDefault();
      setValue(""); closeDropdown(); setScanState("");
      if (debounceTimer.current) clearTimeout(debounceTimer.current);
      onSearch("");
      onEscape?.();
    }
  };

  return (
    <div className="barcode-input-wrap">
      <div className="barcode-input-row">
        <span className="barcode-input-icon">
          <ScanBarcode size={16} strokeWidth={1.75} />
        </span>
        <input
          ref={inputRef}
          className={`barcode-input${scanState ? ` scan-${scanState}` : ""}`}
          type="text"
          value={value}
          placeholder="Scan barcode or search product…"
          onChange={handleChange}
          onKeyDown={handleKeyDown}
          onBlur={handleBlur}
          onFocus={handleFocus}
          disabled={disabled}
          autoComplete="off"
          autoCorrect="off"
          spellCheck={false}
        />
        <kbd className="barcode-input-kbd">F2</kbd>
      </div>

      {showDropdown && (
        <div className="barcode-dropdown" ref={dropdownRef}>
          {results.map((p, i) => (
            <button
              key={p.product_id}
              className={`barcode-dropdown-item${highlighted === i ? " barcode-dropdown-item-active" : ""}`}
              onMouseDown={e => { e.preventDefault(); selectProduct(p); }}
              onMouseEnter={() => setHighlighted(i)}
            >
              <span className="bdi-name">{p.name}</span>
              <span className="bdi-meta">
                {p.sku && <span className="bdi-sku">{p.sku}</span>}
                <span className="bdi-price">{DEVICE.currency} {formatMoney(p.price_minor, DEVICE.currency_exponent)}</span>
              </span>
            </button>
          ))}
        </div>
      )}

      <p className="barcode-input-hint">Tip: type <code>3*barcode</code> to add a quantity</p>
    </div>
  );
});

export default BarcodeInput;
