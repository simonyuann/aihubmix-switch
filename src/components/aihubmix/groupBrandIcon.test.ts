import { describe, expect, it } from "vitest";
import { groupBrandIcon } from "./groupBrandIcon";

describe("groupBrandIcon", () => {
  it("prefers the group brand over a generic OpenAI protocol", () => {
    expect(groupBrandIcon("DeepSeek V4", "openai")).toBe("deepseek");
    expect(groupBrandIcon("GLM-5", "openai")).toBe("zhipu");
    expect(groupBrandIcon("Kimi K2", "openai")).toBe("kimi");
  });

  it.each([
    ["Grok订阅", "grok"],
    ["Kimi_K2", "kimi"],
    ["MiniMax2", "minimax"],
    ["OpenCode Go", "opencode"],
    ["OpenAI订阅", "aihubmix"],
    ["OpenAI Subscription", "aihubmix"],
    ["OpenAI API", "openai"],
  ])("recognizes %s independently of the protocol", (name, icon) => {
    expect(groupBrandIcon(name, "openai")).toBe(icon);
  });

  it("uses the group subscription type to distinguish OpenAI services", () => {
    expect(groupBrandIcon("OpenAI", "subscription", "openai")).toBe("aihubmix");
  });

  it("leaves unknown brands without a misleading icon", () => {
    expect(groupBrandIcon("Custom service")).toBeUndefined();
  });
});
