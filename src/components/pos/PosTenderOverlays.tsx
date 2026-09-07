import type { CustomerRow, DeliveryInput, PaymentInput, SaleResult } from "../../types";
import type { ReceiptConfidenceStatus } from "../../utils/posConfidence";
import PaymentModal, { type PaymentCompletionOptions } from "../PaymentModal";
import ReceiptActionCenter from "../ReceiptActionCenter";
import SaleDetailsModal from "../SaleDetailsModal";
import type { ActiveModal } from "./posModalState";
import type { SessionToken } from "../../types";

interface Props {
  activeModal: ActiveModal;
  payableTotal: number;
  loading: boolean;
  sessionToken: SessionToken;
  defaultPrintReceipt: boolean;
  bannerResult: SaleResult | null;
  receiptStatus: ReceiptConfidenceStatus;
  showSaleDetails: boolean;
  onConfirmPayment: (
    payments: PaymentInput[],
    customerId?: string,
    deliveryInput?: DeliveryInput,
    selectedCustomer?: CustomerRow,
    options?: PaymentCompletionOptions,
  ) => Promise<void>;
  onClosePayment: () => void;
  onPrint: (sale: SaleResult) => void;
  onNewSale: () => void;
  onViewDetails: () => void;
  onCloseDetails: () => void;
}

export default function PosTenderOverlays({
  activeModal,
  payableTotal,
  loading,
  sessionToken,
  defaultPrintReceipt,
  bannerResult,
  receiptStatus,
  showSaleDetails,
  onConfirmPayment,
  onClosePayment,
  onPrint,
  onNewSale,
  onViewDetails,
  onCloseDetails,
}: Props) {
  return (
    <>
      {activeModal.kind === "payment" && (
        <PaymentModal
          netTotal={payableTotal}
          initialMethod={activeModal.method}
          splitMode={activeModal.split}
          journey={activeModal.journey}
          onConfirm={onConfirmPayment}
          onCancel={onClosePayment}
          loading={loading}
          sessionToken={sessionToken}
          defaultPrintReceipt={defaultPrintReceipt}
        />
      )}

      {bannerResult && activeModal.kind === "none" && (
        <ReceiptActionCenter
          sale={bannerResult}
          receiptStatus={receiptStatus}
          whatsappStatus="not_available"
          onPrint={() => onPrint(bannerResult)}
          onNewSale={onNewSale}
          onViewDetails={onViewDetails}
          onDismiss={onNewSale}
        />
      )}

      {showSaleDetails && bannerResult && (
        <SaleDetailsModal sale={bannerResult} onClose={onCloseDetails} />
      )}
    </>
  );
}
