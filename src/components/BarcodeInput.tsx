import React, { useRef, useEffect, useState } from "react";

interface Props {
  onBarcode: (barcode: string) => void;
  onSearch: (query: string) => void;
  disabled?: boolean;
}

/**
 * Unified barcode/search input.
 * - Barcode scanners emit fast keystrokes ending with Enter.
 * - Keyboard search is slower; debounced to trigger search query.
 * Always autofocused; F2 returns focus here.
 */
export default function BarcodeInput({ onBarcode, onSearch, disabled }: Props) {
  const ref = useRef<HTMLInputElement>(null);
  const [value, setValue] = useState("");
  const lastKeyTime = useRef(0);
  const debounceTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Auto-focus on mount and whenever F2 is pressed anywhere
  useEffect(() => {
    ref.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "F2") { e.preventDefault(); ref.current?.focus(); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const v = e.target.value;
    setValue(v);
    // Debounce search query (not barcode scan)
    if (debounceTimer.current) clearTimeout(debounceTimer.current);
    debounceTimer.current = setTimeout(() => {
      if (v.trim()) onSearch(v.trim());
    }, 300);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    const now = Date.now();
    lastKeyTime.current = now;

    if (e.key === "Enter") {
      e.preventDefault();
      if (value.trim()) {
        onBarcode(value.trim());
        setValue("");
        if (debounceTimer.current) clearTimeout(debounceTimer.current);
      }
    } else if (e.key === "Escape") {
      setValue("");
      if (debounceTimer.current) clearTimeout(debounceTimer.current);
    }
  };

  return (
    <div className="barcode-input-wrap">
      <input
        ref={ref}
        className="barcode-input"
        type="text"
        value={value}
        placeholder="Scan barcode or search (F2)"
        onChange={handleChange}
        onKeyDown={handleKeyDown}
        disabled={disabled}
        autoComplete="off"
        autoCorrect="off"
        spellCheck={false}
      />
    </div>
  );
}
