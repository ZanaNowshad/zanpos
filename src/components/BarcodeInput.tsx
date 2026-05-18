import React, { useRef, useEffect, useState, forwardRef, useImperativeHandle } from "react";

export interface BarcodeInputHandle {
  focus: () => void;
  clear: () => void;
}

interface Props {
  /** Called when Enter is pressed. `qty` is parsed from a "N*barcode" prefix. */
  onBarcode: (barcode: string, qty?: number) => void;
  onSearch: (query: string) => void;
  onEscape?: () => void;
  disabled?: boolean;
}

/**
 * Unified barcode/search input.
 * - Barcode scanners emit fast keystrokes ending with Enter.
 * - Keyboard search is slower; debounced at 300ms.
 * - Supports quantity prefix: "3*123456" or "3x123456" → qty=3, barcode=123456.
 * - F2 always returns focus here; expose .focus() via ref for programmatic use.
 */
const BarcodeInput = forwardRef<BarcodeInputHandle, Props>(function BarcodeInput(
  { onBarcode, onSearch, onEscape, disabled },
  ref,
) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [value, setValue] = useState("");
  const debounceTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useImperativeHandle(ref, () => ({
    focus: () => inputRef.current?.focus(),
    clear: () => setValue(""),
  }));

  // Auto-focus on mount; F2 / F3 always return focus here
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

  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const v = e.target.value;
    setValue(v);
    if (debounceTimer.current) clearTimeout(debounceTimer.current);
    debounceTimer.current = setTimeout(() => {
      if (v.trim()) onSearch(v.trim());
      else onSearch("");
    }, 300);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
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
          setValue("");
          if (debounceTimer.current) clearTimeout(debounceTimer.current);
          return;
        }
      }

      onBarcode(raw);
      setValue("");
      if (debounceTimer.current) clearTimeout(debounceTimer.current);
    } else if (e.key === "Escape") {
      e.preventDefault();
      setValue("");
      if (debounceTimer.current) clearTimeout(debounceTimer.current);
      onSearch("");
      onEscape?.();
    }
  };

  return (
    <div className="barcode-input-wrap">
      <span className="barcode-input-icon">▮▮▮</span>
      <input
        ref={inputRef}
        className="barcode-input"
        type="text"
        value={value}
        placeholder="Scan barcode or search product · qty prefix: 3*barcode (F2)"
        onChange={handleChange}
        onKeyDown={handleKeyDown}
        disabled={disabled}
        autoComplete="off"
        autoCorrect="off"
        spellCheck={false}
      />
      <span className="barcode-input-kbd">F2</span>
    </div>
  );
});

export default BarcodeInput;
