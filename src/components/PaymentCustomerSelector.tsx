import { ChevronDown, ChevronUp, UserRound, X } from "lucide-react";
import type { CustomerRow } from "../types";
import { useLanguage } from "../hooks/useLanguage";
import { countText, operationsTranslator } from "../i18n/operationsStrings";

interface Props {
  selectedCustomer: CustomerRow | null;
  open: boolean;
  search: string;
  results: CustomerRow[];
  showResults: boolean;
  onToggle: () => void;
  onSearchChange: (value: string) => void;
  onSearchBlur: () => void;
  onSelect: (customer: CustomerRow) => void;
  onRemove: () => void;
}

export default function PaymentCustomerSelector({
  selectedCustomer,
  open,
  search,
  results,
  showResults,
  onToggle,
  onSearchChange,
  onSearchBlur,
  onSelect,
  onRemove,
}: Props) {
  const { language } = useLanguage();
  const t = operationsTranslator(language);

  return (
    <div className="pm-section">
      <button
        className={`pm-section-hdr${open ? " pm-section-hdr-open" : ""}${selectedCustomer ? " pm-section-hdr-active" : ""}`}
        onClick={onToggle}
      >
        <span className="pm-section-title">
          <UserRound size={15} /> {selectedCustomer ? selectedCustomer.name.split(" ")[0] : t("customer")}
        </span>
        <span className="pm-chevron">{open ? <ChevronUp size={14} /> : <ChevronDown size={14} />}</span>
      </button>
      {open && (
        <div className="pm-section-body">
          {selectedCustomer ? (
            <div className="pm-cust-chip">
              {selectedCustomer.name}{selectedCustomer.phone ? ` · ${selectedCustomer.phone}` : ""}
              <span className="pm-cust-pts numeric-ltr">
                {countText(language, "points", selectedCustomer.loyalty_points)}
              </span>
              <button className="pm-cust-remove" onClick={onRemove} aria-label={t("removeSelectedCustomer")}>
                <X size={14} />
              </button>
            </div>
          ) : (
            <input
              className="pm-cust-search"
              placeholder={t("searchNameOrPhone")}
              value={search}
              onChange={event => onSearchChange(event.target.value)}
              onBlur={onSearchBlur}
            />
          )}
          {showResults && results.length > 0 && (
            <div className="pm-cust-drop">
              {results.map(customer => (
                <button
                  key={customer.customer_id}
                  className="pm-cust-drop-item"
                  onMouseDown={() => onSelect(customer)}
                >
                  <span>{customer.name}</span>
                  {customer.phone && <span className="pm-cust-dd-phone">{customer.phone}</span>}
                </button>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
