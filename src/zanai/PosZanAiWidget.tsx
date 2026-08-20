import { useCallback, useEffect, useRef, useState } from "react";
import type { CSSProperties, KeyboardEvent, PointerEvent as ReactPointerEvent, RefObject } from "react";
import { Maximize2, Minimize2, Minus, Sparkles, X } from "lucide-react";
import { DEVICE, type SessionUser } from "../types";
import type { ChatController } from "../officeai/useChatController";
import ChatPanel from "../officeai/ChatPanel";
import ConfirmActionModal from "../components/ConfirmActionModal";
import { useZanAi } from "./useZanAi";
import type { PosAiContext, ZanAiSurfaceContext } from "./zanAiTypes";
import { summarizePosAiContext } from "./posContext";
import { usePosZanAiWindow } from "./usePosZanAiWindow";
import { useScannerBurstGuard } from "./useScannerBurstGuard";
import { useLanguage } from "../hooks/useLanguage";
import { officeAiTranslator } from "../i18n/officeAiStrings";
import "./zanAiWidget.css";

interface ViewProps {
  ctrl: ChatController;
  userName: string;
  businessName: string;
  composerRef: RefObject<HTMLTextAreaElement | null>;
  open: boolean;
  expanded: boolean;
  unreadCount: number;
  contextAttached: boolean;
  canMutate: boolean;
  suppressed: boolean;
  windowStyle?: CSSProperties;
  launcherStyle?: CSSProperties;
  launcherDraggable?: boolean;
  onHeaderPointerDown?: (event: ReactPointerEvent) => void;
  onLauncherPointerDown?: (event: ReactPointerEvent) => void;
  onOpen: () => void;
  onMinimize: () => void;
  onClose: () => void;
  onToggleExpand: () => void;
  onRemoveContext: () => void;
  onAttachContext?: () => void;
  getSendContext: () => ZanAiSurfaceContext | undefined;
  onComposerKeyDown: (event: KeyboardEvent<HTMLTextAreaElement>) => boolean;
}

export function PosZanAiWidgetView({
  ctrl,
  userName,
  businessName,
  composerRef,
  open,
  expanded,
  unreadCount,
  contextAttached,
  canMutate,
  suppressed,
  windowStyle,
  launcherStyle,
  launcherDraggable = false,
  onHeaderPointerDown,
  onLauncherPointerDown,
  onOpen,
  onMinimize,
  onClose,
  onToggleExpand,
  onRemoveContext,
  onAttachContext,
  getSendContext,
  onComposerKeyDown,
}: ViewProps) {
  const { language } = useLanguage();
  const t = officeAiTranslator(language);
  if (suppressed) return null;
  if (!open) {
    return (
      <button
        className="zanai-pos-launcher"
        style={launcherStyle}
        data-draggable={launcherDraggable ? "true" : undefined}
        onPointerDown={onLauncherPointerDown}
        onClick={onOpen}
        aria-label={t("posOpenZanAi")}
        title="Drag to move · Click to open"
      >
        <Sparkles size={20} />
        <span>ZanAI</span>
        {unreadCount > 0 && (
          <span className="zanai-pos-unread" aria-label={`${unreadCount} ${t("posUnread")}`}>{unreadCount}</span>
        )}
      </button>
    );
  }

  return (
    <section
      className={`zanai-pos-window${expanded ? " zanai-pos-window-expanded" : ""}`}
      style={expanded ? undefined : windowStyle}
      role="dialog"
      aria-label="ZanAI"
    >
      <header className="zanai-pos-header" onPointerDown={onHeaderPointerDown}>
        <div><Sparkles size={17} /><strong>ZanAI</strong><span>{t("posContext")}</span></div>
        <div className="zanai-pos-window-actions">
          <button onClick={onMinimize} aria-label={t("posMinimizeZanAi")}><Minus size={16} /></button>
          <button onClick={onToggleExpand} aria-label={expanded ? t("posRestoreZanAi") : t("posExpandZanAi")}>
            {expanded ? <Minimize2 size={16} /> : <Maximize2 size={16} />}
          </button>
          <button onClick={onClose} aria-label={t("posCloseZanAi")}><X size={16} /></button>
        </div>
      </header>
      <div className="zanai-pos-access-banner">
        {canMutate ? t("posManagerConfirmation") : t("posCashierReadOnly")}
      </div>
      {contextAttached && (
        <div className="zanai-pos-context-chip">
          <span>{t("posTillContext")}</span>
          <button onClick={onRemoveContext} aria-label={t("posRemoveTillContext")}><X size={13} /></button>
        </div>
      )}
      {!contextAttached && onAttachContext && (
        <button className="zanai-pos-context-add" onClick={onAttachContext}>
          {t("posAttachTillContext")}
        </button>
      )}
      <ChatPanel
        ctrl={ctrl}
        variant="pos"
        userName={userName}
        businessName={businessName}
        composerRef={composerRef}
        canMutate={canMutate}
        getSendContext={getSendContext}
        onComposerKeyDown={onComposerKeyDown}
      />
      {canMutate && ctrl.chatState === "confirm" && (
        <ConfirmActionModal
          preview={ctrl.pendingAction?.preview}
          previews={ctrl.pendingBatchActions ?? undefined}
          expiresAt={ctrl.pendingAction?.expires_at}
          onConfirm={ctrl.handleConfirm}
          onCancel={ctrl.handleCancel}
        />
      )}
    </section>
  );
}

interface Props {
  sessionUser: SessionUser;
  branchName: string;
  suppressed: boolean;
  buildContext: (capturedAt: string) => PosAiContext;
  focusBarcode: () => void;
}

export default function PosZanAiWidget({
  sessionUser,
  branchName,
  suppressed,
  buildContext,
  focusBarcode,
}: Props) {
  const { ctrl, registerSurfaceContext, uiState, dispatchUi } = useZanAi();
  const composerRef = useRef<HTMLTextAreaElement>(null);
  const [contextAttached, setContextAttached] = useState(true);
  const windowState = usePosZanAiWindow(
    `zanai-pos-window:${DEVICE.device_id}:${sessionUser.user_id}`,
    suppressed,
    focusBarcode,
  );

  const currentContext = useCallback(() => buildContext(new Date().toISOString()), [buildContext]);
  const getSendContext = useCallback((): ZanAiSurfaceContext | undefined => {
    if (!contextAttached) return undefined;
    const structured = currentContext();
    return { surface: "pos", summary: summarizePosAiContext(structured), structured };
  }, [contextAttached, currentContext]);

  useEffect(() => {
    const structured = currentContext();
    registerSurfaceContext({ surface: "pos", summary: summarizePosAiContext(structured), structured });
  }, [currentContext, registerSurfaceContext]);

  useEffect(() => {
    dispatchUi({ type: windowState.visible ? "open_widget" : "minimize_widget" });
  }, [dispatchUi, windowState.visible]);

  const onComposerKeyDown = useScannerBurstGuard({ focusBarcode });

  return (
    <PosZanAiWidgetView
      ctrl={ctrl}
      userName={sessionUser.display_name}
      businessName={branchName}
      composerRef={composerRef}
      open={windowState.visible}
      expanded={windowState.expanded}
      unreadCount={uiState.unreadCount}
      contextAttached={contextAttached}
      canMutate={sessionUser.role_name === "manager" || sessionUser.role_name === "owner"}
      suppressed={suppressed}
      windowStyle={{
        left: windowState.rect.x,
        top: windowState.rect.y,
        width: windowState.rect.width,
        height: windowState.rect.height,
      }}
      launcherStyle={{ left: windowState.launcher.x, top: windowState.launcher.y }}
      launcherDraggable
      onHeaderPointerDown={windowState.beginDrag}
      onLauncherPointerDown={windowState.beginLauncherDrag}
      onOpen={windowState.open}
      onMinimize={windowState.minimize}
      onClose={windowState.close}
      onToggleExpand={windowState.toggleExpanded}
      onRemoveContext={() => setContextAttached(false)}
      onAttachContext={() => setContextAttached(true)}
      getSendContext={getSendContext}
      onComposerKeyDown={onComposerKeyDown}
    />
  );
}
