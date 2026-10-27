import { useRef, useState } from "react";
import { LogOut } from "lucide-react";
import { useOptionalAccount } from "@/contexts/AccountContext";
import { Button } from "@/components/ui/button";
import { SettingsBlock, SettingsCard, SettingsRow } from "./SettingsLayout";

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
      setError(String(e));
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
          description={account.state.account.email}
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
