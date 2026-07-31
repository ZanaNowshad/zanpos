import { useLanguage } from "../hooks/useLanguage";
import { modalText } from "../i18n/modalStrings";
import { detailText, type DetailStringKey } from "../i18n/detailStrings";

interface Props {
  onClose: () => void;
}

const SECTIONS: { heading: DetailStringKey; rows: { keys: string; action: DetailStringKey }[] }[] = [
  {
    heading: "barcodeNavigation",
    rows: [
      { keys: "F2 / F3", action: "focusScanner" },
      { keys: "Escape", action: "returnFocusScanner" },
    ],
  },
  {
    heading: "cartManagement",
    rows: [
      { keys: "+ / =", action: "incrementRecent" },
      { keys: "−", action: "decrementRecent" },
      { keys: "Delete", action: "removeRecent" },
      { keys: "Backspace", action: "removeRecentEmpty" },
      { keys: "Ctrl+Delete", action: "clearCartConfirm" },
      { keys: "Ctrl+Backspace", action: "clearCartConfirm" },
      { keys: "F8", action: "applyBillDiscount" },
      { keys: "Ctrl+D", action: "discountLastItem" },
    ],
  },
  {
    heading: "paymentSales",
    rows: [
      { keys: "F9", action: "openPayment" },
      { keys: "F12", action: "fastCash" },
      { keys: "Ctrl+P", action: "reprintLastReceipt" },
      { keys: "F11 / Ctrl+N", action: "noSaleDrawer" },
    ],
  },
  {
    heading: "heldCarts",
    rows: [
      { keys: "F6", action: "holdCurrentCart" },
      { keys: "F7", action: "resumeHeldCart" },
    ],
  },
  {
    heading: "refundsReports",
    rows: [
      { keys: "F10 / Ctrl+R", action: "openRefund" },
      { keys: "Ctrl+X", action: "xReportManagers" },
    ],
  },
  {
    heading: "sessionAccess",
    rows: [
      { keys: "Ctrl+L", action: "lockOrLogout" },
      { keys: "Ctrl+H", action: "showShortcutHelp" },
    ],
  },
];

export default function HelpModal({ onClose }: Props) {
  const { language } = useLanguage();
  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal help-modal" onClick={e => e.stopPropagation()} role="dialog" aria-modal="true" aria-labelledby="help-title">
        <div className="modal-header">
          <span className="modal-title" id="help-title">⌨ {detailText(language, "keyboardShortcuts")}</span>
          <button className="modal-close" onClick={onClose}>✕</button>
        </div>

        <div className="help-modal-body">
          {SECTIONS.map(section => (
            <div key={section.heading} className="help-section">
              <div className="help-section-heading">{detailText(language, section.heading)}</div>
              <table className="help-shortcut-table">
                <tbody>
                  {section.rows.map(row => (
                    <tr key={row.keys}>
                      <td className="help-kbd-cell">
                        {row.keys.split(" / ").map((k, i) => (
                          <span key={k}>
                            {i > 0 && <span className="help-kbd-sep"> / </span>}
                            <kbd className="help-kbd">{k}</kbd>
                          </span>
                        ))}
                      </td>
                      <td className="help-action-cell">{detailText(language, row.action)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ))}

          <p className="help-tip">
            💡 {detailText(language, "shortcutTip")}
          </p>
        </div>

        <div className="modal-actions">
          <button className="btn-primary" onClick={onClose}>{modalText(language, "close")}</button>
        </div>
      </div>
    </div>
  );
}
