import { describe, expect, it } from "vitest";
import type { DisplayMessage } from "../officeai/officeAiTypes";
import { appendBoundedMessages, boundLoadedMessages } from "../zanai/messageRetention";

function message(index: number): DisplayMessage {
  return {
    id: `message-${index}`,
    role: index % 2 === 0 ? "user" : "assistant",
    text: `message ${index}`,
    timestamp: new Date(index * 1000),
  };
}

describe("ZanAI message retention", () => {
  it("keeps the newest messages in stable order", () => {
    const current = Array.from({ length: 100 }, (_, index) => message(index));
    const result = appendBoundedMessages(current, [message(100), message(101)], 100);

    expect(result).toHaveLength(100);
    expect(result[0].id).toBe("message-2");
    expect(result.at(-1)?.id).toBe("message-101");
  });

  it("preserves an actionable message while compacting older inactive messages", () => {
    const current = Array.from({ length: 6 }, (_, index) => message(index));
    current[0] = {
      ...current[0],
      pendingBatchActions: [{ action_id: "pending" }] as DisplayMessage["pendingBatchActions"],
    };

    const result = appendBoundedMessages(current, [message(6)], 4);

    expect(result.map(item => item.id)).toEqual([
      "message-0",
      "message-4",
      "message-5",
      "message-6",
    ]);
  });

  it("bounds loaded history without mutating the input", () => {
    const loaded = Array.from({ length: 5 }, (_, index) => message(index));
    const result = boundLoadedMessages(loaded, 3);

    expect(result.map(item => item.id)).toEqual(["message-2", "message-3", "message-4"]);
    expect(loaded).toHaveLength(5);
  });
});
