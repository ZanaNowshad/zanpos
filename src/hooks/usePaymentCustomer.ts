import { useEffect, useRef, useState } from "react";
import type { CustomerRow } from "../types";
import { customerList } from "../tauri/commands";

export function usePaymentCustomer(sessionUserId?: string) {
  const [showCust, setShowCust] = useState(false);
  const [custSearch, setCustSearch] = useState("");
  const [custResults, setCustResults] = useState<CustomerRow[]>([]);
  const [selectedCust, setSelectedCust] = useState<CustomerRow | null>(null);
  const [showCustDrop, setShowCustDrop] = useState(false);
  const searchTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (!custSearch.trim()) {
      setCustResults([]);
      return;
    }
    let mounted = true;
    if (searchTimer.current) clearTimeout(searchTimer.current);
    searchTimer.current = setTimeout(async () => {
      const rows = await customerList(sessionUserId ?? "", custSearch.trim()).catch((error: unknown) => {
        console.warn("customerList failed:", error);
        return [] as CustomerRow[];
      });
      if (!mounted) return;
      setCustResults(rows.slice(0, 6));
      setShowCustDrop(true);
    }, 250);
    return () => {
      mounted = false;
      if (searchTimer.current) clearTimeout(searchTimer.current);
    };
  }, [custSearch, sessionUserId]);

  const toggleCustomer = () => setShowCust(value => !value);
  const changeCustomerSearch = (value: string) => {
    setCustSearch(value);
    if (!value) setShowCustDrop(false);
  };
  const blurCustomerSearch = () => window.setTimeout(() => setShowCustDrop(false), 180);
  const selectCustomer = (customer: CustomerRow) => {
    setSelectedCust(customer);
    setCustSearch("");
    setShowCustDrop(false);
  };
  const removeCustomer = () => {
    setSelectedCust(null);
    setCustSearch("");
  };

  return {
    showCust, custSearch, custResults, selectedCust, showCustDrop,
    toggleCustomer, changeCustomerSearch, blurCustomerSearch, selectCustomer, removeCustomer,
  };
}
