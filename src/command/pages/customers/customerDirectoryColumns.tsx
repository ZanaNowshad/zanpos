import type { CustomerRow } from "../../../types";
import type { Column } from "../../../components/templates";
import { Award } from "lucide-react";
import { loyaltyState } from "./customerModel";
import type { OperationsStringKey } from "../../../i18n/operationsStrings";

/**
 * Columns for the customer directory.
 *
 * Lifted out of CustomersWorkspace for size. Phone and email render inside
 * `<bdi dir="ltr">` so a Latin-digit number keeps its reading order inside an
 * Arabic page — without it the country code jumps to the wrong end.
 */
export function customerDirectoryColumns(
  t: (key: OperationsStringKey) => string,
): Column<CustomerRow>[] {
  return [
    {
      id: "name",
      header: t("fullName"),
      cell: c => (
        <>
          <span className="zp-cell-primary">{c.name}</span>
          {/* A phone number is a Latin identifier. Without an explicit
              direction the bidi algorithm moves the leading "+" to the end
              in Arabic, rendering "+973 3600 1122" as "1122 3600 973+". */}
          {c.phone
            ? <bdi className="zp-cell-sub" dir="ltr">{c.phone}</bdi>
            : <span className="zp-cell-sub">{t("noPhone")}</span>}
        </>
      ),
    },
    {
      id: "email",
      header: t("email"),
      priority: 3,
      cell: c => c.email ?? <span className="zp-status-muted">—</span>,
    },
    {
      id: "points",
      header: t("loyaltyPoints"),
      align: "end",
      numeric: true,
      width: "130px",
      cell: c => (
        // A balance is not a success state, so it does not borrow the success
        // colour. Accent ties it to the balance shown in the detail pane.
        loyaltyState(c) === "active"
          ? <span className="zp-status zp-cust-points"><Award size={13} aria-hidden="true" />{c.loyalty_points}</span>
          : <span className="zp-status zp-status-muted">0</span>
      ),
    },
    {
      id: "since",
      header: t("customerSince"),
      width: "140px",
      priority: 2,
      cell: c => new Date(c.created_at).toLocaleDateString(),
    },
  ];
}
