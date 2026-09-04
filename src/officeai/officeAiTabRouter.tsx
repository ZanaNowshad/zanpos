import type { ReactNode } from "react";
import type { ProductPrefill, SessionUser } from "../types";
import type { OfficeTab } from "./officeAiTypes";
import type { SettingsSection } from "../command/pages/settings/SettingsShell";

import ProductsTab from "../components/ProductsTab";
import CategoriesTab from "../components/CategoriesTab";
import QuickPosTab from "../command/pages/catalogue/QuickPosTab";
import UsersTab from "../components/UsersTab";
import ReportsTab from "../components/ReportsTab";
import InventoryTab from "../components/InventoryTab";
import CustomersWorkspace from "../command/pages/customers/CustomersWorkspace";
import CommandSettingsPage from "../command/pages/settings/CommandSettingsPage";
import ZanShopPage from "../command/pages/ZanShopPage";
import RidersPage from "../command/pages/team/RidersPage";
import AuditLogTab from "../components/AuditLogTab";
import DevicesTab from "../components/DevicesTab";
import CashierReportTab from "../components/CashierReportTab";
import EodCashupTab from "../components/EodCashupTab";
import DeliveriesTab from "../components/DeliveriesTab";
import OfficeAIPurchasingWorkspace from "./OfficeAIPurchasingWorkspace";
import { DEVICE } from "../types";

export interface DataTabContext {
  tab: OfficeTab;
  /** Remount key — bumped after an AI mutation so a tab reloads its data. */
  tabKey: string;
  sessionUser: SessionUser;
  isOwner: boolean;
  isManager: boolean;
  productPrefill: ProductPrefill | null;
  /** Settings opens on Advanced when the page was entered for maintenance. */
  maintenancePane?: boolean;
  settingsSection?: SettingsSection;
  /** Hands a prompt to the assistant — Purchasing offers "ask ZanAI" actions. */
  onSendPrompt: (prompt: string) => void;
  onPrefillConsumed: () => void;
  onOpenTab: (tab: OfficeTab) => void;
  onStartPractice?: () => void;
}

/**
 * Which workspace a tab renders.
 *
 * Split out of OfficeAIPage for size. It is a pure lookup — every branch reads
 * from the context it is handed and none of them touch page state — so keeping
 * it beside the page rather than inside it costs nothing and leaves the page
 * about its own concerns: navigation, the assistant, and the shell.
 */
export function renderDataTab(ctx: DataTabContext): ReactNode {
  const { tab, tabKey, sessionUser, isOwner, isManager, productPrefill } = ctx;
  switch (tab) {
      case "operations": return <ProductsTab key={tabKey} sessionUserId={sessionUser.user_id} prefill={productPrefill} onPrefillConsumed={() => ctx.onPrefillConsumed()} />;
      case "products": return <ProductsTab key={tabKey} sessionUserId={sessionUser.user_id} prefill={productPrefill} onPrefillConsumed={() => ctx.onPrefillConsumed()} />;
      case "categories": return <CategoriesTab key={tabKey} sessionUserId={sessionUser.user_id} />;
      case "quickpos": return <QuickPosTab key={tabKey} sessionUserId={sessionUser.user_id} />;
      case "users": return <UsersTab key={tabKey} sessionUserId={sessionUser.user_id} sessionToken={sessionUser.session_token} sessionRole={sessionUser.role_name} />;
      case "riders": return <RidersPage key={tabKey} actorUserId={sessionUser.user_id} />;
      case "reports": return <ReportsTab key={tabKey} sessionUserId={sessionUser.user_id} />;
      case "cashier": return <CashierReportTab key={tabKey} sessionUserId={sessionUser.user_id} />;
      case "eod": return <EodCashupTab key={tabKey} sessionUserId={sessionUser.user_id} />;
      case "inventory": return <InventoryTab key={tabKey} sessionUserId={sessionUser.user_id} />;
      case "customers": return (
        <CustomersWorkspace
          key={tabKey}
          actorUserId={sessionUser.user_id}
          canAdjustLoyalty={isManager}
        />
      );
      case "settings": return (
        <CommandSettingsPage
          key={tabKey}
          sessionUserId={sessionUser.user_id}
          sessionToken={sessionUser.session_token}
          sessionRole={sessionUser.role_name}
          initialSection={(ctx.maintenancePane ? "advanced" : "store") as SettingsSection}
          onOpenTab={(next) => ctx.onOpenTab(next as OfficeTab)}
          onStartPractice={ctx.onStartPractice}
        />
      );
      case "audit": return isOwner ? <AuditLogTab key={tabKey} sessionUserId={sessionUser.user_id} /> : null;
      case "devices": return isOwner ? <DevicesTab key={tabKey} sessionUserId={sessionUser.user_id} /> : null;
      case "deliveries": return <DeliveriesTab key={tabKey} sessionUser={sessionUser} />;
      case "purchasing": return (
        <OfficeAIPurchasingWorkspace
          key={tabKey}
          actorUserId={sessionUser.user_id}
          currencyExp={DEVICE.currency_exponent}
          onSendPrompt={ctx.onSendPrompt}
        />
      );
      case "zanshop": return (
        <ZanShopPage
          key={tabKey}
          sessionUserId={sessionUser.user_id}
          onOpenSettings={() => ctx.onOpenTab("settings")}
        />
      );
    default: return null;
  }
}
