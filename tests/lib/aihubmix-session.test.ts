import { invoke } from "@tauri-apps/api/core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { dispatch } from "@/lib/api/aihubmix";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
afterEach(() => vi.restoreAllMocks());

describe("account session error events", () => {
  it("keeps the session when a tool reports a rejected API key", async () => {
    const emit = vi.spyOn(window, "dispatchEvent");
    vi.mocked(invoke).mockRejectedValueOnce("登录已过期，请重新登录");
    await expect(dispatch("models")).rejects.toBe("登录已过期，请重新登录");
    expect(emit).not.toHaveBeenCalled();
  });

  it("expires the session only when the backend requires account login", async () => {
    const emit = vi.spyOn(window, "dispatchEvent");
    const error = "AUTH_REQUIRED: 登录已失效，请重新登录";
    vi.mocked(invoke).mockRejectedValueOnce(error);
    await expect(dispatch("keys")).rejects.toBe(error);
    expect(emit).toHaveBeenCalledWith(
      expect.objectContaining({ type: "aihubmix:session-expired" }),
    );
  });
});
