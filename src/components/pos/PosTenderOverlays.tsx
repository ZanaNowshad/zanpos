import type { CustomerRow, DeliveryInput, PaymentInput, SaleResult } from "../../types";
import type { ReceiptConfidenceStatus } from "../../utils/posConfidence";
import PaymentModal from "../PaymentModal";
import ReceiptActionCenter from "../ReceiptActionCenter";
import SaleDetailsModal from "../SaleDetailsModal";
import type { ActiveModal } from "./posModalState";

interface Props {
  activeModal: ActiveModal;
  payableTotal: number;
  loading: boolean;
  sessionUserId: string;
  bannerResult: SaleResult | null;
  receiptStatus: ReceiptConfidenceStatus;
  showSaleDetails: boolean;
  onConfirmPayment: (
    payments: PaymentInput[],
    customerId?: string,
    deliveryInput?: DeliveryInput,
    selectedCustomer?: CustomerRow,
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
  sessionUserId,
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
          onConfirm={onConfirmPayment}
          onCancel={onClosePayment}
          loading={loading}
          sessionUserId={sessionUserId}
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
