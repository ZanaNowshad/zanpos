import ReceiptDesignEditor from "../ReceiptDesignEditor";

interface ReceiptTabProps {
  receiptHeader: string; setReceiptHeader: (v: string) => void;
  receiptFooter: string; setReceiptFooter: (v: string) => void;
  setSavedReceipt: (v: boolean) => void;
  name: string; address: string; phone: string;
  taxNumber: string; crNumber: string;
  savedReceipt: boolean; saving: boolean;
  handleSave: () => void;
}

export default function ReceiptTab(props: ReceiptTabProps) {
  const {
    receiptHeader, setReceiptHeader, receiptFooter, setReceiptFooter,
    setSavedReceipt, name, address, phone, taxNumber, crNumber,
    savedReceipt, saving, handleSave,
  } = props;

  return (
    <div className="settings-page">
      <section>
        <h3 className="settings-page-title">Receipt Text</h3>
        <p className="settings-hint">These lines are printed on every customer receipt.</p>

        <label className="bo-label">Receipt Header</label>
        <input className="bo-input" type="text" value={receiptHeader}
          onChange={e => { setReceiptHeader(e.target.value); setSavedReceipt(false); }}
          placeholder="Printed above the item list (e.g. tagline)"
          maxLength={120} />

        <label className="bo-label">Receipt Footer</label>
        <input className="bo-input" type="text" value={receiptFooter}
          onChange={e => { setReceiptFooter(e.target.value); setSavedReceipt(false); }}
          placeholder="Printed below the total (e.g. Thank you!)"
          maxLength={120} />

        {(receiptHeader || receiptFooter || name) && (
          <div className="settings-receipt-preview">
            <div className="settings-preview-label">Live Preview</div>
            <div className="receipt-mini">
              {name && <div className="receipt-mini-biz">{name}</div>}
              {address && <div className="receipt-mini-addr">{address}</div>}
              {phone && <div className="receipt-mini-addr">{phone}</div>}
              {taxNumber && <div className="receipt-mini-addr">TRN: {taxNumber}</div>}
              {crNumber && <div className="receipt-mini-addr">CR No: {crNumber}</div>}
              {receiptHeader && <div className="receipt-mini-header">{receiptHeader}</div>}
              <div className="receipt-mini-divider">- - - - - - - - - -</div>
              <div className="receipt-mini-item">Item name × 1 .............. 1.500</div>
              <div className="receipt-mini-divider">- - - - - - - - - -</div>
              <div className="receipt-mini-total">TOTAL: 1.500</div>
              {receiptFooter && <div className="receipt-mini-footer">{receiptFooter}</div>}
            </div>
          </div>
        )}
      </section>

      <hr className="settings-page-divider" />

      <section>
        <h3 className="settings-page-title">Receipt Design</h3>
        <p className="settings-hint">
          Customise layout, font size, paper width, and which information appears on receipts.
          Changes are saved locally on this device.
        </p>
        <ReceiptDesignEditor
          storeName={name}
          storeAddress={address}
          storePhone={phone}
          taxNumber={taxNumber}
          crNumber={crNumber}
          receiptHeader={receiptHeader}
          receiptFooter={receiptFooter}
        />
      </section>

      <div className="settings-page-actions">
        {savedReceipt && <span className="settings-action-msg">✓ Saved</span>}
        <button className="btn-primary" onClick={handleSave} disabled={saving}>
          {saving ? "Saving…" : "Save"}
        </button>
      </div>
    </div>
  );
}
