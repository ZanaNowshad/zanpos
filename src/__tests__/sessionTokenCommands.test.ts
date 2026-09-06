import type { SessionToken } from "../types";
import { asSessionToken } from "../types";
import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn(() => Promise.resolve(undefined)));

vi.mock("@tauri-apps/api/core", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tauri-apps/api/core")>();
  return { ...actual, invoke };
});

import * as commands from "../tauri/commands";

const TOKEN = asSessionToken("session-secret");

describe("authenticated Office AI command wrappers", () => {
  beforeEach(() => invoke.mockClear());

  it("sends the opaque session token instead of a caller-supplied actor ID", async () => {
    await commands.adminGetProviderConfig(TOKEN);

    expect(invoke).toHaveBeenCalledWith("admin_get_provider_config", {
      sessionToken: asSessionToken("session-secret"),
    });
  });

  it("sends the session token with chat persistence requests", async () => {
    await commands.aiLoadHistory(TOKEN, "branch-1");

    expect(invoke).toHaveBeenCalledWith("ai_load_history", {
      sessionToken: asSessionToken("session-secret"),
      branchId: "branch-1",
    });
  });

  it("loads resumable AI task state through the authenticated branch boundary", async () => {
    await commands.aiGetTaskLedgerResume(TOKEN, "branch-1");

    expect(invoke).toHaveBeenCalledWith("ai_get_task_ledger_resume", {
      sessionToken: asSessionToken("session-secret"),
      branchId: "branch-1",
    });
  });

  it("exposes a logout wrapper that revokes the opaque session", async () => {
    const authLogout = (commands as typeof commands & {
      authLogout: (sessionToken: SessionToken) => Promise<void>;
    }).authLogout;

    expect(authLogout).toBeTypeOf("function");
    await authLogout(TOKEN);
    expect(invoke).toHaveBeenCalledWith("auth_logout", {
      sessionToken: asSessionToken("session-secret"),
    });
  });

  it("does not trust an actor ID for AI catalog extraction", async () => {
    await commands.catalogImportExtract("media-1", 3, TOKEN);

    expect(invoke).toHaveBeenCalledWith("catalog_import_extract", {
      mediaId: "media-1",
      currencyExponent: 3,
      sessionToken: asSessionToken("session-secret"),
    });
  });
});
