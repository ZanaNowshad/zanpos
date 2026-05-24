interface Props {
  onClose: () => void;
}

const SECTIONS: { heading: string; rows: { keys: string; action: string }[] }[] = [
  {
    heading: "Barcode & Navigation",
    rows: [
      { keys: "F2 / F3",       action: "Focus barcode scanner / search field" },
      { keys: "Escape",         action: "Return focus to barcode scanner" },
    ],
  },
  {
    heading: "Cart Management",
    rows: [
      { keys: "+ / =",          action: "Increment quantity of most-recent item" },
      { keys: "−",              action: "Decrement quantity of most-recent item" },
      { keys: "Delete",         action: "Remove most-recent item from cart" },
      { keys: "Backspace",      action: "Remove most-recent item (barcode field empty)" },
      { keys: "Ctrl+Delete",    action: "Clear entire cart (confirmation required)" },
      { keys: "Ctrl+Backspace", action: "Clear entire cart (confirmation required)" },
      { keys: "F8 / Ctrl+D",   action: "Apply bill-level discount (cart must have items)" },
    ],
  },
  {
    heading: "Payment & Sales",
    rows: [
      { keys: "F9",             action: "Open payment modal (cart must have items)" },
      { keys: "F12",            action: "Fast Cash — exact cash, no receipt printed" },
      { keys: "Ctrl+P",         action: "Reprint last receipt" },
      { keys: "F11 / Ctrl+N",  action: "No-sale — open cash drawer without a sale" },
    ],
  },
  {
    heading: "Held Carts",
    rows: [
      { keys: "F6",             action: "Hold current cart (save for later)" },
      { keys: "F7",             action: "Resume a held cart" },
    ],
  },
  {
    heading: "Refunds & Reports",
    rows: [
      { keys: "F10 / Ctrl+R",  action: "Open refund dialog" },
      { keys: "Ctrl+X",         action: "X-Report — close shift report (managers only)" },
    ],
  },
  {
    heading: "Session & Access",
    rows: [
      { keys: "Ctrl+L",         action: "Lock screen / logout" },
      { keys: "Ctrl+H",         action: "Show this keyboard shortcut help screen" },
    ],
  },
];

export default function HelpModal({ onClose }: Props) {
  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal help-modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <span className="modal-title">⌨ Keyboard Shortcuts</span>
          <button className="modal-close" onClick={onClose}>✕</button>
        </div>

        <div className="help-modal-body">
          {SECTIONS.map(section => (
            <div key={section.heading} className="help-section">
              <div className="help-section-heading">{section.heading}</div>
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
                      <td className="help-action-cell">{row.action}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ))}

          <p className="help-tip">
            💡 Most F-key shortcuts work even when a modal is open. Ctrl shortcuts require no modal to be active.
          </p>
        </div>

        <div className="modal-actions">
          <button className="btn-primary" onClick={onClose}>Close</button>
        </div>
      </div>
    </div>
  );
}
