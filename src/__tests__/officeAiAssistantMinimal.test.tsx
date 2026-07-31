import { createRef } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import OfficeAIAssistantWorkspace from "../officeai/OfficeAIAssistantWorkspace";
import { ChatBubble } from "../officeai/ChatMessages";
import type { ChatController } from "../officeai/useChatController";
import type { SessionUser } from "../types";

const user: SessionUser = {
  user_id: "U1",
  branch_id: "B1",
  display_name: "Store Owner",
  username: "owner",
  role_id: "ROLE_OWNER",
  role_name: "owner",
  session_token: "session-secret",
  session_expires_at: "2099-01-01T00:00:00Z",
};

const ctrl = {
  input: "",
  setInput: () => {},
  messages: [],
  chatState: "idle",
  streamingMsgId: null,
  liveToolCalls: [],
  tokenCount: 0,
  streamStartTime: null,
  runState: null,
  imageAttachment: null,
  kpi: {
    loading: false,
    error: null,
    today: null,
    lowStockCount: 0,
    outOfStockCount: 0,
    sync: null,
    alerts: [],
  },
  handleSend: () => {},
  handleUndo: () => {},
  handleFeedback: () => {},
  handleRunExecute: () => {},
  handleRunCancel: () => {},
  handleRunUndo: () => {},
  handleClearChat: () => {},
  setImageAttachment: () => {},
  fetchKpi: () => Promise.resolve(),
  dismissAlert: () => {},
} as unknown as ChatController;

describe("minimal OfficeAI assistant", () => {
  it("keeps business context behind an on-demand inspector", () => {
    const html = renderToStaticMarkup(
      <OfficeAIAssistantWorkspace
        ctrl={ctrl}
        sessionUser={user}
        storeName="Main Store"
        configured
        configLoaded
        composerRef={createRef<HTMLTextAreaElement>()}
        onProviderConfigured={() => {}}
        onBackFromSetup={() => {}}
        onOpenHealth={() => {}}
        onOpenActions={() => {}}
        canUseManagerTools
      />,
    );

    expect(html).toContain("Business context");
    expect(html).not.toContain("Command context");
    expect(html).not.toContain("Live Snapshot");
  });

  it("offers an orphaned task for an explicit human-triggered resume", () => {
    const html = renderToStaticMarkup(
      <ChatBubble
        msg={{
          id: "ledger-resume",
          role: "system",
          text: "ZanAI was part-way through Importing supplier prices — resume?",
          timestamp: new Date("2026-07-30T10:00:00Z"),
          suggestedLabel: "Resume task",
          suggestedPrompt: "Continue the saved task. Read get_task_ledger first.",
        }}
        onSuggestedPrompt={() => {}}
      />,
    );

    expect(html).toContain("ZanAI was part-way through");
    expect(html).toContain("Resume task");
    expect(html).toContain("<button");
  });
});
