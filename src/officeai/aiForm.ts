/**
 * The shape ZanAI uses to ask for values, and the text the answers travel back
 * in.
 *
 * The backend (`src-tauri/src/ai/forms.rs`) has already validated everything
 * here — caps, names, cell types. What this file owns is the round trip: a form
 * is drawn, the operator edits it, and the result has to reach the model as an
 * ordinary chat message that it can read without ambiguity. There is
 * deliberately no path from a submitted form into a command; the answers go
 * back through the same message flow as anything typed, so a form can never
 * become a second way to mutate the shop.
 */

export type AiFieldKind =
  | "text" | "textarea" | "number" | "money"
  | "integer" | "barcode" | "select" | "date" | "toggle";

export interface AiFormOption {
  value: string;
  label: string;
}

export interface AiFormField {
  name: string;
  label: string;
  type: AiFieldKind;
  value: string;
  placeholder: string;
  help: string;
  required: boolean;
  options: AiFormOption[];
}

export interface AiFormChoice {
  value: string;
  label: string;
  detail: string;
  style: "default" | "primary" | "danger";
}

export interface AiFormColumn {
  name: string;
  label: string;
  type: AiFieldKind;
  required: boolean;
  options: AiFormOption[];
}

export interface AiFormTable {
  columns: AiFormColumn[];
  rows: Array<Record<string, string>>;
  rowLabel: string;
  allowAdd: boolean;
  allowRemove: boolean;
}

export interface AiForm {
  title: string;
  note: string;
  submitLabel: string;
  fields: AiFormField[];
  choices: AiFormChoice[];
  table: AiFormTable | null;
}

const FIELD_KINDS: AiFieldKind[] = [
  "text", "textarea", "number", "money", "integer", "barcode", "select", "date", "toggle",
];

function str(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function bool(value: unknown): boolean {
  return value === true;
}

function kind(value: unknown): AiFieldKind {
  return FIELD_KINDS.includes(value as AiFieldKind) ? (value as AiFieldKind) : "text";
}

function options(value: unknown): AiFormOption[] {
  if (!Array.isArray(value)) return [];
  return value
    .filter((option): option is Record<string, unknown> => !!option && typeof option === "object")
    .map(option => ({ value: str(option.value), label: str(option.label) || str(option.value) }))
    .filter(option => option.value !== "");
}

/**
 * Rebuild the form from the channel payload.
 *
 * The backend validated it, so this is not a second gate — it is the guarantee
 * that a payload from a mismatched build (an installer updated while a session
 * is open) renders as a smaller form rather than throwing inside the message
 * list and taking the whole conversation down with it.
 */
export function normalizeAiForm(raw: unknown): AiForm | null {
  if (!raw || typeof raw !== "object") return null;
  const source = raw as Record<string, unknown>;
  const title = str(source.title).trim();
  if (!title) return null;

  const fields: AiFormField[] = Array.isArray(source.fields)
    ? source.fields
        .filter((field): field is Record<string, unknown> => !!field && typeof field === "object")
        .map(field => ({
          name: str(field.name),
          label: str(field.label) || str(field.name),
          type: kind(field.type),
          value: str(field.value),
          placeholder: str(field.placeholder),
          help: str(field.help),
          required: bool(field.required),
          options: options(field.options),
        }))
        .filter(field => field.name !== "")
    : [];

  const choices: AiFormChoice[] = Array.isArray(source.choices)
    ? source.choices
        .filter((choice): choice is Record<string, unknown> => !!choice && typeof choice === "object")
        .map((choice): AiFormChoice => ({
          value: str(choice.value),
          label: str(choice.label) || str(choice.value),
          detail: str(choice.detail),
          style:
            choice.style === "primary" || choice.style === "danger"
              ? choice.style
              : "default",
        }))
        .filter(choice => choice.value !== "")
    : [];

  const table = normalizeTable(source.table);
  if (fields.length === 0 && choices.length === 0 && !table) return null;

  return {
    title,
    note: str(source.note),
    submitLabel: str(source.submit_label),
    fields,
    choices,
    table,
  };
}

function normalizeTable(raw: unknown): AiFormTable | null {
  if (!raw || typeof raw !== "object") return null;
  const source = raw as Record<string, unknown>;
  const columns: AiFormColumn[] = Array.isArray(source.columns)
    ? source.columns
        .filter((column): column is Record<string, unknown> => !!column && typeof column === "object")
        .map(column => ({
          name: str(column.name),
          label: str(column.label) || str(column.name),
          type: kind(column.type),
          required: bool(column.required),
          options: options(column.options),
        }))
        .filter(column => column.name !== "")
    : [];
  if (columns.length === 0) return null;

  const rows: Array<Record<string, string>> = Array.isArray(source.rows)
    ? source.rows
        .filter((row): row is Record<string, unknown> => !!row && typeof row === "object")
        .map(row => {
          const cells: Record<string, string> = {};
          for (const column of columns) cells[column.name] = str(row[column.name]);
          return cells;
        })
    : [];

  return {
    columns,
    rows,
    rowLabel: str(source.row_label),
    allowAdd: bool(source.allow_add),
    allowRemove: bool(source.allow_remove),
  };
}

export function emptyRow(table: AiFormTable): Record<string, string> {
  const row: Record<string, string> = {};
  for (const column of table.columns) row[column.name] = "";
  return row;
}

/** Names of the fields and cells left empty that the form said were required. */
export function missingRequired(
  form: AiForm,
  values: Record<string, string>,
  rows: Array<Record<string, string>>,
): string[] {
  const missing: string[] = [];
  for (const field of form.fields) {
    if (field.required && !values[field.name]?.trim()) missing.push(field.name);
  }
  if (form.table) {
    for (const [index, row] of rows.entries()) {
      for (const column of form.table.columns) {
        if (column.required && !row[column.name]?.trim()) missing.push(`${index}:${column.name}`);
      }
    }
  }
  return missing;
}

/**
 * Values are written into a line-based message, so anything that could be read
 * as structure has to go: a newline would start a phantom field and a pipe
 * would invent a table column.
 */
function flatten(value: string): string {
  return value.replace(/\s+/g, " ").replace(/\|/g, "/").trim();
}

/**
 * The message the operator's answers are sent as.
 *
 * Machine-first on purpose. The model reads `name: value` without guessing, and
 * the operator still sees a legible record of what they sent in their own chat
 * bubble — which is the same string, because a form that reported one thing to
 * the model and showed another to the shop would be impossible to debug from a
 * transcript.
 */
export function formatFormAnswer(
  form: AiForm,
  values: Record<string, string>,
  rows: Array<Record<string, string>>,
  choice?: AiFormChoice,
): string {
  const lines: string[] = [form.title];

  if (choice) lines.push(`choice: ${flatten(choice.value)}`);

  for (const field of form.fields) {
    const value = flatten(values[field.name] ?? "");
    // An untouched optional field is absent, not blank — "reason:" with nothing
    // after it reads like a value the operator cleared on purpose.
    if (!value && !field.required) continue;
    lines.push(`${field.name}: ${value}`);
  }

  if (form.table && rows.length > 0) {
    lines.push(`columns: ${form.table.columns.map(column => column.name).join(", ")}`);
    rows.forEach((row, index) => {
      const cells = form.table!.columns.map(column => flatten(row[column.name] ?? "") || "—");
      lines.push(`${index + 1} | ${cells.join(" | ")}`);
    });
  }

  return lines.join("\n");
}

/**
 * What the assistant "said" when it put the form on screen.
 *
 * Two jobs, both load-bearing. The turn that shows a form produces no prose, and
 * an empty assistant entry would leave two user messages back to back in the
 * history — the same alternating-role break that already bit the confirmation
 * flow. It also carries the field names forward: the tool call holding the spec
 * is not part of the next request, so without this the model would receive
 * `new_price: 3.500` having forgotten it ever asked for `new_price`.
 */
export function describeFormForHistory(form: AiForm): string {
  const parts: string[] = [];
  if (form.fields.length > 0) {
    parts.push(
      `fields ${form.fields.map(field => `${field.name} (${field.label})`).join(", ")}`,
    );
  }
  if (form.choices.length > 0) {
    parts.push(`choices ${form.choices.map(choice => choice.value).join(", ")}`);
  }
  if (form.table) {
    const columns = form.table.columns.map(column => column.name).join(", ");
    parts.push(`an editable table of ${form.table.rows.length} row(s), columns ${columns}`);
  }
  return `[Asked the operator for input — "${form.title}": ${parts.join("; ")}.]`;
}
