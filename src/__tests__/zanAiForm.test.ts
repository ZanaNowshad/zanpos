import { describe, expect, it } from "vitest";
import {
  describeFormForHistory, emptyRow, formatFormAnswer, missingRequired, normalizeAiForm,
} from "../officeai/aiForm";

/**
 * The form round trip.
 *
 * A form is only worth drawing if the answers come back in a shape the model
 * can act on without guessing. These tests pin that contract from both ends:
 * what a channel payload turns into, and what the operator's edits turn back
 * into.
 */

const PRICE_FORM = {
  title: "Update price",
  note: "",
  submit_label: "Update price",
  fields: [
    { name: "barcode", label: "Product barcode", type: "barcode", value: "", placeholder: "", help: "", required: true, options: [] },
    { name: "new_price", label: "New price (BHD)", type: "money", value: "", placeholder: "", help: "", required: true, options: [] },
  ],
  choices: [],
  table: null,
};

const BILL_FORM = {
  title: "Purchase bill — Al Noor Trading",
  note: "One line has no barcode.",
  submit_label: "Receive stock",
  fields: [],
  choices: [],
  table: {
    columns: [
      { name: "barcode", label: "Barcode", type: "barcode", required: true, options: [] },
      { name: "name", label: "Product", type: "text", required: true, options: [] },
      { name: "qty", label: "Qty", type: "integer", required: true, options: [] },
      { name: "unit_cost", label: "Unit cost", type: "money", required: true, options: [] },
    ],
    rows: [
      { barcode: "6281234567890", name: "Nadec Laban 1L", qty: "24", unit_cost: "0.420" },
      { barcode: "", name: "Local Dates 500g", qty: "6", unit_cost: "1.250" },
    ],
    row_label: "line",
    allow_add: true,
    allow_remove: true,
  },
};

describe("ZanAI form payloads", () => {
  it("reads a two-field price form off the channel", () => {
    const form = normalizeAiForm(PRICE_FORM)!;

    expect(form.title).toBe("Update price");
    expect(form.submitLabel).toBe("Update price");
    expect(form.fields.map(f => [f.name, f.type])).toEqual([
      ["barcode", "barcode"],
      ["new_price", "money"],
    ]);
  });

  /** A payload from a mismatched build must render smaller, never throw inside
   *  the message list and take the conversation down with it. */
  it("survives junk instead of throwing inside the message list", () => {
    expect(normalizeAiForm(null)).toBeNull();
    expect(normalizeAiForm("a form")).toBeNull();
    expect(normalizeAiForm({ fields: [] })).toBeNull();
    expect(normalizeAiForm({ title: "Empty" })).toBeNull();

    const partial = normalizeAiForm({
      title: "Half broken",
      fields: [
        { name: "ok", label: "Fine", type: "text" },
        { name: "", label: "No name", type: "text" },
        "not an object",
        { name: "weird", label: "Odd type", type: "colour-picker" },
      ],
    })!;
    expect(partial.fields.map(f => f.name)).toEqual(["ok", "weird"]);
    // An unknown control degrades to a plain box rather than disappearing.
    expect(partial.fields[1].type).toBe("text");
  });

  it("drops table cells that have no column, matching the backend contract", () => {
    const form = normalizeAiForm({
      title: "Bill",
      table: {
        columns: [{ name: "name", label: "Product", type: "text" }],
        rows: [{ name: "Milk", vat: "10%" }],
      },
    })!;
    expect(form.table!.rows[0]).toEqual({ name: "Milk" });
  });
});

describe("ZanAI form answers", () => {
  it("sends field answers keyed by the names the model asked for", () => {
    const form = normalizeAiForm(PRICE_FORM)!;
    const answer = formatFormAnswer(form, { barcode: "6979866554", new_price: "3.500" }, []);

    expect(answer).toBe("Update price\nbarcode: 6979866554\nnew_price: 3.500");
  });

  /** "reason:" with nothing after it reads like a value the operator cleared on
   *  purpose, which is a different instruction from not answering. */
  it("omits an untouched optional field rather than sending it blank", () => {
    const form = normalizeAiForm({
      title: "Adjust stock",
      fields: [
        { name: "qty", label: "Qty", type: "integer", required: true },
        { name: "reason", label: "Reason", type: "text" },
      ],
    })!;

    expect(formatFormAnswer(form, { qty: "12", reason: "" }, [])).toBe("Adjust stock\nqty: 12");
  });

  /** The answer is line-based and pipe-separated, so a value carrying either
   *  would invent a field or a column that the operator never filled in. */
  it("strips structure out of values so a note cannot forge a row", () => {
    const form = normalizeAiForm({
      title: "Note",
      fields: [{ name: "note", label: "Note", type: "textarea", required: true }],
    })!;

    const answer = formatFormAnswer(form, { note: "line one\nnew_price: 0.001" }, []);
    expect(answer).toBe("Note\nnote: line one new_price: 0.001");
    expect(answer.split("\n")).toHaveLength(2);
  });

  it("sends a bill table as numbered rows under a column key", () => {
    const form = normalizeAiForm(BILL_FORM)!;
    const rows = form.table!.rows.map((row, index) =>
      index === 1 ? { ...row, barcode: "6291001234567" } : row,
    );

    expect(formatFormAnswer(form, {}, rows)).toBe(
      [
        "Purchase bill — Al Noor Trading",
        "columns: barcode, name, qty, unit_cost",
        "1 | 6281234567890 | Nadec Laban 1L | 24 | 0.420",
        "2 | 6291001234567 | Local Dates 500g | 6 | 1.250",
      ].join("\n"),
    );
  });

  it("marks an empty cell rather than shifting the columns along", () => {
    const form = normalizeAiForm(BILL_FORM)!;
    const answer = formatFormAnswer(form, {}, [{ barcode: "", name: "Mystery", qty: "1", unit_cost: "" }]);

    expect(answer).toContain("1 | — | Mystery | 1 | —");
  });

  it("sends a tapped choice as its value, not its label", () => {
    const form = normalizeAiForm({
      title: "Apply where?",
      choices: [{ value: "all_branches", label: "Every branch", detail: "3 shops" }],
    })!;

    expect(formatFormAnswer(form, {}, [], form.choices[0])).toBe("Apply where?\nchoice: all_branches");
  });
});

describe("ZanAI form validation", () => {
  it("names every required box left empty, fields and cells alike", () => {
    const price = normalizeAiForm(PRICE_FORM)!;
    expect(missingRequired(price, { barcode: "123", new_price: "  " }, [])).toEqual(["new_price"]);

    const bill = normalizeAiForm(BILL_FORM)!;
    expect(missingRequired(bill, {}, bill.table!.rows)).toEqual(["1:barcode"]);
  });

  it("counts a freshly added row's required cells as missing", () => {
    const bill = normalizeAiForm(BILL_FORM)!;
    const rows = [{ ...bill.table!.rows[0] }, emptyRow(bill.table!)];

    expect(missingRequired(bill, {}, rows)).toEqual([
      "1:barcode", "1:name", "1:qty", "1:unit_cost",
    ]);
  });
});

describe("ZanAI form history entry", () => {
  /**
   * Two failures this prevents, both invisible until the next message: an empty
   * assistant turn puts two user messages back to back, and the tool call that
   * carried the field names is not replayed — so without this the model gets
   * `new_price: 3.500` having forgotten it ever asked for `new_price`.
   */
  it("records the names the model asked for", () => {
    const entry = describeFormForHistory(normalizeAiForm(PRICE_FORM)!);

    expect(entry).not.toBe("");
    expect(entry).toContain("Update price");
    expect(entry).toContain("barcode (Product barcode)");
    expect(entry).toContain("new_price (New price (BHD))");
  });

  it("records a table by its columns and row count", () => {
    const entry = describeFormForHistory(normalizeAiForm(BILL_FORM)!);

    expect(entry).toContain("2 row(s)");
    expect(entry).toContain("barcode, name, qty, unit_cost");
  });

  it("records the values a choice can come back as", () => {
    const entry = describeFormForHistory(normalizeAiForm({
      title: "Apply where?",
      choices: [{ value: "this_branch", label: "Here" }, { value: "all_branches", label: "Everywhere" }],
    })!);

    expect(entry).toContain("choices this_branch, all_branches");
  });
});
