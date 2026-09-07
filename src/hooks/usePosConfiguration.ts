import { useEffect, useState } from "react";
import type { BusinessFlags } from "../types";
import { businessFlagsLoad, whatsappOrdersGetEnabled } from "../tauri/commands";
import type { SessionToken } from "../types";

const DEFAULT_FLAGS: BusinessFlags = {
  allow_negative_stock: false,
  require_discount_reason: true,
  cashier_can_discount: false,
  auto_print_receipt: false,
};

export function usePosConfiguration(sessionToken: SessionToken) {
  const [businessFlags, setBusinessFlags] = useState(DEFAULT_FLAGS);
  const [commerceEnabled, setCommerceEnabled] = useState(false);

  useEffect(() => {
    let cancelled = false;
    businessFlagsLoad()
      .then(flags => { if (!cancelled) setBusinessFlags(flags); })
      .catch((cause: unknown) => console.warn("businessFlagsLoad failed:", cause));
    whatsappOrdersGetEnabled(sessionToken)
      .then(enabled => { if (!cancelled) setCommerceEnabled(enabled); })
      .catch(() => { if (!cancelled) setCommerceEnabled(false); });
    return () => { cancelled = true; };
  }, [sessionToken]);

  return { businessFlags, setBusinessFlags, commerceEnabled } as const;
}
