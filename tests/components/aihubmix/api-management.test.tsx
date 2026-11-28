import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
const mocks = vi.hoisted(() => ({ dispatch: vi.fn(), copy: vi.fn() }));
vi.mock("@/lib/api/aihubmix", () => ({ dispatch: mocks.dispatch }));
vi.mock("@/contexts/AccountContext", () => ({
  useAccount: () => ({
    state: { account: { email: "account@example.test" } },
    logout: vi.fn(),
  }),
}));
import { AccountPage } from "@/components/aihubmix/AccountPage";
const key = {
  id: 7,
  name: "开发工具",
  keyMasked: "sk-••••test",
  groupName: "OpenAI",
  status: "active",
  local: false,
};
beforeEach(() => {
  vi.clearAllMocks();
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText: mocks.copy.mockResolvedValue(undefined) },
  });
  mocks.dispatch.mockImplementation((command) =>
    Promise.resolve(command === "keys" ? [key] : "test-only-secret"),
  );
});
describe("API management", () => {
  it("copies the selected API Key without exposing the secret in the list", async () => {
    render(<AccountPage />);
    fireEvent.click(
      await screen.findByRole("button", { name: "accountPage.aria.copyKey" }),
    );
    await screen.findByText("accountPage.notice.keyCopied");
    expect(mocks.dispatch).toHaveBeenCalledWith("get-key", { id: 7 });
    expect(mocks.copy).toHaveBeenCalledWith("test-only-secret");
    expect(screen.queryByText("test-only-secret")).not.toBeInTheDocument();
  });
  it("only deletes after confirmation and prevents duplicate submissions", async () => {
    let complete!: () => void;
    mocks.dispatch.mockImplementation((command) =>
      command === "delete-key"
        ? new Promise<void>((resolve) => {
            complete = resolve;
          })
        : Promise.resolve([key]),
    );
    render(<AccountPage />);
    const remove = await screen.findByRole("button", {
      name: "accountPage.aria.deleteKey",
    });
    fireEvent.click(remove);
    expect(screen.getByRole("dialog")).toHaveTextContent(
      "accountPage.dialog.deleteBody",
    );
    expect(mocks.dispatch.mock.calls.some(([c]) => c === "delete-key")).toBe(
      false,
    );
    fireEvent.click(
      screen.getByRole("button", { name: "accountPage.action.cancel" }),
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    fireEvent.click(remove);
    const confirm = screen.getByRole("button", {
      name: "accountPage.dialog.confirmDelete",
    });
    fireEvent.click(confirm);
    fireEvent.click(confirm);
    expect(confirm).toBeDisabled();
    expect(
      mocks.dispatch.mock.calls.filter(([c]) => c === "delete-key"),
    ).toEqual([["delete-key", { id: 7 }]]);
    await act(async () => complete());
    await waitFor(() =>
      expect(
        screen.queryByRole("button", { name: "accountPage.aria.deleteKey" }),
      ).not.toBeInTheDocument(),
    );
    expect(screen.getByRole("status")).toHaveTextContent(
      "accountPage.notice.keyDeleted",
    );
  });
  it("keeps the Key visible when deletion fails and asks for a list refresh", async () => {
    mocks.dispatch.mockImplementation((command) =>
      command === "delete-key"
        ? Promise.reject(new Error("network timeout"))
        : Promise.resolve([key]),
    );
    render(<AccountPage />);
    fireEvent.click(
      await screen.findByRole("button", { name: "accountPage.aria.deleteKey" }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "accountPage.dialog.confirmDelete" }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "accountPage.error.deleteUnconfirmedDetail",
    );
    expect(
      screen.getByRole("button", { name: "accountPage.aria.deleteKey" }),
    ).toBeEnabled();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});
