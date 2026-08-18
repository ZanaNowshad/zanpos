import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import type { ReactNode } from "react";
import type { SessionUser } from "../types";
import { useChatController } from "../officeai/useChatController";
import { reduceZanAiUiState } from "./zanAiState";
import type { ZanAiSurfaceContext } from "./zanAiTypes";
import { ZanAiContext } from "./useZanAi";
import type { ZanAiNavigationRequest } from "./useZanAi";

interface Props {
  sessionUser: SessionUser;
  children: ReactNode;
}

export function ZanAiProvider({ sessionUser, children }: Props) {
  const registeredContextRef = useRef<ZanAiSurfaceContext>({
    surface: "office",
    summary: "ZANPOS",
  });
  const [navigationRequest, setNavigationRequest] = useState<ZanAiNavigationRequest | null>(null);
  const [dataEpoch, setDataEpoch] = useState(0);
  const [uiState, dispatchUi] = useReducer(reduceZanAiUiState, {
    activeSurface: "pos",
    widgetOpen: false,
    widgetExpanded: false,
    unreadCount: 0,
  });

  const registerSurfaceContext = useCallback((context: ZanAiSurfaceContext) => {
    registeredContextRef.current = context;
    dispatchUi({ type: "set_surface", surface: context.surface });
  }, []);

  const consumeNavigationRequest = useCallback((id: string) => {
    setNavigationRequest(current => current?.id === id ? null : current);
  }, []);

  const ctrl = useChatController({
    sessionUser,
    getUiContext: () => registeredContextRef.current.summary,
    onNavigate: tab => setNavigationRequest({ id: crypto.randomUUID(), tab }),
    onMutationApplied: () => setDataEpoch(value => value + 1),
  });
  const previousChatStateRef = useRef(ctrl.chatState);
  useEffect(() => {
    const previous = previousChatStateRef.current;
    previousChatStateRef.current = ctrl.chatState;
    if (previous !== "idle" && ctrl.chatState === "idle") {
      dispatchUi({ type: "assistant_result" });
    }
  }, [ctrl.chatState]);

  const value = useMemo(() => ({
    ctrl,
    registerSurfaceContext,
    navigationRequest,
    consumeNavigationRequest,
    dataEpoch,
    uiState,
    dispatchUi,
  }), [
    consumeNavigationRequest,
    ctrl,
    dataEpoch,
    navigationRequest,
    registerSurfaceContext,
    uiState,
  ]);

  return <ZanAiContext.Provider value={value}>{children}</ZanAiContext.Provider>;
}
