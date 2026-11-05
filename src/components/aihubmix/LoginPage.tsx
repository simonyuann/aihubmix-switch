import { useEffect, useRef, useState } from "react";
import { ArrowLeft, ArrowRight, Check, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useAccount } from "@/contexts/AccountContext";
import { dispatch } from "@/lib/api/aihubmix";
import {
  DRAG_REGION_ATTR,
  DRAG_REGION_STYLE,
  isLinux,
  isWindows,
} from "@/lib/platform";
import { WindowControls } from "@/components/shell/WindowControls";
import { useSettingsQuery } from "@/lib/query";
import logo from "@/assets/icons/aihubmix-a.png";

type Mode = "login" | "register" | "forgot";
interface AuthSettings {
  registrationEnabled: boolean;
  emailVerifyEnabled: boolean;
  passwordResetEnabled: boolean;
  invitationCodeEnabled: boolean;
  captchaRequired: boolean;
}
export function LoginPage() {
  const session = useAccount();
  const { data: windowSettings } = useSettingsQuery();
  const windowControls =
    isWindows() || (isLinux() && windowSettings?.useAppWindowControls);
  const [mode, setMode] = useState<Mode>("login");
  const [settings, setSettings] = useState<AuthSettings | null>(null);
  const [settingsAttempt, setSettingsAttempt] = useState(0);
  const [settingsLoading, setSettingsLoading] = useState(true);
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [code, setCode] = useState("");
  const [invitation, setInvitation] = useState("");
  const [busy, setBusy] = useState(false);
  const [sending, setSending] = useState(false);
  const [cooldown, setCooldown] = useState(0);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const gate = useRef(false);
  const active = useRef(true);
  useEffect(() => {
    active.current = true;
    let current = true;
    setSettingsLoading(true);
    setError("");
    dispatch<AuthSettings>("auth-settings")
      .then((value) => {
        if (current) setSettings(value);
      })
      .catch((e) => {
        if (current) setError(String(e));
      })
      .finally(() => {
        if (current) setSettingsLoading(false);
      });
    return () => {
      current = false;
      active.current = false;
    };
  }, [settingsAttempt]);
  useEffect(() => {
    if (cooldown <= 0) return;
    const timer = window.setTimeout(
      () => setCooldown((value) => value - 1),
      1000,
    );
    return () => clearTimeout(timer);
  }, [cooldown]);
  function changeMode(next: Mode) {
    setMode(next);
    setError("");
    setNotice("");
    setPassword("");
    setConfirm("");
    setCode("");
  }
  async function sendCode() {
    if (gate.current || cooldown || !email.trim()) return;
    gate.current = true;
    setSending(true);
    setError("");
    setNotice("");
    try {
      await dispatch("send-code", { email: email.trim() });
      if (active.current) {
        setNotice("验证码已发送，请检查邮箱");
        setCooldown(60);
      }
    } catch (e) {
      if (active.current) setError(String(e));
    } finally {
      gate.current = false;
      if (active.current) setSending(false);
    }
  }
  async function submit() {
    if (gate.current) return;
    setError("");
    setNotice("");
    if (mode === "register" && password !== confirm) {
      setError("两次输入的密码不一致");
      return;
    }
    gate.current = true;
    setBusy(true);
    try {
      if (mode === "forgot") {
        await dispatch("forgot-password", { email: email.trim() });
        if (active.current)
          setNotice(
            "若该邮箱已注册，你会收到密码重置邮件。请通过邮件中的链接设置新密码，再返回登录。",
          );
      } else {
        await session.authenticate(mode, {
          email: email.trim(),
          password,
          emailCode: code.trim(),
          invitationCode: invitation.trim(),
        });
      }
    } catch (e) {
      if (active.current) setError(String(e));
    } finally {
      gate.current = false;
      if (active.current) setBusy(false);
    }
  }
  const title =
    mode === "login"
      ? "登录你的账户"
      : mode === "register"
        ? "创建账户"
        : "找回密码";
  return (
    <div className="flex h-screen flex-col overflow-hidden bg-app text-fg-1">
      <div className="flex h-12 shrink-0 items-center">
        <div
          className="h-full flex-1"
          {...DRAG_REGION_ATTR}
          style={DRAG_REGION_STYLE as React.CSSProperties}
        />
        {windowControls && (
          <div className="flex gap-1 px-3">
            <WindowControls />
          </div>
        )}
      </div>
      <main className="flex min-h-0 flex-1 items-center justify-center overflow-y-auto px-6 py-10">
        <div className="aihub-page-enter w-full max-w-[400px]">
          <div className="mb-10 flex items-center justify-center gap-3">
            <img src={logo} alt="" className="h-10 w-10 rounded-xl" />
            <span className="text-xl font-semibold tracking-tight">
              AIHubMix Switch
            </span>
          </div>
          {mode !== "login" && (
            <Button
              variant="ghost"
              className="-ms-3 mb-5"
              onClick={() => changeMode("login")}
              disabled={busy || sending}
            >
              <ArrowLeft className="me-2 h-4 w-4" />
              返回登录
            </Button>
          )}
          <h2 className="text-page font-semibold">{title}</h2>
          <p className="mb-7 mt-2 text-body text-fg-3">
            {mode === "login"
              ? "登录后即可配置工具、申请 API 和充值。"
              : mode === "register"
                ? "使用邮箱注册，开启你的 API 服务。"
                : "输入注册邮箱，我们会发送密码重置邮件。"}
          </p>
          {settings?.captchaRequired ? (
            <div className="space-y-4">
              <p className="text-body text-fg-2">
                平台已开启当前客户端暂不支持的安全验证，请联系管理员。
              </p>
            </div>
          ) : (
            <form
              className="space-y-4"
              onSubmit={(e) => {
                e.preventDefault();
                void submit();
              }}
            >
              <label className="block space-y-2 text-body">
                <span>邮箱</span>
                <Input
                  aria-label="邮箱"
                  type="email"
                  autoComplete="username"
                  placeholder="you@example.com"
                  value={email}
                  onChange={(e) => {
                    setEmail(e.target.value);
                    setCode("");
                  }}
                  required
                  disabled={busy || sending}
                />
              </label>
              {mode !== "forgot" && (
                <label className="block space-y-2 text-body">
                  <span>密码</span>
                  <Input
                    aria-label="密码"
                    type="password"
                    autoComplete={
                      mode === "register" ? "new-password" : "current-password"
                    }
                    minLength={mode === "register" ? 8 : undefined}
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                    required
                    disabled={busy}
                  />
                </label>
              )}
              {mode === "register" && (
                <>
                  <label className="block space-y-2 text-body">
                    <span>确认密码</span>
                    <Input
                      aria-label="确认密码"
                      type="password"
                      autoComplete="new-password"
                      minLength={8}
                      value={confirm}
                      onChange={(e) => setConfirm(e.target.value)}
                      required
                      disabled={busy}
                    />
                  </label>
                  {settings?.emailVerifyEnabled && (
                    <label className="block space-y-2 text-body">
                      <span>邮箱验证码</span>
                      <div className="flex gap-2">
                        <Input
                          aria-label="邮箱验证码"
                          autoComplete="one-time-code"
                          value={code}
                          onChange={(e) => setCode(e.target.value)}
                          required
                          disabled={busy}
                        />
                        <Button
                          type="button"
                          variant="outline"
                          onClick={() => void sendCode()}
                          disabled={
                            busy || sending || cooldown > 0 || !email.trim()
                          }
                        >
                          {cooldown > 0
                            ? `${cooldown} 秒`
                            : sending
                              ? "发送中…"
                              : "发送验证码"}
                        </Button>
                      </div>
                    </label>
                  )}
                  {settings?.invitationCodeEnabled && (
                    <Input
                      aria-label="邀请码"
                      placeholder="邀请码"
                      required
                      value={invitation}
                      onChange={(e) => setInvitation(e.target.value)}
                      disabled={busy}
                    />
                  )}
                </>
              )}
              {mode === "login" && (
                <div className="flex justify-end">
                  <button
                    type="button"
                    disabled={busy || !settings?.passwordResetEnabled}
                    className="text-caption text-fg-2 hover:text-action disabled:opacity-40"
                    onClick={() => changeMode("forgot")}
                  >
                    忘记密码？
                  </button>
                </div>
              )}
              <Button
                className="w-full"
                disabled={
                  busy ||
                  sending ||
                  !settings ||
                  (mode === "register" && !settings.registrationEnabled) ||
                  (mode === "forgot" && !settings.passwordResetEnabled)
                }
              >
                {busy ? (
                  <Loader2 className="me-2 h-4 w-4 animate-spin" />
                ) : null}
                {mode === "forgot"
                  ? "发送重置邮件"
                  : mode === "register"
                    ? "注册并登录"
                    : "登录"}
                {!busy && <ArrowRight className="ms-2 h-4 w-4" />}
              </Button>
            </form>
          )}
          {!settings && (
            <Button
              variant="ghost"
              className="mt-4 w-full"
              disabled={settingsLoading}
              onClick={() => setSettingsAttempt((value) => value + 1)}
            >
              {settingsLoading ? "正在连接平台…" : "重新连接平台"}
            </Button>
          )}
          {(error || session.error) && (
            <p role="alert" className="mt-4 text-body text-destructive">
              {error || session.error}
            </p>
          )}
          {notice && (
            <p role="status" className="mt-4 flex gap-2 text-body text-fg-2">
              <Check className="mt-0.5 h-4 w-4 shrink-0" />
              {notice}
            </p>
          )}
          {mode === "login" && (
            <p className="mt-7 text-center text-body text-fg-3">
              还没有账户？{" "}
              <button
                type="button"
                className="font-medium text-action disabled:opacity-40"
                onClick={() => changeMode("register")}
                disabled={busy || !settings?.registrationEnabled}
              >
                立即注册
              </button>
            </p>
          )}
        </div>
      </main>
    </div>
  );
}
