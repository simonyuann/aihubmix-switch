import { describe, expect, it } from "vitest";
import { codexMarketError } from "@/lib/codexPluginMarket";

describe("plugin market error messages", () => {
  it("explains a GitHub network failure without exposing raw process errors", () => {
    expect(
      codexMarketError("CODEX_MARKET_GIT_NETWORK_ERROR", "generic"),
    ).toContain("无法连接 GitHub");
    expect(
      codexMarketError("fatal: failed to connect token=secret", "generic"),
    ).toBe("generic");
  });
  it("only displays explicit safe backend messages", () => {
    expect(codexMarketError("插件目录已更新，请刷新后再安装", "generic")).toBe(
      "插件目录已更新，请刷新后再安装",
    );
    expect(codexMarketError(new Error("untrusted credential"), "generic")).toBe(
      "generic",
    );
  });
});
