import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { UpdateProvider, useUpdate } from "@/contexts/UpdateContext";

const check = vi.hoisted(() => vi.fn());
vi.mock("@/lib/updater", () => ({ checkForUpdate: check }));

describe("automatic app update checks", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    check.mockReset();
    localStorage.clear();
  });
  afterEach(() => vi.useRealTimers());

  it("checks at startup and again after an hour without losing an update on network failure", async () => {
    check
      .mockResolvedValueOnce({
        status: "available",
        info: { currentVersion: "1.1.3", availableVersion: "1.1.4" },
      })
      .mockRejectedValueOnce(new Error("offline"));
    const log = vi.spyOn(console, "error").mockImplementation(() => {});
    const { result, unmount } = renderHook(() => useUpdate(), {
      wrapper: UpdateProvider,
    });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(result.current.hasUpdate).toBe(true);
    expect(result.current.updateInfo?.availableVersion).toBe("1.1.4");
    await act(async () => {
      await vi.advanceTimersByTimeAsync(60 * 60 * 1000 - 1000);
    });
    expect(check).toHaveBeenCalledTimes(2);
    expect(result.current.hasUpdate).toBe(true);
    expect(result.current.updateInfo?.availableVersion).toBe("1.1.4");
    unmount();
    log.mockRestore();
  });
});
