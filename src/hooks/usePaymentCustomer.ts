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
      // The picker only ever shows six, so ask the server for six rather than
      // fetching a page and discarding most of it.
      const page = await customerList(sessionUserId ?? "", custSearch.trim(), 0, 6)
        .catch((error: unknown) => {
          console.warn("customerList failed:", error);
          return { items: [] as CustomerRow[], total: 0, offset: 0, limit: 6 };
        });
      if (!mounted) return;
      setCustResults(page.items);
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
