import { useCallback, useEffect, useRef, useState } from "react";
import type { useCart } from "./useCart";
import { applyDialpadKey } from "../components/Dialpad";

export function usePosNumpad(
  recentLineId: string | null,
  updateQuantity: ReturnType<typeof useCart>["updateQuantity"],
) {
  const [numpadValue, setNumpadValue] = useState("1");
  const numpadRef = useRef(numpadValue);
  useEffect(() => { numpadRef.current = numpadValue; }, [numpadValue]);

  const handleNumpadKey = useCallback((key: string) => {
    if (key === "C") {
      setNumpadValue("1");
      return;
    }
    const next = applyDialpadKey(
      numpadRef.current === "1" && key !== "⌫" ? "" : numpadRef.current,
      key,
    );
    setNumpadValue(next === "" ? "1" : next);
    if (recentLineId && next !== "" && next !== "0") {
      void updateQuantity(recentLineId, next);
    }
  }, [recentLineId, updateQuantity]);

  return { numpadValue, setNumpadValue, handleNumpadKey } as const;
}
