import { describe, expect, it } from "vitest";
import { friendlyError } from "./friendlyError";

describe("friendlyError", () => {
  it("keeps actionable account and payment errors", () => {
    expect(friendlyError("邮箱或密码不正确", "失败")).toBe("邮箱或密码不正确");
    expect(friendlyError("请先退出当前账号，再登录其他账号", "失败")).toContain(
      "请先退出当前账号",
    );
    expect(
      friendlyError("已有待付款订单，请先继续支付或取消。", "失败"),
    ).toContain("待付款");
  });
  it("does not expose technical traces or credentials", () => {
    for (const error of [
      "Error: network timeout",
      "错误: sk-secret123",
      "请求失败 https://example.com/secret",
      "请求失败\nstack trace",
      "故障".repeat(100),
    ]) {
      expect(friendlyError(error, "操作失败，请重试。")).toBe(
        "操作失败，请重试。",
      );
    }
  });
});
