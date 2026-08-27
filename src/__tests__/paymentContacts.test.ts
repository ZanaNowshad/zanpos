import { describe, expect, it } from "vitest";
import type { CustomerRow, WaContact, WaMessage } from "../types";
import {
  chatAge, localPhone, mergeContacts, rankContacts, scoreContact, sourceLabel, toE164,
} from "../components/paymentContacts";

const customer = (id: string, name: string, phone: string | null, pts = 0): CustomerRow => ({
  customer_id: id, branch_id: "br_1", name, phone, email: null,
  loyalty_points: pts, created_at: "2026-01-01T00:00:00Z", notes: null,
});
const waContact = (jid: string, name: string): WaContact => ({ id: jid, name });
const waMessage = (jid: string, name: string | null, body: string, ts: number, group = false): WaMessage => ({
  id: `wam_${jid}_${ts}`, chat_jid: jid, chat_name: name, is_group: group,
  sender_jid: jid, sender_name: name, body, ts, read: false, media_type: null,
});

describe("phone normalisation", () => {
  it("reduces every spelling of a Bahrain number to the same eight digits", () => {
    // These are the four ways the same customer gets written down at a till.
    for (const written of ["+973 3600 1122", "97336001122", "0097336001122", "36001122"]) {
      expect(localPhone(written), written).toBe("36001122");
      expect(toE164(written), written).toBe("+97336001122");
    }
  });

  it("refuses to invent a number from something that is not one", () => {
    expect(toE164("Fatima")).toBeNull();
    expect(toE164("3600")).toBeNull();
    expect(toE164("")).toBeNull();
    expect(toE164(null)).toBeNull();
  });
});

describe("merging the three directories", () => {
  const customers = [customer("cus_1", "Fatima Al Sayed", "+973 3600 1122", 340)];
  const contacts = [
    waContact("97336001122@s.whatsapp.net", "Fatima"),
    waContact("97339887766@s.whatsapp.net", "Mariam Al Kooheji"),
  ];
  const messages = [
    waMessage("97339887766@s.whatsapp.net", "Mariam Al Kooheji", "Do you have laban?", 1_800_000_000),
    waMessage("97336001122@s.whatsapp.net", "Fatima", "Same address please", 1_800_000_200),
    waMessage("120363000000000000@g.us", "Amwaj Staff", "Stock at 6", 1_800_000_500, true),
  ];

  it("shows one row for a person the shop knows three ways", () => {
    const rows = mergeContacts(customers, contacts, messages);
    const fatima = rows.filter(row => row.localPhone === "36001122");
    expect(fatima).toHaveLength(1);
    expect(fatima[0].sources.sort()).toEqual(["chat", "customer", "whatsapp"].sort());
  });

  it("keeps the shop's spelling of a saved customer's name", () => {
    // WhatsApp carries whatever the customer called themselves. The receipt and
    // the ledger use the name the shop entered, so that one wins.
    const [fatima] = mergeContacts(customers, contacts, messages)
      .filter(row => row.localPhone === "36001122");
    expect(fatima.name).toBe("Fatima Al Sayed");
    expect(fatima.loyaltyPoints).toBe(340);
  });

  it("offers someone who has only ever chatted", () => {
    const mariam = mergeContacts(customers, contacts, messages)
      .find(row => row.localPhone === "39887766");
    expect(mariam?.customer).toBeNull();
    expect(mariam?.e164).toBe("+97339887766");
    expect(mariam?.lastChatPreview).toBe("Do you have laban?");
  });

  it("never offers a group as a receipt destination", () => {
    // A group JID is not a person and cannot receive a receipt. It reached the
    // list once and would have sent a customer's total to the staff group.
    const rows = mergeContacts(customers, contacts, messages);
    expect(rows.some(row => row.name === "Amwaj Staff")).toBe(false);
  });

  it("carries the most recent message, not the first one read", () => {
    const rows = mergeContacts([], [], [
      waMessage("97339887766@s.whatsapp.net", "Mariam", "older", 1_800_000_000),
      waMessage("97339887766@s.whatsapp.net", "Mariam", "newer", 1_800_009_000),
    ]);
    expect(rows[0].lastChatPreview).toBe("newer");
    expect(rows[0].lastChatAt).toBe(1_800_009_000);
  });
});

describe("ranking what was typed", () => {
  const rows = mergeContacts(
    [customer("cus_1", "Fatima Al Sayed", "+973 3600 1122"), customer("cus_2", "Layla Bu Ali", null)],
    [waContact("97336009999@s.whatsapp.net", "Fatima Hassan")],
    [],
  );

  it("puts an exact number above a partial one", () => {
    const [first] = rankContacts(rows, "36001122");
    expect(first.name).toBe("Fatima Al Sayed");
  });

  it("finds a name typed as a middle word", () => {
    // "Layla Bu Ali" is how the shop wrote it; the cashier hears "Ali".
    expect(rankContacts(rows, "ali").map(r => r.name)).toContain("Layla Bu Ali");
  });

  it("prefers a saved customer when the query fits both equally", () => {
    const ordered = rankContacts(rows, "Fatima").map(row => row.name);
    expect(ordered[0]).toBe("Fatima Al Sayed");
    expect(ordered).toContain("Fatima Hassan");
  });

  it("offers nothing rather than everything for a query that matches nothing", () => {
    expect(rankContacts(rows, "zzzz")).toEqual([]);
  });

  it("scores a mixed name-and-number query on whichever half matches", () => {
    const [fatima] = rows.filter(row => row.localPhone === "36001122");
    expect(scoreContact(fatima, "Fatima 3600")).toBeGreaterThan(0);
  });
});

describe("what the row says about itself", () => {
  it("names the directories in a fixed order", () => {
    expect(sourceLabel(["whatsapp", "customer"])).toBe("Saved · WhatsApp");
    expect(sourceLabel(["chat"])).toBe("Chatted");
  });

  it("describes chat recency in units a cashier reads at a glance", () => {
    const now = 1_800_000_000_000;
    expect(chatAge(1_800_000_000 - 30 * 60, now)).toBe("30m ago");
    expect(chatAge(1_800_000_000 - 5 * 3600, now)).toBe("5h ago");
    expect(chatAge(1_800_000_000 - 3 * 86400, now)).toBe("3d ago");
    expect(chatAge(null, now)).toBeNull();
  });
});

describe("finding someone by whichever name the till knows them under", () => {
  /* The reported problem: a shop saves a customer under its own name, the
     customer's WhatsApp profile says something else, and only the profile name
     could be typed. At a counter the cashier thinks of the name they saved. */
  const bothNames: WaContact = {
    id: "97333050666@s.whatsapp.net",
    name: "Ali Baqala",
    savedName: "Ali Baqala",
    pushName: "Ali ⚡",
  };

  it("keeps every name a person answers to", () => {
    const [row] = mergeContacts([], [bothNames], []);
    expect(row.name).toBe("Ali Baqala");
    expect(row.altNames).toContain("Ali ⚡");
  });

  it("finds them by the name the shop saved and by their own", () => {
    const rows = mergeContacts([], [bothNames], []);
    expect(rankContacts(rows, "Baqala").map(r => r.name)).toEqual(["Ali Baqala"]);
    expect(rankContacts(rows, "Ali").map(r => r.name)).toEqual(["Ali Baqala"]);
  });

  it("does not list the same name twice", () => {
    const same: WaContact = {
      id: "97333050777@s.whatsapp.net", name: "Sara", savedName: "Sara", pushName: "Sara",
    };
    const [row] = mergeContacts([], [same], []);
    expect(row.altNames).toEqual([]);
  });

  /* A saved customer's own spelling still wins for display — it is what goes on
     the receipt — but the WhatsApp name must stay searchable, which is exactly
     what the old code discarded. */
  it("shows the shop's name but still finds them by the WhatsApp one", () => {
    const rows = mergeContacts(
      [customer("c1", "Ali Cold Store", "33050666")],
      [bothNames],
      [],
    );
    expect(rows[0].name).toBe("Ali Cold Store");
    expect(rankContacts(rows, "Baqala").map(r => r.name)).toEqual(["Ali Cold Store"]);
    expect(rankContacts(rows, "Ali ⚡").map(r => r.name)).toEqual(["Ali Cold Store"]);
  });

  it("prefers the row matched on its display name when two people match", () => {
    const rows = mergeContacts(
      [customer("c1", "Baqala Mahmood", "33050111")],
      [bothNames],
      [],
    );
    expect(rankContacts(rows, "Baqala")[0].name).toBe("Baqala Mahmood");
  });
});

describe("typing a name the way people actually type it", () => {
  const rows = mergeContacts(
    [
      customer("c1", "Ali Baqala", "33050666"),
      customer("c2", "Al Osra Market", "33050777"),
      customer("c3", "فاطمة السيد", "33050888"),
    ],
    [],
    [],
  );
  const find = (query: string) => rankContacts(rows, query).map(row => row.name);

  it("takes the words in any order", () => {
    expect(find("baqala ali")).toContain("Ali Baqala");
  });

  it("ignores emoji, punctuation and case", () => {
    const emoji = mergeContacts([customer("c9", "Ali ⚡ Baqala!", "33050999")], [], []);
    expect(rankContacts(emoji, "ali baqala")).toHaveLength(1);
    expect(find("ALI")).toContain("Ali Baqala");
    expect(find("ali-baqala")).toContain("Ali Baqala");
  });

  it("matches inside a word, so a forgotten prefix still finds them", () => {
    expect(find("osra")).toContain("Al Osra Market");
  });

  it("folds the Arabic letters that vary by keyboard habit", () => {
    // ة vs ه and أ vs ا are typed interchangeably; neither should hide a customer.
    expect(find("فاطمه")).toContain("فاطمة السيد");
  });

  it("still refuses a query that matches nothing", () => {
    expect(find("zzzz")).toEqual([]);
  });
});

describe("the name a customer was imported under", () => {
  /* Contact import used to keep only the customer's own WhatsApp name, so a
     person this shop saved as "Ali Baqala" could be found at the till only by
     typing "Ali ⚡". Both names now reach the customer record, and both are
     matched — which matters most when the WhatsApp sidecar is disconnected and
     the stored row is all the till has. */
  const saved = (over: Partial<CustomerRow> = {}): CustomerRow => ({
    customer_id: "c1", branch_id: "br_1", name: "Ali Baqala",
    whatsapp_name: "Ali ⚡", phone: "33050666", email: null,
    loyalty_points: 0, created_at: "2026-01-01T00:00:00Z", notes: null, ...over,
  });

  it("finds a saved customer by their WhatsApp name with no live feed", () => {
    const rows = mergeContacts([saved()], [], []);
    expect(rankContacts(rows, "Ali ⚡").map(r => r.name)).toEqual(["Ali Baqala"]);
    expect(rankContacts(rows, "Baqala").map(r => r.name)).toEqual(["Ali Baqala"]);
  });

  it("still shows the shop's own spelling", () => {
    const [row] = mergeContacts([saved()], [], []);
    expect(row.name).toBe("Ali Baqala");
    expect(row.altNames).toContain("Ali ⚡");
  });

  it("does not break a customer who has no WhatsApp name", () => {
    const rows = mergeContacts([saved({ name: "Walk-in", whatsapp_name: null })], [], []);
    expect(rankContacts(rows, "Walk-in")).toHaveLength(1);
    expect(rows[0].altNames).toEqual([]);
  });
});
