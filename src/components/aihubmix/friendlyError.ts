/** Keep short, actionable platform messages; never display raw diagnostics. */
export function friendlyError(error: unknown, fallback: string): string {
  const message = (
    typeof error === "string"
      ? error
      : error instanceof Error
        ? error.message
        : ""
  ).trim();
  if (
    !message ||
    message.length > 140 ||
    !/[\u3400-\u9fff]/.test(message) ||
    /[\r\n{}]|https?:\/\/|(?:[A-Za-z]:\\|\/Users\/|\/home\/)|\b(?:stack trace|traceback|bearer|sk-[A-Za-z0-9_-]+)\b/i.test(
      message,
    )
  )
    return fallback;
  return message;
}
