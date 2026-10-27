import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AboutSection } from "@/components/settings/AboutSection";
import { settingsApi } from "@/lib/api";

const update = vi.hoisted(() => ({
  hasUpdate: false,
  updateInfo: {
    availableVersion: "1.1.2",
    notes: "Release notes must stay hidden",
  },
  checkUpdate: vi.fn().mockResolvedValue(false),
  resetDismiss: vi.fn(),
  isChecking: false,
}));
vi.mock("@/contexts/UpdateContext", () => ({ useUpdate: () => update }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "1.1.1" }));

describe("AboutSection", () => {
  beforeEach(() => {
    update.hasUpdate = false;
  });

  it("opens the official website and keeps checking for updates", async () => {
    const open = vi
      .spyOn(settingsApi, "openExternal")
      .mockResolvedValue(undefined);
    render(<AboutSection isPortable={false} />);
    fireEvent.click(
      screen.getByRole("button", { name: "settings.officialWebsite" }),
    );
    expect(open).toHaveBeenCalledWith(
      "https://dl.csmy.fun:23457/?github=gh-2dfe2a292f49bc85",
    );
    fireEvent.click(
      screen.getByRole("button", { name: "settings.checkForUpdates" }),
    );
    await waitFor(() => expect(update.checkUpdate).toHaveBeenCalled());
    expect(
      screen.queryByRole("button", { name: "settings.releaseNotes" }),
    ).toBeNull();
    expect(
      screen.queryByRole("button", { name: "whatsNew.recentTitle" }),
    ).toBeNull();
    open.mockRestore();
  });

  it("hides release notes even when an update is available", () => {
    update.hasUpdate = true;
    render(<AboutSection isPortable />);
    expect(screen.queryByText(update.updateInfo.notes)).toBeNull();
    expect(
      screen.getByRole("button", { name: "settings.updateTo" }),
    ).toBeInTheDocument();
    expect(screen.getByText("settings.portableUpdateHint")).toBeInTheDocument();
  });
});
