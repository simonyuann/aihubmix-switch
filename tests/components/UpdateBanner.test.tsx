import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { UpdateBanner } from "@/components/shell/UpdateBanner";

const update = vi.hoisted(() => ({
  hasUpdate: true,
  updateInfo: { availableVersion: "1.1.4" },
  isDismissed: false,
  dismissUpdate: vi.fn(),
}));
vi.mock("@/contexts/UpdateContext", () => ({ useUpdate: () => update }));

describe("UpdateBanner", () => {
  beforeEach(() => {
    update.hasUpdate = true;
    update.isDismissed = false;
    update.dismissUpdate.mockClear();
  });

  it("shows the available update and opens the update page", () => {
    const open = vi.fn();
    render(<UpdateBanner onOpenUpdate={open} />);
    expect(screen.getByRole("status")).toHaveTextContent(
      "settings.updateAvailable",
    );
    fireEvent.click(screen.getByRole("button", { name: "settings.updateTo" }));
    expect(open).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByRole("button", { name: "common.close" }));
    expect(update.dismissUpdate).toHaveBeenCalledOnce();
  });

  it("does not show a dismissed update or when no update is available", () => {
    update.isDismissed = true;
    const { rerender } = render(<UpdateBanner onOpenUpdate={vi.fn()} />);
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    update.isDismissed = false;
    update.hasUpdate = false;
    rerender(<UpdateBanner onOpenUpdate={vi.fn()} />);
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });
});
