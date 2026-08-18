import type { AdminProduct, StockLevel } from "../types";
import type { Column } from "./templates";
import { formatMoney } from "../money";
import { Check, CircleSlash, Package, TriangleAlert } from "lucide-react";
import { productImageSrc } from "../productImage";
import type { ModalStringKey } from "../i18n/modalStrings";

/**
 * Column definitions for the product catalogue table.
 *
 * Lifted out of ProductsTab for size — the table is the widest in the app and
 * its cells carry most of the presentation logic. Kept a plain function of its
 * four inputs rather than a hook, so the caller keeps ownership of when it is
 * recomputed.
 *
 * `priority` drives the container-query column dropping in datatable.css: the
 * lead column and the row actions never drop, everything else can.
 */
export function productCatalogueColumns(
  t: (key: ModalStringKey) => string,
  cur: string,
  exp: number,
  stockByProduct: Map<string, StockLevel> | null,
): Column<AdminProduct>[] {
  return [
    {
      id: "name",
      header: t("productName"),
      cell: p => (
        <span className="product-catalogue-identity">
          <span className="product-catalogue-thumb">
            <Package size={18} aria-hidden="true" />
            {p.image_path && (
              <img
                src={productImageSrc(p.image_path) ?? ""}
                alt=""
                onError={event => { event.currentTarget.style.display = "none"; }}
              />
            )}
          </span>
          <span>
            <span className="zp-cell-primary">{p.name}</span>
            <span className="zp-cell-sub zp-cell-sub-p2">{p.category_name}</span>
          </span>
        </span>
      ),
    },
    {
      id: "barcode",
      header: t("barcode"),
      priority: 2,
      numeric: true,
      cell: p => p.barcode ?? p.sku ?? <span className="zp-status-muted">—</span>,
    },
    {
      id: "category",
      header: t("category"),
      priority: 2,
      cell: p => p.category_name || <span className="zp-status-muted">—</span>,
    },
    {
      id: "price",
      header: t("price"),
      align: "end",
      numeric: true,
      width: "120px",
      cell: p => `${cur} ${formatMoney(p.price_minor, exp)}`,
    },
    {
      // Cost is commercially sensitive but already visible to any back-office
      // role through admin_list_products and the CSV export, so showing it here
      // exposes nothing new. Priority 3: it is the first column to drop.
      id: "cost",
      header: t("cost"),
      align: "end",
      numeric: true,
      priority: 3,
      width: "120px",
      cell: p => p.cost_minor == null
        ? <span className="zp-status-muted">—</span>
        : `${cur} ${formatMoney(p.cost_minor, exp)}`,
    },
    {
      id: "stock",
      header: t("stock"),
      align: "end",
      numeric: true,
      width: "110px",
      // Priority 1: on a shelf tablet the on-hand number is the whole point of
      // opening this screen, so it outranks the barcode.
      priority: 1,
      cell: p => {
        if (!p.track_inventory) return <span className="zp-status-muted">—</span>;
        if (!stockByProduct) return <span className="zp-status-muted">—</span>;
        const lvl = stockByProduct.get(p.product_id);
        if (!lvl) return <span className="zp-status-muted">—</span>;
        if (lvl.is_out_of_stock) {
          return (
            <span className="zp-status zp-status-danger">
              <CircleSlash size={13} aria-hidden="true" />{lvl.quantity_on_hand}
            </span>
          );
        }
        if (lvl.is_low_stock) {
          return (
            <span className="zp-status zp-status-warn">
              <TriangleAlert size={13} aria-hidden="true" />{lvl.quantity_on_hand}
            </span>
          );
        }
        return lvl.quantity_on_hand;
      },
    },
    {
      id: "status",
      header: t("status"),
      width: "110px",
      priority: 3,
      cell: p => p.is_active
        ? <span className="zp-status zp-status-ok"><Check size={13} aria-hidden="true" />{t("active")}</span>
        : <span className="zp-status zp-status-muted"><CircleSlash size={13} aria-hidden="true" />{t("inactive")}</span>,
    },
  ];
}
