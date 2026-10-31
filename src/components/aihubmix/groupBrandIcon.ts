// Names are more specific than the platform protocol (often simply "openai").
export function groupBrandIcon(
  ...parts: (string | null | undefined)[]
): string | undefined {
  const text = parts.filter(Boolean).join(" ").toLowerCase();
  const brands: [RegExp, string][] = [
    [/deep\s*seek|深度求索/, "deepseek"],
    [/\bglm\b|chatglm|zhipu|智谱/, "zhipu"],
    [/\bkimi\b|moonshot|月之暗面/, "kimi"],
    [/\bqwen\b|通义|千问/, "qwen"],
    [/minimax|海螺/, "minimax"],
    [/\bgemini\b|google/, "gemini"],
    [/\bclaude\b|anthropic/, "claude"],
    [/\bgrok\b|\bxai\b/, "grok"],
    [/\bdoubao\b|豆包/, "doubao"],
    [/\bmistral\b/, "mistral"],
    [/\bopenai\b|\bchatgpt\b|\bgpt\b/, "openai"],
  ];
  return brands.find(([pattern]) => pattern.test(text))?.[1];
}
