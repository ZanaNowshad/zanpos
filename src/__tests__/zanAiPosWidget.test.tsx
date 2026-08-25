import { createElement, createRef, type ComponentProps } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { ChatController } from "../officeai/useChatController";
import { PosZanAiWidgetView } from "../zanai/PosZanAiWidget";

const ctrl = {
  messages: [], input: "", setInput: () => {}, imageAttachment: null, setImageAttachment: () => {},
  chatState: "idle", pendingAction: null, pendingBatchActions: null, liveToolCalls: [], streamingMsgId: null,
  tokenCount: 0, streamStartTime: null, canStop: false,
  kpi: { loading: false, error: null, today: null, lowStockCount: 0, outOfStockCount: 0, sync: null, alerts: [] },
  runState: null, bulkProgress: null, errorMessage: null, dismissError: () => {},
  fetchKpi: async () => {}, dismissAlert: async () => {}, handleSend: async () => {}, handleStop: async () => {},
  handleConfirm: async () => {}, handleCancel: async () => {}, handleUndo: async () => {}, handleFeedback: async () => {},
  dismissForm: () => {}, conversationId: null, conversations: [], conversationsLoading: false,
  refreshConversations: async () => {}, startNewConversation: () => {},
  openConversation: async () => {}, deleteConversation: async () => {}, renameConversation: async () => {}, launcherOpen: false, setLauncherOpen: () => {}, handleClearChat: () => {}, handleRunExecute: async () => {}, handleRunCancel: () => {}, handleRunUndo: async () => {},
} satisfies ChatController;

describe("POS ZanAI widget", () => {
  it("renders accessible floating chat with removable till context", () => {
    const html = renderToStaticMarkup(
      <PosZanAiWidgetView
        ctrl={ctrl}
        userName="Cashier"
        businessName="Main Branch"
        composerRef={createRef<HTMLTextAreaElement>()}
        open
        expanded={false}
        unreadCount={2}
        contextAttached
        canMutate={false}
        suppressed={false}
        onOpen={() => {}}
        onMinimize={() => {}}
        onClose={() => {}}
        onToggleExpand={() => {}}
        onRemoveContext={() => {}}
        getSendContext={() => undefined}
        onComposerKeyDown={() => false}
      />,
    );

    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-label="ZanAI"');
    expect(html).toContain("Till context");
    expect(html).toContain('aria-label="Remove till context"');
    expect(html).toContain("Read-only assistant");
  });

  it("renders a labelled launcher and unread badge while minimized", () => {
    const props: ComponentProps<typeof PosZanAiWidgetView> & {
      launcherStyle: { left: number; top: number };
      launcherDraggable: boolean;
    } = {
        ctrl,
        userName: "Cashier",
        businessName: "Main Branch",
        composerRef: createRef<HTMLTextAreaElement>(),
        open: false,
        expanded: false,
        unreadCount: 2,
        contextAttached: true,
        canMutate: false,
        suppressed: false,
        onOpen: () => {},
        onMinimize: () => {},
        onClose: () => {},
        onToggleExpand: () => {},
        onRemoveContext: () => {},
        getSendContext: () => undefined,
        onComposerKeyDown: () => false,
        launcherStyle: { left: 120, top: 80 },
        launcherDraggable: true,
      };
    const html = renderToStaticMarkup(createElement(PosZanAiWidgetView, props));

    expect(html).toContain('aria-label="Open ZanAI"');
    expect(html).toContain('aria-label="2 unread"');
    expect(html).toContain('data-draggable="true"');
    expect(html).toContain('left:120px;top:80px');
  });
});
