import { useRef, useState } from "react";
import { LogOut } from "lucide-react";
import { useOptionalAccount } from "@/contexts/AccountContext";
import { Button } from "@/components/ui/button";
import { SettingsBlock, SettingsCard, SettingsRow } from "./SettingsLayout";
import { friendlyError } from "@/components/aihubmix/friendlyError";

export function AccountSettings() {
  const account = useOptionalAccount();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const submitting = useRef(false);
  if (!account?.state?.account.loggedIn) return null;

  async function logout() {
    if (!account || submitting.current) return;
    submitting.current = true;
    setBusy(true);
    setError("");
    try {
      await account.logout();
    } catch (e) {
      setError(friendlyError(e, "退出失败，请重试。"));
    } finally {
      submitting.current = false;
      setBusy(false);
    }
  }

  return (
    <SettingsBlock title="账号">
      <SettingsCard>
        <SettingsRow
          label="当前账号"
          description={`${account.state.account.email ?? ""}。退出或换号后，请关闭并重新打开 Codex 和 DeepSeek Harness；已运行的进程可能继续使用旧凭据。为保护自定义配置，DSH 凭据文件可能保留旧引用。`}
          control={
            <Button
              variant="outline"
              disabled={busy}
              onClick={() => void logout()}
            >
              <LogOut className="h-3.5 w-3.5" />
              {busy ? "正在退出…" : "退出登录"}
            </Button>
          }
        >
          {error && (
            <p role="alert" className="text-body text-destructive">
              {error}
            </p>
          )}
        </SettingsRow>
      </SettingsCard>
    </SettingsBlock>
  );
}
