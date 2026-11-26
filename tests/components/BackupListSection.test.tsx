import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { BackupListSection } from "@/components/settings/BackupListSection";

const { restore, retrySync, toastSuccess } = vi.hoisted(() => ({
  restore: vi.fn(),
  retrySync: vi.fn(),
  toastSuccess: vi.fn(),
}));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock("@/lib/toast", () => ({
  toast: { success: toastSuccess, error: vi.fn() },
}));
vi.mock("@/hooks/useBackupManager", () => ({
  useBackupManager: () => ({
    backups: [
      {
        filename: "backup.db",
        createdAt: "2026-01-01T00:00:00Z",
        sizeBytes: 1024,
      },
    ],
    restore,
    retrySync,
  }),
}));
vi.mock("@/components/settings/BackupStorageSection", () => ({
  BackupStorageSection: () => null,
  formatBytes: String,
}));

it("lets the user retry a failed projection without restoring the database again", async () => {
  restore.mockResolvedValue({
    backupId: "safety.db",
    warning: "Cannot write AGENTS.md",
  });
  retrySync
    .mockRejectedValueOnce(new Error("File is still locked"))
    .mockResolvedValueOnce(undefined);
  render(<BackupListSection onSettingsChange={vi.fn()} />);
  const restoreLabel = { name: "settings.backupManager.restore" };
  fireEvent.click(screen.getByRole("button", restoreLabel));
  fireEvent.click(
    within(screen.getByRole("dialog")).getByRole("button", restoreLabel),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Cannot write AGENTS.md",
  );
  expect(toastSuccess).not.toHaveBeenCalled();
  const retry = screen.getByRole("button", {
    name: "settings.backupManager.retrySync",
  });
  fireEvent.click(retry);
  expect(await screen.findByText("File is still locked")).toBeInTheDocument();
  fireEvent.click(retry);
  await waitFor(() =>
    expect(screen.queryByRole("alert")).not.toBeInTheDocument(),
  );
  expect(restore).toHaveBeenCalledTimes(1);
  expect(restore).toHaveBeenCalledWith("backup.db");
  expect(retrySync).toHaveBeenCalledTimes(2);
});
