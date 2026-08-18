import {
  BarChart2, Bell, Building2, LogOut, Power,
  ShoppingBag, ShoppingCart, StickyNote,
} from "lucide-react";
import type { PosStringKey } from "../../i18n/posStrings";

interface Props {
  visible: boolean;
  t: (key: PosStringKey) => string;
  canOpenBackOffice: boolean;
  commerceEnabled: boolean;
  notifCount: number;
  orderCount: number;
  onOpenReport: () => void;
  onOpenOfficeAI: () => void;
  onOpenNotes: () => void;
  onOpenOrders: () => void;
  onOpenNotifications: () => void;
  onCloseShift: () => void;
  onLogout: () => void;
}

/** Badge shared by the orders and alerts buttons — capped so a runaway count
 *  cannot widen the sidebar and push the till layout around. */
function Badge({ count }: { count: number }) {
  if (count <= 0) return null;
  return <span key={count} className="pos-sidebar-badge">{count > 99 ? "99+" : count}</span>;
}

/**
 * The POS icon rail. Purely presentational: every action arrives as a prop, so
 * this renders the same way regardless of what the till is doing. Extracted
 * from PosPage to keep the sale-critical page smaller without moving any of
 * the sale logic itself.
 */
export default function PosSidebar({
  visible, t, canOpenBackOffice, commerceEnabled,
  notifCount, orderCount, onOpenReport,
  onOpenOfficeAI, onOpenNotes, onOpenOrders, onOpenNotifications,
  onCloseShift, onLogout,
}: Props) {
  return (
    <div className={`pos-sidebar${visible ? "" : " pos-sidebar-hidden"}`}>
      <button className="pos-sidebar-item active" aria-label="Quick Sale">
        <ShoppingBag size={18} strokeWidth={1.75} aria-hidden="true" />
        <span>{t("sale")}</span>
      </button>
      <div className="pos-sidebar-divider" />
      <button className="pos-sidebar-item" aria-label="Reports — sales and cash drawer" onClick={onOpenReport}>
        <BarChart2 size={18} strokeWidth={1.75} aria-hidden="true" />
        <span>Reports</span>
      </button>
      {canOpenBackOffice && (
        <button className="pos-sidebar-item" aria-label="Admin" onClick={onOpenOfficeAI}>
          <Building2 size={18} strokeWidth={1.75} aria-hidden="true" />
          <span>Admin</span>
        </button>
      )}
      {canOpenBackOffice && (
        <button className="pos-sidebar-item" aria-label="Notes (admin)" onClick={onOpenNotes}>
          <StickyNote size={18} strokeWidth={1.75} aria-hidden="true" />
          <span>{t("notes")}</span>
        </button>
      )}
      {canOpenBackOffice && commerceEnabled && (
        <button
          className="pos-sidebar-item"
          aria-label={`WhatsApp Orders${orderCount > 0 ? ` (${orderCount} unfulfilled)` : ""}`}
          onClick={onOpenOrders}
        >
          <ShoppingCart size={18} strokeWidth={1.75} aria-hidden="true" />
          <Badge count={orderCount} />
          <span>{t("orders")}</span>
        </button>
      )}
      <div className="pos-sidebar-spacer" />

      {/* ── Bottom group: notifications (admin), close shift, logout ── */}
      {canOpenBackOffice && (
        <button
          className="pos-sidebar-item pos-sidebar-notif"
          aria-label={`Notifications${notifCount > 0 ? ` (${notifCount} unread)` : ""}`}
          onClick={onOpenNotifications}
        >
          <Bell size={18} strokeWidth={1.75} aria-hidden="true" />
          <Badge count={notifCount} />
          <span>{t("alerts")}</span>
        </button>
      )}
      <div className="pos-sidebar-divider" />
      <button className="pos-sidebar-item" aria-label="Close Shift" onClick={onCloseShift}>
        <Power size={18} strokeWidth={1.75} aria-hidden="true" />
        <span>Close</span>
      </button>
      <button
        className="pos-sidebar-item pos-sidebar-item-danger"
        aria-label="Logout"
        title="Ctrl+L"
        onClick={onLogout}
      >
        <LogOut className="icon-directional" size={18} strokeWidth={1.75} aria-hidden="true" />
        <span>Logout</span>
      </button>
    </div>
  );
}
