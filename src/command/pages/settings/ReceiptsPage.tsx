import { useEffect, useState } from "react";
import { settingsGetBranch, settingsUpdateBranch } from "../../../tauri/commands";
import ReceiptTab from "../../../components/settings/ReceiptTab";
import type { SessionToken } from "../../../types";

interface Props { sessionToken: SessionToken; }

export default function ReceiptsPage({ sessionToken }: Props) {
  const [name, setName] = useState(""); const [address, setAddress] = useState("");
  const [phone, setPhone] = useState(""); const [taxNumber, setTaxNumber] = useState("");
  const [crNumber, setCrNumber] = useState("");
  const [receiptHeader, setReceiptHeader] = useState("");
  const [receiptFooter, setReceiptFooter] = useState("");
  const [saving, setSaving] = useState(false); const [saved, setSaved] = useState(false);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    settingsGetBranch(sessionToken).then(b => {
      setName(b.name); setAddress(b.address ?? ""); setPhone(b.phone ?? "");
      setTaxNumber(b.tax_number ?? ""); setCrNumber(b.cr_number ?? "");
      setReceiptHeader(b.receipt_header ?? ""); setReceiptFooter(b.receipt_footer ?? "");
    }).finally(() => setLoading(false));
  }, [sessionToken]);

  const handleSave = async () => {
    setSaving(true);
    try {
      await settingsUpdateBranch({
        name: name.trim(), timezone: "Asia/Bahrain", address: address.trim() || undefined,
        phone: phone.trim() || undefined, tax_number: taxNumber.trim() || undefined,
        cr_number: crNumber.trim() || undefined,
        receipt_header: receiptHeader.trim() || undefined,
        receipt_footer: receiptFooter.trim() || undefined,
      }, sessionToken);
      setSaved(true); setTimeout(() => setSaved(false), 3000);
    } finally { setSaving(false); }
  };

  if (loading) return <div className="bo-empty">Loading receipt settings…</div>;
  return (
    <ReceiptTab
      receiptHeader={receiptHeader} setReceiptHeader={setReceiptHeader}
      receiptFooter={receiptFooter} setReceiptFooter={setReceiptFooter}
      setSavedReceipt={setSaved} name={name} address={address} phone={phone}
      taxNumber={taxNumber} crNumber={crNumber}
      savedReceipt={saved} saving={saving} handleSave={handleSave}
    />
  );
}
