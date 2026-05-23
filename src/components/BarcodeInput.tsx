import React, { useRef, useEffect, useState, useCallback, forwardRef, useImperativeHandle } from "react";
import { ScanBarcode } from "lucide-react";
import { productSearch } from "../tauri/commands";
import type { ProductWithPrice } from "../types";
import { formatMoney } from "../money";
import { DEVICE } from "../types";

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
  { onBarcode, onSelectProduct, onSearch, onEscape, disabled },
  ref,
) {
  const inputRef    = useRef<HTMLInputElement>(null);
  const dropdownRef = useRef<HTMLDivElement>(null);
  const [value, setValue]           = useState("");
  const [scanState, setScanState]   = useState<"" | "success" | "error">("");
  const [results, setResults]       = useState<ProductWithPrice[]>([]);
  const [highlighted, setHighlighted] = useState(-1);
  const debounceTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const flashTimer    = useRef<ReturnType<typeof setTimeout> | null>(null);

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

  // Auto-focus on mount; F2 / F3 return focus here
  useEffect(() => {
    inputRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "F2" || e.key === "F3") {
        e.preventDefault();
        inputRef.current?.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
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
        const found = await productSearch(v.trim());
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
          return;
        }
      }
      onBarcode(raw);
      setValue(""); closeDropdown();
      if (debounceTimer.current) clearTimeout(debounceTimer.current);
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
          <ScanBarcode size={18} strokeWidth={1.75} />
        </span>
        <input
          ref={inputRef}
          className={`barcode-input${scanState ? ` scan-${scanState}` : ""}`}
          type="text"
          value={value}
          placeholder="Scan barcode or search product…"
          onChange={handleChange}
          onKeyDown={handleKeyDown}
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
