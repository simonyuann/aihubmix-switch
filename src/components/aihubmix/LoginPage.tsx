import { useEffect, useRef, useState } from "react";
import { ArrowLeft, ArrowRight, Check, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useAccount } from "@/contexts/AccountContext";
import { dispatch, type LogoutResult } from "@/lib/api/aihubmix";
import {
  DRAG_REGION_ATTR,
  DRAG_REGION_STYLE,
  isLinux,
  isWindows,
} from "@/lib/platform";
import { WindowControls } from "@/components/shell/WindowControls";
import { useSettingsQuery } from "@/lib/query";
import logo from "@/assets/icons/aihubmix-a.png";
import { friendlyError } from "./friendlyError";
import { useTranslation } from "react-i18next";

type Mode = "login" | "register" | "forgot";
interface AuthSettings {
  registrationEnabled: boolean;
  emailVerifyEnabled: boolean;
  passwordResetEnabled: boolean;
  invitationCodeEnabled: boolean;
  captchaRequired: boolean;
  agreements?: { title: string; content: string }[];
}
export function LoginPage() {
  const { t } = useTranslation();
  const session = useAccount();
  const { data: windowSettings } = useSettingsQuery();
  const windowControls =
    isWindows() || (isLinux() && windowSettings?.useAppWindowControls);
  const [mode, setMode] = useState<Mode>("login");
  const [settings, setSettings] = useState<AuthSettings | null>(null);
  const [settingsAttempt, setSettingsAttempt] = useState(0);
  const [settingsLoading, setSettingsLoading] = useState(true);
  const settingsRequest = useRef<{
    attempt: number;
    request: Promise<AuthSettings>;
  } | null>(null);
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
  const [accepted, setAccepted] = useState(false);
  const agreements = settings?.agreements ?? [];
  const needsAgreement = mode === "register" && agreements.length > 0;
  const [remember, setRemember] = useState(false);
  const [savedEmail, setSavedEmail] = useState("");
  const [forgetting, setForgetting] = useState(false);
  const formEdited = useRef(false);
  const savedPasswordAvailable =
    mode === "login" && remember && !!savedEmail && savedEmail === email.trim();
  useEffect(() => {
    let current = true;
    void dispatch<string | null>("remembered-login")
      .then((saved) => {
        if (current && typeof saved === "string" && saved) {
          setSavedEmail(saved);
          if (!formEdited.current) {
            setEmail(saved);
            setRemember(true);
          }
        }
      })
      .catch(() => {});
    return () => {
      current = false;
    };
  }, []);
  async function changeRemember(checked: boolean) {
    formEdited.current = true;
    if (!checked && savedEmail) {
      setForgetting(true);
      try {
        await dispatch("forget-remembered-login");
        setSavedEmail("");
      } catch {
        setError(t("loginPage.error.clearSavedPassword"));
        return;
      } finally {
        setForgetting(false);
      }
    }
    setRemember(checked);
  }
  const gate = useRef(false);
  const active = useRef(true);
  useEffect(() => {
    active.current = true;
    let current = true;
    setSettingsLoading(true);
    setSettings(null);
    setAccepted(false);
    setError("");
    if (settingsRequest.current?.attempt !== settingsAttempt) {
      settingsRequest.current = {
        attempt: settingsAttempt,
        request: dispatch<AuthSettings>("auth-settings"),
      };
    }
    settingsRequest.current.request
      .then((value) => {
        if (current) setSettings(value);
      })
      .catch((e) => {
        if (current)
          setError(friendlyError(e, t("loginPage.error.connectFailed")));
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
    formEdited.current = true;
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
        setNotice(t("loginPage.notice.codeSent"));
        setCooldown(60);
      }
    } catch (e) {
      if (active.current)
        setError(friendlyError(e, t("loginPage.error.codeSendFailed")));
    } finally {
      gate.current = false;
      if (active.current) setSending(false);
    }
  }
  async function submit() {
    if (
      gate.current ||
      forgetting ||
      !settings ||
      settings.captchaRequired ||
      (mode === "register" && !settings.registrationEnabled) ||
      (mode === "forgot" && !settings.passwordResetEnabled) ||
      (needsAgreement && !accepted)
    )
      return;
    setError("");
    setNotice("");
    if (mode === "register" && password !== confirm) {
      setError(t("loginPage.error.passwordMismatch"));
      return;
    }
    gate.current = true;
    setBusy(true);
    try {
      if (mode === "forgot") {
        await dispatch("forgot-password", { email: email.trim() });
        if (active.current) setNotice(t("loginPage.notice.resetEmailSent"));
      } else {
        await session.authenticate(mode, {
          email: email.trim(),
          password,
          ...(mode === "login" && remember
            ? {
                remember: true,
                useSavedPassword: savedPasswordAvailable && !password,
              }
            : {}),
          emailCode: code.trim(),
          invitationCode: invitation.trim(),
        });
      }
    } catch (e) {
      if (active.current) {
        const message = friendlyError(e, t("loginPage.error.operationFailed"));
        if (message === t("loginPage.error.reenterPassword")) setSavedEmail("");
        setError(message);
      }
    } finally {
      gate.current = false;
      if (active.current) setBusy(false);
    }
  }
  async function clearPreviousAccount() {
    if (gate.current) return;
    gate.current = true;
    setBusy(true);
    try {
      const result = await dispatch<LogoutResult>("logout");
      setError("");
      setNotice(
        result?.logoutNotice ||
          result?.revocationWarning ||
          t("loginPage.notice.loggedOutLocal"),
      );
    } catch (e) {
      setError(friendlyError(e, t("loginPage.error.logoutFailed")));
    } finally {
      gate.current = false;
      setBusy(false);
    }
  }
  const title = t(
    mode === "login"
      ? "loginPage.title.login"
      : mode === "register"
        ? "loginPage.title.register"
        : "loginPage.title.forgot",
  );
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
      <main className="flex min-h-0 flex-1 flex-col items-center overflow-y-auto px-6 py-10">
        <div className="aihub-page-enter my-auto w-full max-w-[400px] shrink-0">
          <div className="mb-10 flex items-center justify-center gap-3">
            <img src={logo} alt="" className="h-10 w-10 rounded-xl" />
            <span className="text-xl font-semibold tracking-tight">
              AIHub Switch
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
              {t("loginPage.action.backToLogin")}
            </Button>
          )}
          <h2 className="text-page font-semibold">{title}</h2>
          <p className="mb-7 mt-2 text-body text-fg-3">
            {mode === "login"
              ? t("loginPage.subtitle.login")
              : mode === "register"
                ? t("loginPage.subtitle.register")
                : t("loginPage.subtitle.forgot")}
          </p>
          {settings?.captchaRequired ? (
            <div className="space-y-4">
              <p className="text-body text-fg-2">
                {t("loginPage.error.captchaUnsupported")}
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
                <span>{t("loginPage.label.email")}</span>
                <Input
                  aria-label={t("loginPage.label.email")}
                  type="email"
                  autoComplete="username"
                  placeholder="you@example.com"
                  value={email}
                  onChange={(e) => {
                    formEdited.current = true;
                    setEmail(e.target.value);
                    setCode("");
                  }}
                  required
                  disabled={busy || sending}
                />
              </label>
              {mode !== "forgot" && (
                <label className="block space-y-2 text-body">
                  <span>{t("loginPage.label.password")}</span>
                  <Input
                    aria-label={t("loginPage.label.password")}
                    type="password"
                    autoComplete={
                      mode === "register" ? "new-password" : "current-password"
                    }
                    minLength={mode === "register" ? 8 : undefined}
                    value={password}
                    onChange={(e) => {
                      formEdited.current = true;
                      setPassword(e.target.value);
                    }}
                    placeholder={
                      savedPasswordAvailable
                        ? t("loginPage.placeholder.savedPassword")
                        : undefined
                    }
                    required={!savedPasswordAvailable}
                    disabled={busy}
                  />
                </label>
              )}
              {mode === "register" && (
                <>
                  <label className="block space-y-2 text-body">
                    <span>{t("loginPage.label.confirmPassword")}</span>
                    <Input
                      aria-label={t("loginPage.label.confirmPassword")}
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
                      <span>{t("loginPage.label.emailCode")}</span>
                      <div className="flex gap-2">
                        <Input
                          aria-label={t("loginPage.label.emailCode")}
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
                            ? t("loginPage.register.codeCooldown", {
                                seconds: cooldown,
                              })
                            : sending
                              ? t("loginPage.register.sending")
                              : t("loginPage.register.sendCode")}
                        </Button>
                      </div>
                    </label>
                  )}
                  {settings?.invitationCodeEnabled && (
                    <Input
                      aria-label={t("loginPage.label.invitationCode")}
                      placeholder={t("loginPage.label.invitationCode")}
                      required
                      value={invitation}
                      onChange={(e) => setInvitation(e.target.value)}
                      disabled={busy}
                    />
                  )}
                </>
              )}
              {mode === "login" && (
                <div className="flex items-center justify-between">
                  <label className="flex items-center gap-2 text-caption text-fg-2">
                    <input
                      type="checkbox"
                      checked={remember}
                      disabled={busy || forgetting}
                      onChange={(e) => void changeRemember(e.target.checked)}
                    />
                    {t("loginPage.label.rememberMe")}
                  </label>
                  <button
                    type="button"
                    disabled={busy || !settings?.passwordResetEnabled}
                    className="text-caption text-fg-2 hover:text-action disabled:opacity-40"
                    onClick={() => changeMode("forgot")}
                  >
                    {t("loginPage.action.forgotPassword")}
                  </button>
                </div>
              )}
              {needsAgreement && (
                <section className="space-y-2 text-caption">
                  {agreements.map((doc, index) => (
                    <details key={index}>
                      <summary className="cursor-pointer text-action">
                        {doc.title}
                      </summary>
                      <div
                        tabIndex={0}
                        className="mt-2 max-h-40 overflow-y-auto whitespace-pre-wrap break-words text-fg-2"
                      >
                        {doc.content}
                      </div>
                    </details>
                  ))}
                  <label className="flex items-center gap-2">
                    <input
                      type="checkbox"
                      checked={accepted}
                      onChange={(e) => setAccepted(e.target.checked)}
                      required
                    />
                    {t("loginPage.label.agreeToTerms")}
                  </label>
                </section>
              )}
              <Button
                className="w-full"
                disabled={
                  busy ||
                  sending ||
                  forgetting ||
                  !settings ||
                  (needsAgreement && !accepted) ||
                  (mode === "register" && !settings.registrationEnabled) ||
                  (mode === "forgot" && !settings.passwordResetEnabled)
                }
              >
                {busy ? (
                  <Loader2 className="me-2 h-4 w-4 animate-spin" />
                ) : null}
                {mode === "forgot"
                  ? t("loginPage.action.sendResetEmail")
                  : mode === "register"
                    ? t("loginPage.action.registerAndLogin")
                    : t("loginPage.action.login")}
                {!busy && <ArrowRight className="ms-2 h-4 w-4" />}
              </Button>
            </form>
          )}
          {!settings && !settingsLoading && (
            <Button
              variant="ghost"
              className="mt-4 w-full"
              disabled={settingsLoading || busy || sending}
              onClick={() => setSettingsAttempt((value) => value + 1)}
            >
              {t("loginPage.action.reconnectPlatform")}
            </Button>
          )}
          {(error || session.error) && (
            <p role="alert" className="mt-4 text-body text-destructive">
              {error || session.error}
            </p>
          )}
          {error.includes("请先退出当前账号") && (
            <Button
              type="button"
              variant="outline"
              className="mt-3 w-full"
              disabled={busy}
              onClick={() => void clearPreviousAccount()}
            >
              {t("loginPage.action.logoutOldAccount")}
            </Button>
          )}
          {notice && (
            <p role="status" className="mt-4 flex gap-2 text-body text-fg-2">
              <Check className="mt-0.5 h-4 w-4 shrink-0" />
              {notice}
            </p>
          )}
          {mode === "login" && (
            <p className="mt-7 text-center text-body text-fg-3">
              {t("loginPage.label.noAccountYet")}{" "}
              <button
                type="button"
                className="font-medium text-action disabled:opacity-40"
                onClick={() => changeMode("register")}
                disabled={busy || !settings?.registrationEnabled}
              >
                {t("loginPage.action.registerNow")}
              </button>
            </p>
          )}
        </div>
      </main>
    </div>
  );
}
