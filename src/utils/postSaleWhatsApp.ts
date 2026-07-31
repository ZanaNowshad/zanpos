import type { CustomerRow, DeliveryInput, SaleResult, SessionUser } from "../types";
import { DEVICE } from "../types";
import {
  appConfigLoad,
  whatsappSendDelivery,
  whatsappSendReceiptPdf,
  whatsappStatus,
} from "../tauri/commands";
import { formatMoney } from "../money";
import {
  buildCustomerMessage,
  buildDeliveryMessage,
  loadWaCustomerFormat,
  loadWaFormat,
} from "./waMessageFormat";

interface Options {
  result: SaleResult;
  deliveryInput?: DeliveryInput;
  selectedCustomer?: CustomerRow;
  sessionUser: SessionUser;
  onPairingRequired: () => void;
}

export function dispatchPostSaleWhatsApp({
  result,
  deliveryInput,
  selectedCustomer,
  sessionUser,
  onPairingRequired,
}: Options): void {
  if (result.delivery && result.delivery.contact_number) {
    const delivery = result.delivery;
    const sendDelivery = async () => {
      try {
        const status = await whatsappStatus(sessionUser.user_id);
        if (status.connected) {
          let messageOverride: string | undefined;
          try {
            const format = loadWaFormat();
            const config = await appConfigLoad();
            const activeLines = format.language === "ar" ? format.ar_lines : format.en_lines;
            const now = new Date();
            const date = now.toLocaleDateString(format.language === "ar" ? "ar-BH" : "en-GB", {
              day: "numeric", month: "long", year: "numeric",
            }) + ", " + now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
            const method = result.payments[0]?.method ?? "cash";
            const methodLabel = method === "wallet"
              ? "BenefitPay"
              : method.charAt(0).toUpperCase() + method.slice(1);
            messageOverride = buildDeliveryMessage(activeLines, {
              customer_name: delivery.customer_name ?? "",
              receipt_number: result.receipt_number,
              date,
              amount: `${DEVICE.currency} ${formatMoney(result.net_total_minor, DEVICE.currency_exponent)}`,
              address: delivery.address_text,
              house_number: delivery.house_number ?? "",
              area: delivery.area ?? "",
              delivery_note: delivery.delivery_note ?? "",
              method: methodLabel,
              benefit_number: config.whatsapp_benefit_number ?? "",
              store_name: DEVICE.branch_name,
              store_phone: "",
            }, result.items, DEVICE.currency_exponent);
          } catch {
            // Template failure falls through to the backend's receipt builder.
          }

          const digits = (delivery.contact_number || "").replace(/\D/g, "");
          const to = digits.startsWith("973") ? digits : `973${digits}`;
          let pdfSent = false;
          try {
            pdfSent = await whatsappSendReceiptPdf(sessionUser.user_id, {
              to,
              receipt_number: result.receipt_number,
              branch_name: result.branch_name,
              cashier_name: result.cashier_name,
              sold_at: result.sold_at,
              currency: DEVICE.currency,
              currency_exponent: DEVICE.currency_exponent,
              items: result.items.map(item => ({
                product_name: item.product_name,
                quantity: item.quantity,
                unit_price_minor: item.unit_price_minor,
                line_total_minor: item.line_total_minor,
              })),
              net_total_minor: result.net_total_minor,
              tax_total_minor: result.tax_total_minor,
              discount_total_minor: result.discount_total_minor,
              payments: result.payments.map(payment => ({
                method: payment.method,
                amount_minor: payment.amount_minor,
                change_minor: payment.change_minor,
              })),
              caption: messageOverride,
              address_text: delivery.address_text,
              house_number: delivery.house_number ?? undefined,
              area: delivery.area ?? undefined,
            });
          } catch {
            // PDF failure falls through to text-only delivery confirmation.
          }
          if (!pdfSent) {
            await whatsappSendDelivery(sessionUser.user_id, {
              to,
              receipt_number: result.receipt_number,
              net_total_minor: result.net_total_minor,
              currency_exponent: DEVICE.currency_exponent,
              address_text: delivery.address_text,
              house_number: delivery.house_number ?? undefined,
              area: delivery.area ?? undefined,
              message_override: messageOverride,
            });
          }
        } else if (sessionUser.role_name === "owner" || sessionUser.role_name === "manager") {
          onPairingRequired();
        }
      } catch {
        // WhatsApp is downstream of the committed sale and must never propagate.
      }
    };
    void sendDelivery();
  }

  if (selectedCustomer?.phone && !deliveryInput) {
    const sendCustomerReceipt = async () => {
      try {
        const status = await whatsappStatus(sessionUser.user_id);
        if (!status.connected) return;
        const config = await appConfigLoad();
        const digits = selectedCustomer.phone!.replace(/\D/g, "");
        const to = digits.startsWith("973") ? digits : `973${digits}`;
        const format = loadWaCustomerFormat();
        const activeLines = format.language === "ar" ? format.ar_lines : format.en_lines;
        const now = new Date();
        const date = now.toLocaleDateString(format.language === "ar" ? "ar-BH" : "en-GB", {
          day: "numeric", month: "long", year: "numeric",
        }) + ", " + now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
        const method = result.payments[0]?.method ?? "cash";
        const methodLabel = method === "wallet"
          ? "BenefitPay"
          : method.charAt(0).toUpperCase() + method.slice(1);
        const amount = `${DEVICE.currency} ${formatMoney(result.net_total_minor, DEVICE.currency_exponent)}`;
        const built = buildCustomerMessage(activeLines, {
          customer_name: selectedCustomer.name ?? "",
          receipt_number: result.receipt_number,
          date,
          amount,
          address: "",
          house_number: "",
          area: "",
          delivery_note: "",
          method: methodLabel,
          benefit_number: method === "wallet" ? (config.whatsapp_benefit_number ?? "") : "",
          store_name: DEVICE.branch_name,
          store_phone: "",
        }, result.items, DEVICE.currency_exponent);
        const message = built.trim().length > 0
          ? built
          : `✅ Thank you, ${selectedCustomer.name}!\n`
            + `Receipt #${result.receipt_number}\n`
            + `Date: ${date}\n`
            + `Amount: ${amount}\n`
            + `Paid by: ${methodLabel}`;
        let pdfSent = false;
        try {
          pdfSent = await whatsappSendReceiptPdf(sessionUser.user_id, {
            to,
            receipt_number: result.receipt_number,
            branch_name: result.branch_name,
            cashier_name: result.cashier_name,
            sold_at: result.sold_at,
            currency: DEVICE.currency,
            currency_exponent: DEVICE.currency_exponent,
            items: result.items.map(item => ({
              product_name: item.product_name,
              quantity: item.quantity,
              unit_price_minor: item.unit_price_minor,
              line_total_minor: item.line_total_minor,
            })),
            net_total_minor: result.net_total_minor,
            tax_total_minor: result.tax_total_minor,
            discount_total_minor: result.discount_total_minor,
            payments: result.payments.map(payment => ({
              method: payment.method,
              amount_minor: payment.amount_minor,
              change_minor: payment.change_minor,
            })),
            caption: message,
          });
        } catch {
          // PDF failure falls through to text-only receipt confirmation.
        }
        if (!pdfSent) {
          await whatsappSendDelivery(sessionUser.user_id, {
            to,
            receipt_number: result.receipt_number,
            net_total_minor: result.net_total_minor,
            currency_exponent: DEVICE.currency_exponent,
            address_text: "",
            message_override: message,
          });
        }
      } catch {
        // Customer receipt confirmation must never block the receipt flow.
      }
    };
    void sendCustomerReceipt();
  }
}
