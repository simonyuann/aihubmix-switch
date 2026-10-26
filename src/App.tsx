import { lazy, Suspense, useEffect, useMemo, useRef, useState } from "react";
import { Loader2 } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { AccountProvider, useAccount } from "@/contexts/AccountContext";
import { LoginPage } from "@/components/aihubmix/LoginPage";
import { AccountPage } from "@/components/aihubmix/AccountPage";
import { ToolPage } from "@/components/aihubmix/ToolPage";
import { Sidebar } from "@/components/shell/Sidebar";
import { WindowControlsContext } from "@/components/shell/AppPageHeader";
import { WindowControls } from "@/components/shell/WindowControls";
import { useSettingsQuery } from "@/lib/query";
import { APP_IDS, DEFAULT_VISIBLE_APPS } from "@/config/appConfig";
import type { AppId } from "@/lib/api";
import {
  APP_STORAGE_KEY,
  readStoredView,
  storeView,
  type GlobalPage,
  type SettingsSection,
  type View,
} from "@/lib/navigation";
import { isLinux, isWindows } from "@/lib/platform";
import {
  checkToolUpdatesInBackground,
  useToolUpdatesAvailable,
} from "@/components/apps/useToolManagement";
import {
  syncModelsDevPricingOnStartup,
  MODELS_DEV_SYNC_CONFIG_QUERY_KEY,
} from "@/lib/modelsDevAutoSync";
import { useQueryClient } from "@tanstack/react-query";

const UsagePage = lazy(() =>
  import("@/components/usage/UsagePage").then((m) => ({
    default: m.UsagePage,
  })),
);
const SessionManagerPage = lazy(() =>
  import("@/components/sessions/SessionManagerPage").then((m) => ({
    default: m.SessionManagerPage,
  })),
);
const AppsPage = lazy(() =>
  import("@/components/apps/AppsPage").then((m) => ({ default: m.AppsPage })),
);
const SettingsPage = lazy(() =>
  import("@/components/settings/SettingsPage").then((m) => ({
    default: m.SettingsPage,
  })),
);
const PAGES = new Set<View>([
  "providers",
  "aihubmix",
  "sessions",
  "usage",
  "apps",
  "settings",
]);
function LoadingScreen() {
  return (
    <div
      role="status"
      className="flex h-screen items-center justify-center gap-3 bg-app text-body text-fg-3"
    >
      <Loader2 className="h-5 w-5 animate-spin" />
      正在验证登录…
    </div>
  );
}
export function AccountBoundary({ children }: { children: React.ReactNode }) {
  const { state, checking } = useAccount();
  if (checking) return <LoadingScreen />;
  return state?.account.loggedIn ? <>{children}</> : <LoginPage />;
}
function ClientShell() {
  const client = useQueryClient();
  useEffect(() => {
    void syncModelsDevPricingOnStartup()
      .then((result) => {
        if (!result.skipped) {
          void client.invalidateQueries({ queryKey: ["usage"] });
          void client.invalidateQueries({
            queryKey: MODELS_DEV_SYNC_CONFIG_QUERY_KEY,
          });
        }
      })
      .catch(() => {
        /* Pricing availability does not block the account. */
      });
  }, [client]);
  const [app, setApp] = useState<AppId>(() => {
    const saved = localStorage.getItem(APP_STORAGE_KEY);
    return APP_IDS.includes(saved as AppId) ? (saved as AppId) : "claude";
  });
  const [view, setView] = useState<View>(() => {
    const saved = readStoredView();
    return PAGES.has(saved) ? saved : "providers";
  });
  const [section, setSection] = useState<SettingsSection>("general");
  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    const navigate = async () => {
      try {
        const next = await invoke<{ app?: string; section?: string } | null>(
          "take_tray_navigation",
        );
        if (!active || !next) return;
        if (next.app && APP_IDS.includes(next.app as AppId)) {
          setApp(next.app as AppId);
          localStorage.setItem(APP_STORAGE_KEY, next.app);
          setView("providers");
        } else if (next.section) {
          setSection(next.section === "about" ? "about" : "general");
          setView("settings");
        }
      } catch {
        /* Native events are unavailable in renderer-only development. */
      }
    };
    void listen("tray-navigate", () => void navigate())
      .then((stop) => {
        if (active) {
          unlisten = stop;
          void navigate();
        } else stop();
      })
      .catch(() => {});
    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  const { data: settings } = useSettingsQuery();
  const toolsUpdateAvailable = useToolUpdatesAvailable();
  const toolCheckStarted = useRef(false);
  useEffect(() => {
    if (settings?.checkToolUpdatesOnStartup && !toolCheckStarted.current) {
      toolCheckStarted.current = true;
      void checkToolUpdatesInBackground();
    }
  }, [settings?.checkToolUpdatesOnStartup]);
  const visible = useMemo(
    () => ({ ...DEFAULT_VISIBLE_APPS, ...settings?.visibleApps }),
    [settings?.visibleApps],
  );
  const controls =
    isWindows() || (isLinux() && (settings?.useAppWindowControls ?? false));
  useEffect(() => {
    storeView(view);
  }, [view]);
  useEffect(() => {
    void getCurrentWindow()
      .setDecorations(!controls)
      .catch(() => {});
  }, [controls]);
  function selectApp(next: AppId) {
    setApp(next);
    setView("providers");
    localStorage.setItem(APP_STORAGE_KEY, next);
  }
  function selectPage(next: GlobalPage | "settings") {
    setView(PAGES.has(next) ? next : "providers");
  }
  const sharedApp = app === "claude-desktop" ? "claude" : app;
  return (
    <WindowControlsContext.Provider
      value={controls ? <WindowControls /> : null}
    >
      <div className="flex h-screen overflow-hidden bg-app text-fg-1 selection:bg-action/25">
        <Sidebar
          appsUpdateAvailable={
            !!settings?.checkToolUpdatesOnStartup && toolsUpdateAvailable
          }
          activeApp={app}
          view={view}
          visibleApps={visible}
          settingsSection={section}
          onSelectApp={selectApp}
          onSelectPage={selectPage}
          onSelectSettingsSection={(next) =>
            setSection(next === "about" ? "about" : "general")
          }
          onExitSettings={() => setView("providers")}
        />
        <div
          id="content-area"
          className="relative isolate flex min-w-0 flex-1 flex-col overflow-hidden bg-app"
        >
          <Suspense
            fallback={
              <div role="status" className="p-6 text-body text-fg-3">
                正在加载…
              </div>
            }
          >
            {view === "providers" && <ToolPage key={app} app={app} />}
            {view === "aihubmix" && <AccountPage />}
            {view === "usage" && <UsagePage />}
            {view === "sessions" && (
              <SessionManagerPage
                key={sharedApp}
                appId={sharedApp}
                fromApp={app}
                onOpenTerminalSettings={() => {
                  setSection("general");
                  setView("settings");
                }}
              />
            )}
            {view === "apps" && <AppsPage />}
            {view === "settings" && (
              <SettingsPage
                section={section}
                onOpenApps={() => setView("apps")}
                onOpenApp={selectApp}
              />
            )}
          </Suspense>
        </div>
      </div>
    </WindowControlsContext.Provider>
  );
}
export default function App() {
  return (
    <AccountProvider>
      <AccountBoundary>
        <ClientShell />
      </AccountBoundary>
    </AccountProvider>
  );
}
