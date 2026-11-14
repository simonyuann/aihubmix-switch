import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import { SettingsRow } from "@/components/settings/SettingsLayout";
import { isMac, isWindows } from "@/lib/platform";

interface ProgramStatus {
  installed: boolean;
  executablePath?: string;
}

export function DesktopProgramLocation({
  name,
  executablePath,
  inspect,
  onChange,
  canChoose = true,
}: {
  name: string;
  executablePath: string;
  inspect: (path: string) => Promise<ProgramStatus>;
  onChange: (path: string) => void;
  canChoose?: boolean;
}) {
  const [status, setStatus] = useState<ProgramStatus | null>(null);
  const [checking, setChecking] = useState(true);
  const [failed, setFailed] = useState(false);
  const [selecting, setSelecting] = useState(false);
  const [error, setError] = useState("");
  const [revision, setRevision] = useState(0);

  useEffect(() => {
    let current = true;
    setChecking(true);
    setStatus(null);
    setFailed(false);
    void Promise.resolve()
      .then(() => inspect(executablePath))
      .then((result) => {
        if (current) setStatus(result);
      })
      .catch(() => {
        if (current) setFailed(true);
      })
      .finally(() => {
        if (current) setChecking(false);
      });
    return () => {
      current = false;
    };
  }, [executablePath, inspect, revision]);

  async function choose() {
    setSelecting(true);
    setError("");
    try {
      const path = await open({
        multiple: false,
        directory: false,
        ...(isWindows()
          ? { filters: [{ name: `${name} 桌面程序`, extensions: ["exe"] }] }
          : isMac()
            ? { filters: [{ name: `${name} 应用`, extensions: ["app"] }] }
            : {}),
      });
      if (typeof path !== "string") return;
      const result = await inspect(path);
      if (!result.installed) {
        setError(`无法识别所选 ${name} 程序，请选择完整安装包中的官方应用。`);
        return;
      }
      onChange(path);
      setRevision((value) => value + 1);
    } catch {
      setError(`无法检查 ${name} 程序，请重新选择或重试。`);
    } finally {
      setSelecting(false);
    }
  }

  let label = "未找到，请手动选择";
  if (!canChoose) label = "未找到，请安装后重新检测";
  if (status?.installed)
    label = executablePath.trim() ? "手动指定" : "自动识别";
  if (failed) label = "检测失败，请重新检测或选择程序";
  if (checking) label = "正在查找程序…";

  return (
    <SettingsRow
      label={`${name} 程序位置`}
      description={
        <div aria-live="polite">
          <span
            className={
              !checking && !status?.installed ? "text-danger-text" : ""
            }
          >
            {label}
          </span>
          {status?.installed && status.executablePath && (
            <div className="mt-1 select-text break-all text-fg-2">
              {status.executablePath}
            </div>
          )}
          {!checking && !status?.installed && executablePath && (
            <div className="mt-1 select-text break-all">
              原指定路径 {executablePath}
            </div>
          )}
        </div>
      }
      control={
        <>
          {canChoose && (
            <Button
              variant="outline"
              size="compact"
              disabled={selecting}
              onClick={() => void choose()}
            >
              {selecting ? "正在检查…" : `选择 ${name} 程序`}
            </Button>
          )}
          {executablePath ? (
            <Button
              variant="quiet"
              size="compact"
              disabled={selecting}
              onClick={() => {
                setError("");
                onChange("");
              }}
            >
              恢复自动查找
            </Button>
          ) : (
            <Button
              variant="quiet"
              size="compact"
              disabled={checking || selecting}
              onClick={() => setRevision((value) => value + 1)}
            >
              重新检测
            </Button>
          )}
        </>
      }
    >
      {error && (
        <p role="alert" className="text-caption text-danger-text">
          {error}
        </p>
      )}
    </SettingsRow>
  );
}
