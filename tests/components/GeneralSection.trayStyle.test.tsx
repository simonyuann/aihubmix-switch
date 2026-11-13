import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import "@testing-library/jest-dom";
import { GeneralSection } from "@/components/settings/sections/GeneralSection";
import type { SettingsFormState } from "@/hooks/useSettings";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock("@/components/theme-provider", () => ({
  useTheme: () => ({ theme: "system", setTheme: vi.fn() }),
}));

vi.mock("@/components/settings/TerminalSettings", () => ({
  TerminalSelect: () => null,
}));

const platform = vi.hoisted(() => ({ linux: false }));
vi.mock("@/lib/platform", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/platform")>()),
  isLinux: () => platform.linux,
}));

const baseSettings = {
  showInTray: true,
  minimizeToTrayOnClose: true,
  language: "zh",
} as SettingsFormState;

function renderSection(settings: Partial<SettingsFormState> = {}) {
  const onAutoSave = vi.fn().mockResolvedValue(true);
  render(
    <GeneralSection
      settings={{ ...baseSettings, ...settings }}
      onAutoSave={onAutoSave}
      onOpenApps={vi.fn()}
    />,
  );
  return onAutoSave;
}

describe("GeneralSection tray style", () => {
  beforeEach(() => {
    platform.linux = false;
    localStorage.clear();
  });

  it("only shows sidebar controls in the advanced interface without changing saved settings", () => {
    const onAutoSave = renderSection({
      showProfileSwitcher: true,
      showProviderSearch: true,
    });
    expect(
      screen.queryByText("settings.general.sidebarAndHeader"),
    ).not.toBeInTheDocument();
    expect(onAutoSave).not.toHaveBeenCalled();
    localStorage.setItem("aihubmix:advanced-interface", "true");
    fireEvent(window, new Event("aihubmix:interface-changed"));
    expect(
      screen.getByText("settings.general.sidebarAndHeader"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("switch", {
        name: "settings.appVisibility.showProfileSwitcher",
      }),
    ).toBeChecked();
    expect(
      screen.getByRole("switch", {
        name: "settings.appVisibility.showProviderSearch",
      }),
    ).toBeChecked();
    expect(onAutoSave).not.toHaveBeenCalled();
  });

  it("keeps other providers disabled by default and saves explicit opt-in", () => {
    const onAutoSave = renderSection();
    const toggle = screen.getByRole("switch", { name: "启用其他供应商" });
    expect(toggle).not.toBeChecked();
    fireEvent.click(toggle);
    expect(onAutoSave).toHaveBeenCalledWith({ enableOtherProviders: true });
  });

  it("can hide other providers again without deleting their configuration", () => {
    const onAutoSave = renderSection({ enableOtherProviders: true });
    const toggle = screen.getByRole("switch", { name: "启用其他供应商" });
    expect(toggle).toBeChecked();
    fireEvent.click(toggle);
    expect(onAutoSave).toHaveBeenCalledWith({ enableOtherProviders: false });
  });

  it("defaults to the panel and saves the native menu when Menu is picked", () => {
    const onAutoSave = renderSection();
    const group = screen.getByRole("group", {
      name: "settings.general.trayStyle",
    });
    expect(group).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "settings.general.trayStylePanel" }),
    ).toHaveAttribute("aria-pressed", "true");

    fireEvent.click(
      screen.getByRole("button", { name: "settings.general.trayStyleMenu" }),
    );
    expect(onAutoSave).toHaveBeenCalledWith({ nativeTrayMenu: true });
  });

  it("switches back to the panel", () => {
    const onAutoSave = renderSection({ nativeTrayMenu: true });
    fireEvent.click(
      screen.getByRole("button", { name: "settings.general.trayStylePanel" }),
    );
    expect(onAutoSave).toHaveBeenCalledWith({ nativeTrayMenu: false });
  });

  it("is hidden on Linux, which only has the native menu", () => {
    platform.linux = true;
    renderSection();
    expect(
      screen.queryByRole("group", { name: "settings.general.trayStyle" }),
    ).not.toBeInTheDocument();
  });
});
