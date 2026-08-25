import { useEffect, useId, useMemo, useRef, useState } from "react";
import type { Ref } from "react";
import { Plus, Trash2 } from "lucide-react";
import { useLanguage } from "../hooks/useLanguage";
import { officeAiFormat, officeAiTranslator } from "../i18n/officeAiStrings";
import {
  emptyRow, formatFormAnswer, missingRequired,
  type AiFieldKind, type AiForm, type AiFormChoice, type AiFormColumn, type AiFormField,
} from "./aiForm";
import "./aiForm.css";

/**
 * The form ZanAI draws in the chat.
 *
 * Everything rendered here was written by the model, so it is text and only
 * text — no markdown, no HTML, no dangerouslySetInnerHTML. A supplier's
 * delivery note goes through OCR into a table on this card, and a note that
 * managed to smuggle markup into a product name should show up as the odd
 * product name it is, not as markup.
 */

/** Keyboard the till should open. A money field that raises the alphabet is a
 *  field the cashier has to fight, and the panel is a touchscreen. */
function inputModeFor(type: AiFieldKind): "text" | "decimal" | "numeric" | "none" {
  switch (type) {
    case "money":
    case "number":
      return "decimal";
    case "integer":
    case "barcode":
      return "numeric";
    default:
      return "text";
  }
}

interface CellProps {
  id?: string;
  type: AiFieldKind;
  value: string;
  options: Array<{ value: string; label: string }>;
  placeholder?: string;
  invalid: boolean;
  ariaLabel?: string;
  onChange: (next: string) => void;
  inputRef?: Ref<HTMLInputElement>;
}

function Cell({ id, type, value, options, placeholder, invalid, ariaLabel, onChange, inputRef }: CellProps) {
  const shared = {
    id,
    "aria-label": ariaLabel,
    "aria-invalid": invalid || undefined,
    className: `aiform-input${invalid ? " aiform-input-invalid" : ""}`,
  };

  if (type === "select") {
    return (
      <select {...shared} value={value} onChange={event => onChange(event.target.value)}>
        <option value="" />
        {options.map(option => (
          <option key={option.value} value={option.value}>{option.label}</option>
        ))}
      </select>
    );
  }

  if (type === "toggle") {
    return (
      <label className="aiform-toggle">
        <input
          id={id}
          type="checkbox"
          aria-label={ariaLabel}
          checked={value === "true"}
          onChange={event => onChange(event.target.checked ? "true" : "false")}
        />
        <span />
      </label>
    );
  }

  if (type === "textarea") {
    return (
      <textarea
        {...shared}
        rows={2}
        value={value}
        placeholder={placeholder}
        onChange={event => onChange(event.target.value)}
      />
    );
  }

  return (
    <input
      {...shared}
      ref={inputRef}
      type={type === "date" ? "date" : "text"}
      inputMode={type === "date" ? undefined : inputModeFor(type)}
      value={value}
      placeholder={placeholder}
      onChange={event => onChange(event.target.value)}
    />
  );
}

function FieldRow({
  field, value, invalid, onChange, inputRef,
}: {
  field: AiFormField;
  value: string;
  invalid: boolean;
  onChange: (next: string) => void;
  inputRef?: Ref<HTMLInputElement>;
}) {
  const id = useId();
  return (
    <div className={`aiform-field aiform-field-${field.type}`}>
      <label className="aiform-label" htmlFor={id}>
        {field.label}
        {field.required && <span className="aiform-req" aria-hidden="true"> *</span>}
      </label>
      <Cell
        id={id}
        type={field.type}
        value={value}
        options={field.options}
        placeholder={field.placeholder}
        invalid={invalid}
        onChange={onChange}
        inputRef={inputRef}
      />
      {field.help && <span className="aiform-help">{field.help}</span>}
    </div>
  );
}

/**
 * One line per row, one labelled box per column.
 *
 * The first attempt was a spreadsheet: a header row of column names and bare
 * inputs beneath. It read well in the Assistant tab and was unusable everywhere
 * else — the docked copilot is about 260px wide, so a four-column bill came out
 * as 30px columns headed "BA RC OD E" over values reading "62" and "Na".
 *
 * So the cells wrap instead of shrinking, and each one carries its own label.
 * On a wide surface that lays out as the grid it always wanted to be; in the
 * dock it becomes one labelled box per line. No breakpoint, no knowledge of
 * which surface it is on, and it holds for two columns or eight.
 */
function TableGrid({
  rowLabel, columns, rows, missing, onCell, onRemove,
}: {
  rowLabel: string;
  columns: AiFormColumn[];
  rows: Array<Record<string, string>>;
  missing: Set<string>;
  onCell: (index: number, name: string, next: string) => void;
  onRemove?: (index: number) => void;
}) {
  const gridId = useId();
  const line = rowLabel || "line";
  return (
    <div className="aiform-grid">
      {rows.map((row, index) => (
        // Rows are positional: the operator reorders nothing, and a row carries
        // no id of its own because the model addressed them by position too.
        <div className="aiform-grid-row" key={index} role="group" aria-label={`${line} ${index + 1}`}>
          <div className="aiform-grid-bar">
            <span className="aiform-grid-n">{line} {index + 1}</span>
            {onRemove && (
              <button
                type="button"
                className="aiform-row-remove"
                onClick={() => onRemove(index)}
                aria-label={`Remove ${line} ${index + 1}`}
              >
                <Trash2 size={14} />
              </button>
            )}
          </div>
          <div className="aiform-grid-cells">
            {columns.map(column => {
              const id = `${gridId}-${index}-${column.name}`;
              return (
                <div className="aiform-grid-cell" key={column.name}>
                  <label className="aiform-label" htmlFor={id}>
                    {column.label}
                    {column.required && <span className="aiform-req" aria-hidden="true"> *</span>}
                  </label>
                  <Cell
                    id={id}
                    type={column.type}
                    value={row[column.name] ?? ""}
                    options={column.options}
                    invalid={missing.has(`${index}:${column.name}`)}
                    onChange={next => onCell(index, column.name, next)}
                  />
                </div>
              );
            })}
          </div>
        </div>
      ))}
    </div>
  );
}

interface Props {
  form: AiForm;
  disabled: boolean;
  onSubmit: (text: string) => void;
  onDismiss: () => void;
}

export default function AiFormCard({ form, disabled, onSubmit, onDismiss }: Props) {
  const { language } = useLanguage();
  const t = officeAiTranslator(language);
  const [values, setValues] = useState<Record<string, string>>(() =>
    Object.fromEntries(form.fields.map(field => [field.name, field.value])),
  );
  const [rows, setRows] = useState<Array<Record<string, string>>>(() => form.table?.rows ?? []);
  const [showErrors, setShowErrors] = useState(false);
  const firstInputRef = useRef<HTMLInputElement>(null);

  // The composer keeps focus when a message arrives, so without this the
  // operator sees boxes, starts typing, and the answer lands in the chat input
  // instead of the field they were looking at.
  useEffect(() => {
    firstInputRef.current?.focus();
  }, []);

  const missing = useMemo(
    () => new Set(missingRequired(form, values, rows)),
    [form, values, rows],
  );
  const invalid = showErrors ? missing : new Set<string>();

  const setCell = (index: number, name: string, next: string) =>
    setRows(prev => prev.map((row, i) => (i === index ? { ...row, [name]: next } : row)));

  const submit = (choice?: AiFormChoice) => {
    if (disabled) return;
    // A choice is an answer in itself — "cancel" must not be blocked by an
    // empty field the operator is choosing not to fill in.
    if (!choice && missing.size > 0) {
      setShowErrors(true);
      return;
    }
    onSubmit(formatFormAnswer(form, values, rows, choice));
  };

  const hasEntry = form.fields.length > 0 || !!form.table;

  return (
    <section className="aiform" aria-label={form.title}>
      <header className="aiform-head">
        <h4>{form.title}</h4>
        {form.note && <p className="aiform-note">{form.note}</p>}
      </header>

      {form.fields.length > 0 && (
        <div className="aiform-fields">
          {form.fields.map((field, index) => (
            <FieldRow
              key={field.name}
              field={field}
              value={values[field.name] ?? ""}
              invalid={invalid.has(field.name)}
              inputRef={index === 0 ? firstInputRef : undefined}
              onChange={next => setValues(prev => ({ ...prev, [field.name]: next }))}
            />
          ))}
        </div>
      )}

      {form.table && (
        <div className="aiform-table">
          <TableGrid
            rowLabel={form.table.rowLabel}
            columns={form.table.columns}
            rows={rows}
            missing={invalid}
            onCell={setCell}
            onRemove={
              form.table.allowRemove && rows.length > 1
                ? index => setRows(prev => prev.filter((_, i) => i !== index))
                : undefined
            }
          />
          {form.table.allowAdd && (
            <button
              type="button"
              className="aiform-row-add"
              onClick={() => setRows(prev => [...prev, emptyRow(form.table!)])}
              disabled={disabled}
            >
              <Plus size={14} />
              {officeAiFormat(t("formAddRow"), { row: form.table.rowLabel || t("formRow") })}
            </button>
          )}
        </div>
      )}

      {form.choices.length > 0 && (
        <div className="aiform-choices">
          {form.choices.map(choice => (
            <button
              key={choice.value}
              type="button"
              className={`aiform-choice aiform-choice-${choice.style}`}
              onClick={() => submit(choice)}
              disabled={disabled}
            >
              <span className="aiform-choice-label">{choice.label}</span>
              {choice.detail && <span className="aiform-choice-detail">{choice.detail}</span>}
            </button>
          ))}
        </div>
      )}

      <footer className="aiform-foot">
        {showErrors && missing.size > 0 && (
          <span className="aiform-error" role="alert">
            {officeAiFormat(t("formMissingRequired"), { count: missing.size })}
          </span>
        )}
        <button type="button" className="aiform-dismiss" onClick={onDismiss} disabled={disabled}>
          {t("formDismiss")}
        </button>
        {hasEntry && (
          <button
            type="button"
            className="aiform-submit"
            onClick={() => submit()}
            disabled={disabled}
          >
            {form.submitLabel || t("formSend")}
          </button>
        )}
      </footer>
    </section>
  );
}
