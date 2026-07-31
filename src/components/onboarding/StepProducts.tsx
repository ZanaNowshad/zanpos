import { useState } from "react";
import BulkImportModal from "../BulkImportModal";
import ProductsZanAiPanel from "./ProductsZanAiPanel";

interface Props {
  ownerUserId: string;
  ownerUsername: string;
  onDone: () => void;
}

type Choice = "menu" | "paste" | "csv";

/** Step 4 — load products, three ways: paste a list (ZanAI bulk create), CSV
 * import (BulkImportModal — already reports skipped rows in detail, never
 * silently swallowed), or start empty. All three are equally valid and this
 * step is fully skippable via "start empty". */
export default function StepProducts({ ownerUserId, ownerUsername, onDone }: Props) {
  const [choice, setChoice] = useState<Choice>("menu");
  const [addedCount, setAddedCount] = useState(0);

  return (
    <div className="setup-content">
      <h2 className="setup-title">Load Products</h2>
      <p className="setup-body">Pick whichever is fastest for you — you can always add more later in Back Office.</p>

      {addedCount > 0 && (
        <div className="setup-review-row"><span>Products added so far</span><strong>{addedCount}</strong></div>
      )}

      <div className="setup-path-cards">
        <button className="setup-path-card" onClick={() => setChoice("paste")}>
          <span className="setup-path-icon">💬</span>
          <span className="setup-path-title">Paste a list</span>
          <span className="setup-path-desc">Paste product names and prices as text — ZanAI creates them for you.</span>
        </button>

        <button className="setup-path-card" onClick={() => setChoice("csv")}>
          <span className="setup-path-icon">📊</span>
          <span className="setup-path-title">Import CSV</span>
          <span className="setup-path-desc">Upload a spreadsheet of products. Skipped rows are always shown.</span>
        </button>

        <button className="setup-path-card" onClick={onDone}>
          <span className="setup-path-icon">🏁</span>
          <span className="setup-path-title">Start empty</span>
          <span className="setup-path-desc">Skip this step — add products manually later from Back Office.</span>
        </button>
      </div>

      <div className="setup-actions">
        <button className="setup-btn-primary" onClick={onDone}>Continue <span className="icon-directional" aria-hidden="true">→</span></button>
      </div>

      {choice === "paste" && (
        <ProductsZanAiPanel
          ownerUsername={ownerUsername}
          onClose={() => setChoice("menu")}
        />
      )}

      {choice === "csv" && (
        <BulkImportModal
          mode="products"
          sessionUserId={ownerUserId}
          onClose={() => setChoice("menu")}
          onDone={() => setAddedCount(n => n + 1)}
        />
      )}
    </div>
  );
}
