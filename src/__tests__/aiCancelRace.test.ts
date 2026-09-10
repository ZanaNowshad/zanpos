import { describe, expect, it } from "vitest";
import { isAlreadyStopped } from "../officeai/useChatController";

/**
 * Pressing Stop just as a stream ends produced a red banner reading "AI request
 * is not active" — a failure notice for an intent that had already succeeded.
 * The match has to track the strings the backend actually returns, so those are
 * kept byte-identical to ai_admin_commands.rs here.
 */
describe("recognising a cancel that lost the race", () => {
  it("matches the refusals the backend produces when the stream has ended", () => {
    // Byte-identical to request_chat_cancel in ai_admin_commands.rs.
    expect(isAlreadyStopped("AI request is not active")).toBe(true);
    expect(isAlreadyStopped("AI request is no longer active")).toBe(true);
  });

  it("survives the Error wrapper the invoke boundary adds", () => {
    expect(isAlreadyStopped("Error: AI request is not active")).toBe(true);
  });

  it("still surfaces an ownership refusal, which is not a race", () => {
    // One session must never silently cancel another session's request.
    expect(isAlreadyStopped("AI request does not belong to this session")).toBe(false);
  });

  it("does not swallow unrelated failures", () => {
    expect(isAlreadyStopped("AI request_id must contain 1..=128 characters")).toBe(false);
    expect(isAlreadyStopped("Session expired")).toBe(false);
    expect(isAlreadyStopped("")).toBe(false);
  });
});
