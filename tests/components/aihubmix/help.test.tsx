import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ dispatch: vi.fn() }));
vi.mock("@/lib/api/aihubmix", () => ({ dispatch: mocks.dispatch }));
vi.mock("@/components/shell/AppPageHeader", () => ({
  AppPageHeader: ({ title }: { title: string }) => <h1>{title}</h1>,
}));
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
    fireEvent.click(await screen.findByRole("button", { name: "重试" }));
    expect(await screen.findByText("QQ: 123")).toBeInTheDocument();
  });
  it("explains when no contact has been published", async () => {
    mocks.dispatch.mockResolvedValue({ contactInfo: "" });
    showHelp();
    expect(await screen.findByText("暂未公布联系方式。")).toBeInTheDocument();
  });
});
