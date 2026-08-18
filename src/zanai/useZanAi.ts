import { createContext, useContext } from "react";
import type { Dispatch } from "react";
import type { ChatController } from "../officeai/useChatController";
import type { ZanAiSurfaceContext, ZanAiUiAction, ZanAiUiState } from "./zanAiTypes";

export interface ZanAiNavigationRequest {
  id: string;
  tab: string;
}

export interface ZanAiContextValue {
  ctrl: ChatController;
  registerSurfaceContext: (context: ZanAiSurfaceContext) => void;
  navigationRequest: ZanAiNavigationRequest | null;
  consumeNavigationRequest: (id: string) => void;
  dataEpoch: number;
  uiState: ZanAiUiState;
  dispatchUi: Dispatch<ZanAiUiAction>;
}

export const ZanAiContext = createContext<ZanAiContextValue | null>(null);

export function useZanAi(): ZanAiContextValue {
  const value = useContext(ZanAiContext);
  if (!value) throw new Error("useZanAi must be used inside ZanAiProvider");
  return value;
}
