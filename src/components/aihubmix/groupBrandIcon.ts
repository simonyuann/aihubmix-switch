// Names are more specific than the platform protocol (often simply "openai").
export function groupBrandIcon(
  ...parts: (string | null | undefined)[]
): string | undefined {
  const text = parts.filter(Boolean).join(" ").toLowerCase();
  const brands: [RegExp, string][] = [
    [/deep\s*seek|深度求索/, "deepseek"],
    [/\bglm\b|chatglm|zhipu|智谱/, "zhipu"],
    [/kimi|moonshot|月之暗面/, "kimi"],
    [/\bqwen\b|通义|千问/, "qwen"],
    [/mini[\s_-]*max|海螺/, "minimax"],
    [/open[\s_-]*code/, "opencode"],
    [/\bgemini\b|google/, "gemini"],
    [/\bclaude\b|anthropic/, "claude"],
    [/grok|\bx[.\s_-]?ai\b/, "grok"],
    [/\bdoubao\b|豆包/, "doubao"],
    [/\bmistral\b/, "mistral"],
    [
      /(?:open[\s_-]*ai|chatgpt|gpt).*(?:订阅|訂閱|subscription|subscribe)|(?:订阅|訂閱|subscription|subscribe).*(?:open[\s_-]*ai|chatgpt|gpt)/,
      "aihubmix",
    ],
    [/open[\s_-]*ai|chatgpt|\bgpt\b/, "openai"],
  ];
  return brands.find(([pattern]) => pattern.test(text))?.[1];
}
