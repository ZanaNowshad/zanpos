interface Props {
  checked: boolean;
  onChange: (checked: boolean) => void;
}

export function AiConfirmationPolicyToggle({ checked, onChange }: Props) {
  return (
    <label htmlFor="ai-confirm-safe-actions" className="ai-toggle-row">
      <input
        id="ai-confirm-safe-actions"
        type="checkbox"
        checked={checked}
        onChange={event => onChange(event.target.checked)}
      />
      <span>
        Confirm non-destructive ZanAI actions
        <small>
          When off, creates and updates run immediately. Delete, remove, archive,
          deactivate, void, cancel, and merge actions always require confirmation.
        </small>
      </span>
    </label>
  );
}
