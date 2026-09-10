import { describe, expect, it } from "vitest";
import { resolvedChatStateAfterStream } from "../officeai/useChatController";

/**
 * The stream can end while a confirmation is on screen — mutation_pending and
 * the final event land in the same tick. Forcing "idle" there tears down the
 * ConfirmActionModal before the user has answered, so a change ZanAI proposed
 * is silently dropped.
 *
 * This rule now runs on two paths: the `done` event, and the reconciliation
 * that fires when the awaited call returns without a terminal event ever
 * arriving. Both have to make the same choice.
 */
describe("settling chat state when a stream ends", () => {
  it("leaves a pending confirmation standing", () => {
    expect(resolvedChatStateAfterStream("confirm")).toBe("confirm");
    expect(resolvedChatStateAfterStream("run_confirm")).toBe("run_confirm");
  });

  it("does not interrupt a run that is mid-execution", () => {
    expect(resolvedChatStateAfterStream("run_executing")).toBe("run_executing");
  });

  it("returns everything else to idle, so the composer is usable again", () => {
    expect(resolvedChatStateAfterStream("thinking")).toBe("idle");
    expect(resolvedChatStateAfterStream("idle")).toBe("idle");
  });
});
