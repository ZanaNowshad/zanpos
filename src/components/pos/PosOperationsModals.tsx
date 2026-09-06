import type { Dispatch, SetStateAction } from "react";
import type { Cart, SaleListRow, SessionUser, Shift } from "../../types";
import CashEventModal from "../CashEventModal";
import HelpModal from "../HelpModal";
import HoldModal from "../HoldModal";
import RecentSalesModal from "../RecentSalesModal";
import RefundModal from "../RefundModal";
import ShiftModal from "../ShiftModal";
import TodayReportModal from "../TodayReportModal";
import type { ActiveModal, ExchangeCredit } from "./posModalState";

interface Props {
  activeModal: ActiveModal;
  setActiveModal: Dispatch<SetStateAction<ActiveModal>>;
  sessionUser: SessionUser;
  shift: Shift;
  cart: Cart;
  lineCount: number;
  netTotal: number;
  clearCart: () => void;
  replaceCart: (cart: Cart) => void;
  setExchangeCredit: Dispatch<SetStateAction<ExchangeCredit | null>>;
  onShiftClose: (closed: boolean) => void;
  onReprintReceipt: (receiptNumber: string) => Promise<void>;
  onEditSale: (sale: SaleListRow) => Promise<void>;
  focusBarcode: () => void;
}

export default function PosOperationsModals({
  activeModal,
  setActiveModal,
  sessionUser,
  shift,
  cart,
  lineCount,
  netTotal,
  clearCart,
  replaceCart,
  setExchangeCredit,
  onShiftClose,
  onReprintReceipt,
  onEditSale,
  focusBarcode,
}: Props) {
  const close = () => {
    setActiveModal({ kind: "none" });
    focusBarcode();
  };

  return (
    <>
      {activeModal.kind === "cashEvent" && (
        <CashEventModal
          shiftId={shift.shift_id}
          sessionToken={sessionUser.session_token}
          userId={sessionUser.user_id}
          cashierName={sessionUser.display_name}
          onDone={close}
          onCancel={close}
        />
      )}

      {activeModal.kind === "shiftClose" && (
        <ShiftModal
          mode="close"
          user={sessionUser}
          shift={shift}
          onShiftOpened={() => {}}
          onShiftClosed={() => {
            setActiveModal({ kind: "none" });
            onShiftClose(true);
          }}
          onCancel={close}
        />
      )}

      {activeModal.kind === "hold" && (
        <HoldModal
          cart={cart}
          lineCount={lineCount}
          netTotal={netTotal}
          sessionToken={sessionUser.session_token}
          onHeld={() => {
            setActiveModal({ kind: "none" });
            clearCart();
            focusBarcode();
          }}
          onResume={resumed => {
            setActiveModal({ kind: "none" });
            replaceCart(resumed);
            focusBarcode();
          }}
          onClose={close}
        />
      )}

      {activeModal.kind === "refund" && (
        <RefundModal
          cashierUserId={sessionUser.user_id}
          sessionToken={sessionUser.session_token}
          onExchangeStarted={({ refund, creditMinor, originalReceipt }) => {
            setExchangeCredit({
              refundId: refund.refund_id,
              creditMinor,
              refundReceipt: refund.refund_receipt_number,
              originalReceipt,
            });
            setActiveModal({ kind: "none" });
            clearCart();
            focusBarcode();
          }}
          onClose={close}
        />
      )}

      {activeModal.kind === "report" && (
        <TodayReportModal
          sessionUserId={sessionUser.user_id}
          shiftId={shift.shift_id}
          sessionToken={sessionUser.session_token}
          includeCashDrawer={sessionUser.role_name === "owner" || sessionUser.role_name === "manager"}
          onClose={close}
        />
      )}

      {activeModal.kind === "help" && <HelpModal onClose={close} />}

      {activeModal.kind === "recent" && (
        <RecentSalesModal
          sessionUserId={sessionUser.user_id}
          onReprint={onReprintReceipt}
          onEdit={onEditSale}
          onClose={close}
        />
      )}
    </>
  );
}
