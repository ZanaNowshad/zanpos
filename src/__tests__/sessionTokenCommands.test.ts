import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn(() => Promise.resolve(undefined)));

vi.mock("@tauri-apps/api/core", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tauri-apps/api/core")>();
  return { ...actual, invoke };
});

import * as commands from "../tauri/commands";

describe("authenticated Office AI command wrappers", () => {
  beforeEach(() => invoke.mockClear());

  it("sends the opaque session token instead of a caller-supplied actor ID", async () => {
    await commands.adminGetProviderConfig("session-secret");

    expect(invoke).toHaveBeenCalledWith("admin_get_provider_config", {
      sessionToken: "session-secret",
    });
  });

  it("sends the session token with chat persistence requests", async () => {
    await commands.aiLoadHistory("session-secret", "branch-1");

    expect(invoke).toHaveBeenCalledWith("ai_load_history", {
      sessionToken: "session-secret",
      branchId: "branch-1",
    });
  });

  it("loads resumable AI task state through the authenticated branch boundary", async () => {
    await commands.aiGetTaskLedgerResume("session-secret", "branch-1");

    expect(invoke).toHaveBeenCalledWith("ai_get_task_ledger_resume", {
      sessionToken: "session-secret",
      branchId: "branch-1",
    });
  });

  it("exposes a logout wrapper that revokes the opaque session", async () => {
    const authLogout = (commands as typeof commands & {
      authLogout: (sessionToken: string) => Promise<void>;
    }).authLogout;

    expect(authLogout).toBeTypeOf("function");
    await authLogout("session-secret");
    expect(invoke).toHaveBeenCalledWith("auth_logout", {
      sessionToken: "session-secret",
    });
  });

  it("does not trust an actor ID for AI catalog extraction", async () => {
    await commands.catalogImportExtract("media-1", 3, "session-secret");

    expect(invoke).toHaveBeenCalledWith("catalog_import_extract", {
      mediaId: "media-1",
      currencyExponent: 3,
      sessionToken: "session-secret",
    });
  });
});
