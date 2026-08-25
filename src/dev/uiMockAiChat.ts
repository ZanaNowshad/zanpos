/**
 * Canned ZanAI streams for the browser mock.
 *
 * The chat is the one surface that cannot be reached in QA by stubbing a
 * return value: `ai_chat_stream` answers over a Tauri Channel, so an unstubbed
 * command leaves the widget stuck on "Thinking…" forever. That is how the
 * interactive form nearly shipped unlooked-at — the states it introduces are
 * exactly the ones a canned reply has to reproduce.
 *
 * These streams follow the real event order: started, then tokens or tool
 * activity, then whatever ends the turn. A form request ends it, the same way
 * the backend does after `request_input`.
 */

interface MockChannel {
  onmessage?: (event: unknown) => void;
}

const PRICE_FORM = {
  title: "Update price",
  note: "Scan the barcode or type it in.",
  submit_label: "Update price",
  fields: [
    { name: "barcode", label: "Product barcode", type: "barcode", value: "", placeholder: "e.g. 6291001234567", help: "", required: true, options: [] },
    { name: "new_price", label: "New price (BHD)", type: "money", value: "", placeholder: "0.000", help: "Three decimals", required: true, options: [] },
  ],
  choices: [],
  table: null,
};

const BILL_FORM = {
  title: "Purchase bill — Al Noor Trading",
  note: "Two lines have no barcode. Fill those in and check the costs before I receive the stock.",
  submit_label: "Receive stock",
  fields: [
    { name: "supplier", label: "Supplier", type: "text", value: "Al Noor Trading", placeholder: "", help: "", required: true, options: [] },
    { name: "invoice_no", label: "Invoice number", type: "text", value: "AN-20418", placeholder: "", help: "", required: true, options: [] },
    { name: "received_on", label: "Received on", type: "date", value: "2026-08-23", placeholder: "", help: "", required: false, options: [] },
  ],
  choices: [],
  table: {
    columns: [
      { name: "barcode", label: "Barcode", type: "barcode", required: true, options: [] },
      { name: "name", label: "Product", type: "text", required: true, options: [] },
      { name: "qty", label: "Qty", type: "integer", required: true, options: [] },
      { name: "unit_cost", label: "Unit cost", type: "money", required: true, options: [] },
    ],
    rows: [
      { barcode: "6291001234567", name: "Nadec Laban 1L", qty: "24", unit_cost: "0.420" },
      { barcode: "6281002345678", name: "Almarai Milk 2L", qty: "12", unit_cost: "1.150" },
      { barcode: "", name: "Local Dates 500g", qty: "6", unit_cost: "1.250" },
      { barcode: "", name: "Bahrain Bread (large)", qty: "40", unit_cost: "0.085" },
    ],
    row_label: "line",
    allow_add: true,
    allow_remove: true,
  },
};

const BRANCH_CHOICE = {
  title: "Apply to which branches?",
  note: "",
  submit_label: "",
  fields: [],
  choices: [
    { value: "this_branch", label: "This branch only", detail: "Amwaj AlDair", style: "primary" },
    { value: "all_branches", label: "Every branch", detail: "3 shops", style: "default" },
    { value: "cancel", label: "Cancel", detail: "", style: "danger" },
  ],
  table: null,
};

function pickForm(message: string): unknown | null {
  const text = message.toLowerCase();
  if (text.includes("bill") || text.includes("delivery note") || text.includes("purchase")) return BILL_FORM;
  if (text.includes("branch")) return BRANCH_CHOICE;
  if (text.includes("price") || text.includes("update") || text.includes("form")) return PRICE_FORM;
  return null;
}

/** Answers submitted from a form come back as the title followed by the values,
 *  so this is what the canned assistant echoes to prove the round trip. */
function isFormAnswer(message: string): boolean {
  return /\n(choice|columns|[a-z0-9_]+):/.test(message);
}

export function mockAiChatStream(
  input: { message?: string } | undefined,
  channel: MockChannel | undefined,
): Promise<void> {
  const send = (event: unknown, delay: number) =>
    new Promise<void>(resolve => setTimeout(() => { channel?.onmessage?.(event); resolve(); }, delay));

  const message = String(input?.message ?? "");

  return (async () => {
    await send({ type: "started" }, 120);

    if (isFormAnswer(message)) {
      // The point of the round trip: values arrive, the work happens, one line
      // comes back. No re-asking, no confirming what was just typed.
      for (const chunk of ["Done — ", "Nadec Laban 1L ", "0.550 → 3.500 BHD"]) {
        await send({ type: "token", text: chunk }, 90);
      }
      await send({ type: "done" }, 120);
      return;
    }

    const form = pickForm(message);
    if (form) {
      await send({ type: "tool_start", name: "request_input" }, 260);
      await send({ type: "tool_done", name: "request_input" }, 220);
      await send({ type: "form_request", form }, 60);
      await send({ type: "done" }, 40);
      return;
    }

    for (const chunk of ["Nothing canned for that. ", "Ask for a price update, a purchase bill, or a branch choice."]) {
      await send({ type: "token", text: chunk }, 120);
    }
    await send({ type: "done" }, 100);
  })();
}

// ── Threads ──────────────────────────────────────────────────────────────────
//
// Held here rather than returned as a canned constant, because New chat,
// reopen, rename and delete are *transitions* — a fixed list would render the
// sidebar and leave every one of them unverifiable.

interface MockConversation {
  conversation_id: string;
  branch_id: string;
  user_id: string;
  title: string;
  message_count: number;
  last_message_at: string | null;
  created_at: string;
  updated_at: string;
  messages: Array<{ role: string; content: string }>;
}

const ago = (minutes: number) => new Date(Date.now() - minutes * 60_000).toISOString();

let conversations: MockConversation[] = [
  {
    conversation_id: "conv-mock-vat",
    branch_id: "br_amwaj", user_id: "usr_renihal",
    title: "VAT filing for August",
    message_count: 4, last_message_at: ago(90), created_at: ago(120), updated_at: ago(90),
    messages: [
      { role: "user", content: "Prepare the VAT return figures for August" },
      { role: "assistant", content: "Output VAT 412.500 · Input VAT 236.750 · Payable 175.750" },
    ],
  },
  {
    conversation_id: "conv-mock-stock",
    branch_id: "br_amwaj", user_id: "usr_renihal",
    title: "Which products are low on stock?",
    message_count: 2, last_message_at: ago(1500), created_at: ago(1510), updated_at: ago(1500),
    messages: [
      { role: "user", content: "Which products are low on stock?" },
      { role: "assistant", content: "93 out of stock, 2 more below reorder level." },
    ],
  },
];

let activeConversation = conversations[0].conversation_id;

export function mockActiveConversation(): string {
  return activeConversation;
}

export function mockListConversations(): MockConversation[] {
  return conversations.map(({ messages: _messages, ...row }) => row as MockConversation);
}

export function mockConversationView(conversationId: string) {
  const found = conversations.find(c => c.conversation_id === conversationId);
  activeConversation = conversationId;
  return {
    conversation_id: conversationId,
    title: found?.title ?? "",
    messages: (found?.messages ?? []).map((m, index) => ({
      id: index,
      message_id: `${conversationId}-${index}`,
      session_id: `s-${index}`,
      branch_id: "br_amwaj",
      user_id: "usr_renihal",
      role: m.role,
      content: m.content,
      message_type: "text",
      created_at: ago(100 - index),
    })),
  };
}

export function mockDeleteConversation(conversationId: string): void {
  conversations = conversations.filter(c => c.conversation_id !== conversationId);
}

export function mockRenameConversation(conversationId: string, title: string): void {
  const found = conversations.find(c => c.conversation_id === conversationId);
  if (found) found.title = title;
}

/** A thread is named by its first message and appears in the list only once it
 *  has one — the same rule the real repo applies. */
export function mockRecordExchange(conversationId: string | null, message: string): void {
  if (!conversationId) return;
  activeConversation = conversationId;
  const existing = conversations.find(c => c.conversation_id === conversationId);
  if (existing) {
    existing.message_count += 2;
    existing.last_message_at = new Date().toISOString();
    return;
  }
  conversations.unshift({
    conversation_id: conversationId,
    branch_id: "br_amwaj", user_id: "usr_renihal",
    title: message.split(/\s+/).slice(0, 10).join(" ").slice(0, 80),
    message_count: 2,
    last_message_at: new Date().toISOString(),
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    messages: [],
  });
}
