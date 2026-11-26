import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ dispatch: vi.fn(), copyText: vi.fn() }));
vi.mock("@/lib/api/aihubmix", () => ({ dispatch: mocks.dispatch }));
vi.mock("@/components/shell/AppPageHeader", () => ({
  AppPageHeader: ({ title }: { title: string }) => <h1>{title}</h1>,
}));
vi.mock("@/lib/clipboard", () => ({ copyText: mocks.copyText }));
import { HelpPage } from "@/components/aihubmix/HelpPage";

function showHelp() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <HelpPage />
    </QueryClientProvider>,
  );
}
beforeEach(() => vi.resetAllMocks());
afterEach(cleanup);

describe("support contact", () => {
  it("shows the public contact as text without creating links or HTML", async () => {
    mocks.dispatch.mockResolvedValue({
      contactInfo: '<a href="https://example.invalid/admin">QQ: 123</a>',
    });
    const { container } = showHelp();
    expect(await screen.findByText(/QQ: 123/)).toBeInTheDocument();
    expect(container.querySelector("a")).toBeNull();
    expect(mocks.dispatch).toHaveBeenCalledWith("support-contact");
  });
  it("offers retry when the service is unavailable", async () => {
    mocks.dispatch
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValue({ contactInfo: "QQ: 123" });
    showHelp();
    const retry = await screen.findByRole("button", { name: "重试" });
    expect(
      screen.getByRole("img", { name: "AIHubMix QQ 交流群二维码" }),
    ).toBeVisible();
    fireEvent.click(retry);
    expect(await screen.findByText("QQ: 123")).toBeInTheDocument();
  });
  it("shows the bundled QR and copies the group number when the platform contact is empty", async () => {
    mocks.dispatch.mockResolvedValue({ contactInfo: "" });
    mocks.copyText.mockResolvedValue(undefined);
    showHelp();
    expect(
      screen.getByRole("img", { name: "AIHubMix QQ 交流群二维码" }),
    ).toBeVisible();
    expect(screen.getByText("1091823578")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "复制群号" }));
    expect(mocks.copyText).toHaveBeenCalledWith("1091823578");
    await waitFor(() =>
      expect(screen.queryByRole("status")).not.toBeInTheDocument(),
    );
    expect(screen.queryByText("暂未公布联系方式。")).not.toBeInTheDocument();
  });
});
