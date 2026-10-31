import { describe, expect, it } from "vitest";
import { groupBrandIcon } from "./groupBrandIcon";

describe("groupBrandIcon", () => {
  it("prefers the group brand over a generic OpenAI protocol", () => {
    expect(groupBrandIcon("DeepSeek V4", "openai")).toBe("deepseek");
    expect(groupBrandIcon("GLM-5", "openai")).toBe("zhipu");
    expect(groupBrandIcon("Kimi K2", "openai")).toBe("kimi");
  });

  it("leaves unknown brands without a misleading icon", () => {
    expect(groupBrandIcon("Custom service")).toBeUndefined();
  });
});
