// T22 LoginPage 批次:硬编码中文 → loginPage.* keys
import fs from "node:fs";
const p = "src/components/aihubmix/LoginPage.tsx";
let s = fs.readFileSync(p, "utf8");

const pairs = [
  [`        setError("未能清除保存的密码，请重试。");`, `        setError(t("loginPage.error.clearSavedPassword"));`],
  [`          setError(friendlyError(e, "连接平台失败，请检查网络后重试。"));`, `          setError(friendlyError(e, t("loginPage.error.connectFailed")));`],
  [`        setNotice("验证码已发送，请检查邮箱");`, `        setNotice(t("loginPage.notice.codeSent"));`],
  [`        setError(friendlyError(e, "验证码发送失败，请稍后重试。"));`, `        setError(friendlyError(e, t("loginPage.error.codeSendFailed")));`],
  [`      setError("两次输入的密码不一致");`, `      setError(t("loginPage.error.passwordMismatch"));`],
  [`          setNotice(
            "若该邮箱已注册，你会收到密码重置邮件。请通过邮件中的链接设置新密码，再返回登录。",
          );`, `          setNotice(t("loginPage.notice.resetEmailSent"));`],
  [`      setError(friendlyError(e, "操作失败，请检查网络后重试。"));`, `      setError(friendlyError(e, t("loginPage.error.operationFailed")));`],
  [`      setError("请重新输入密码");`, `      setError(t("loginPage.error.reenterPassword"));`],
  [`        result?.logoutNotice ||
          result?.revocationWarning ||
          "已退出本机旧账号，请重新登录。",`, `        result?.logoutNotice ||
          result?.revocationWarning ||
          t("loginPage.notice.loggedOutLocal"),`],
  [`      setError(friendlyError(e, "退出账号失败，请重试。"));`, `      setError(friendlyError(e, t("loginPage.error.logoutFailed")));`],
  [`  const title =
    mode === "login"
      ? "登录你的账户"
      : mode === "register"
        ? "创建账户"
        : "找回密码";`, `  const title = t(
    mode === "login"
      ? "loginPage.title.login"
      : mode === "register"
        ? "loginPage.title.register"
        : "loginPage.title.forgot",
  );`],
  [`              <ArrowLeft className="me-2 h-4 w-4" />
              返回登录`, `              <ArrowLeft className="me-2 h-4 w-4" />
              {t("loginPage.action.backToLogin")}`],
  [`            {mode === "login"
              ? "登录后即可配置工具、申请 API 和充值。"
              : mode === "register"
                ? "使用邮箱注册，开启你的 API 服务。"
                : "输入注册邮箱，我们会发送密码重置邮件。"}`, `            {mode === "login"
              ? t("loginPage.subtitle.login")
              : mode === "register"
                ? t("loginPage.subtitle.register")
                : t("loginPage.subtitle.forgot")}`],
  [`              <span>邮箱</span>
                <Input
                  aria-label="邮箱"`, `              <span>{t("loginPage.label.email")}</span>
                <Input
                  aria-label={t("loginPage.label.email")}`],
  [`                  <span>密码</span>
                  <Input
                    aria-label="密码"`, `                  <span>{t("loginPage.label.password")}</span>
                  <Input
                    aria-label={t("loginPage.label.password")}`],
  [`                    placeholder={
                      savedPasswordAvailable ? "已记住密码" : undefined
                    }`, `                    placeholder={
                      savedPasswordAvailable
                        ? t("loginPage.placeholder.savedPassword")
                        : undefined
                    }`],
  [`                    <span>确认密码</span>
                    <Input
                      aria-label="确认密码"`, `                    <span>{t("loginPage.label.confirmPassword")}</span>
                    <Input
                      aria-label={t("loginPage.label.confirmPassword")}`],
  [`                      <span>邮箱验证码</span>
                      <div className="flex gap-2">
                        <Input
                          aria-label="邮箱验证码"`, `                      <span>{t("loginPage.label.emailCode")}</span>
                      <div className="flex gap-2">
                        <Input
                          aria-label={t("loginPage.label.emailCode")}`],
  [`                          {cooldown > 0
                            ? \`\${cooldown} 秒\`
                            : sending
                              ? "发送中…"
                              : "发送验证码"}`, `                          {cooldown > 0
                            ? t("loginPage.register.codeCooldown", { seconds: cooldown })
                            : sending
                              ? t("loginPage.register.sending")
                              : t("loginPage.register.sendCode")}`],
  [`                      aria-label="邀请码"
                      placeholder="邀请码"`, `                      aria-label={t("loginPage.label.invitationCode")}
                      placeholder={t("loginPage.label.invitationCode")}`],
  [`                    记住我`, `                    {t("loginPage.label.rememberMe")}`],
  [`                    忘记密码？`, `                    {t("loginPage.action.forgotPassword")}`],
  [`                    我已阅读并同意以上协议`, `                    {t("loginPage.label.agreeToTerms")}`],
  [`                {mode === "forgot"
                  ? "发送重置邮件"
                  : mode === "register"
                    ? "注册并登录"
                    : "登录"}`, `                {mode === "forgot"
                  ? t("loginPage.action.sendResetEmail")
                  : mode === "register"
                    ? t("loginPage.action.registerAndLogin")
                    : t("loginPage.action.login")}`],
  [`              重新连接平台`, `              {t("loginPage.action.reconnectPlatform")}`],
  [`              退出本机旧账号`, `              {t("loginPage.action.logoutOldAccount")}`],
  [`              还没有账户？{" "}
              <button
                type="button"
                className="font-medium text-action disabled:opacity-40"
                onClick={() => changeMode("register")}
                disabled={busy || !settings?.registrationEnabled}
              >
                立即注册`, `              {t("loginPage.label.noAccountYet")}{" "}
              <button
                type="button"
                className="font-medium text-action disabled:opacity-40"
                onClick={() => changeMode("register")}
                disabled={busy || !settings?.registrationEnabled}
              >
                {t("loginPage.action.registerNow")}`],
  [`import { friendlyError } from "./friendlyError";`, `import { friendlyError } from "./friendlyError";
import { useTranslation } from "react-i18next";`],
];

// useTranslation hook 注入(找组件第一个 hook 行)
const hookAnchor = s.indexOf("const [mode, setMode]"); // LoginPage 状态起点(探测)
let applied = 0;
const missed = [];
for (const [oldStr, newStr] of pairs) {
  if (s.includes(oldStr)) {
    s = s.split(oldStr).join(newStr);
    applied++;
  } else missed.push(oldStr.slice(0, 56).replace(/\n/g, "\\n"));
}
fs.writeFileSync(p, s);
console.log(`applied: ${applied}/${pairs.length}`);
if (missed.length) {
  console.log("MISSED:");
  for (const m of missed) console.log("  -", m);
}
console.log("hook anchor found:", hookAnchor >= 0);
