import { expect, it, vi } from "vitest";
import { checkForUpdate } from "@/lib/updater";

const { check } = vi.hoisted(() => ({ check: vi.fn() }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "1.0.0" }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: async () => "stable" }));

it("copies the release notes and closes the native resource after a check", async () => {
  const close = vi.fn().mockResolvedValue(undefined);
  check.mockResolvedValue({
    version: "2.0.0",
    body: "更新说明",
    date: "2026-11-26",
    close,
  });
  expect(await checkForUpdate()).toEqual({
    status: "available",
    info: {
      currentVersion: "1.0.0",
      availableVersion: "2.0.0",
      notes: "更新说明",
      pubDate: "2026-11-26",
    },
  });
  expect(close).toHaveBeenCalledOnce();
});
