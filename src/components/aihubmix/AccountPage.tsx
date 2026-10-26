import { useEffect, useRef, useState } from "react";
import { Globe } from "lucide-react";
import { AppPageHeader } from "@/components/shell/AppPageHeader";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  dispatch,
  type AccountState,
  type KeyInfo,
  type GroupInfo,
  type LegacyMapping,
  type WalletBalance,
} from "@/lib/api/aihubmix";
import { ProviderDialog } from "./ProviderDialog";
import { RechargeDialog } from "./RechargeDialog";
export function AccountPage() {
  const [state, setState] = useState<AccountState | null>(null),
    [keys, setKeys] = useState<KeyInfo[]>([]),
    [groups, setGroups] = useState<GroupInfo[]>([]),
    [wallet, setWallet] = useState<WalletBalance | null>(null);
  const [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [email, setEmail] = useState(""),
    [password, setPassword] = useState(""),
    [invitationCode, setInvitation] = useState(""),
    [register, setRegister] = useState(false),
    [base, setBase] = useState(""),
    [apiKey, setApiKey] = useState(""),
    [name, setName] = useState(""),
    [groupId, setGroup] = useState("");
  const [provider, setProvider] = useState<KeyInfo | null>(null),
    [recharge, setRecharge] = useState(false);
  const [legacyMapping, setLegacyMapping] = useState<
    LegacyMapping | undefined
  >();
  const revision = useRef(0),
    gate = useRef(false);
  async function loadDetails(next: AccountState, rev: number) {
    const results = await Promise.allSettled([
      next.account.loggedIn ? dispatch<KeyInfo[]>("keys") : Promise.resolve([]),
      next.account.loggedIn
        ? dispatch<GroupInfo[]>("groups")
        : Promise.resolve([]),
      next.account.loggedIn
        ? dispatch<WalletBalance>("payment-balance")
        : Promise.resolve(null),
    ]);
    if (rev !== revision.current) return;
    if (results[0].status === "fulfilled") setKeys(results[0].value);
    if (results[1].status === "fulfilled") {
      setGroups(results[1].value);
      setGroup(String(results[1].value[0]?.id ?? ""));
    }
    if (results[2].status === "fulfilled") setWallet(results[2].value);
    const failed = results.find((r) => r.status === "rejected");
    if (failed?.status === "rejected") setError(String(failed.reason));
  }
  async function run(command = "state", payload?: unknown) {
    if (gate.current) return;
    gate.current = true;
    setBusy(true);
    setError("");
    const rev = ++revision.current;
    setKeys([]);
    setGroups([]);
    setWallet(null);
    setProvider(null);
    setLegacyMapping(undefined);
    setRecharge(false);
    try {
      const next = await dispatch<AccountState>(command, payload);
      if (rev !== revision.current) return;
      setState(next);
      setBase(next.relayBaseUrl);
      setPassword("");
      await loadDetails(next, rev);
    } catch (e) {
      if (rev === revision.current) setError(String(e));
    } finally {
      if (rev === revision.current) {
        gate.current = false;
        setBusy(false);
      }
    }
  }
  useEffect(() => {
    void run();
    return () => {
      revision.current++;
      gate.current = false;
    };
  }, []);
  async function keyAction(command: string, payload: unknown) {
    if (gate.current) return;
    gate.current = true;
    setBusy(true);
    setError("");
    try {
      const result = await dispatch<{ warning?: string }>(command, payload);
      gate.current = false;
      await run();
      setApiKey("");
      if (result?.warning) setError(result.warning);
    } catch (e) {
      setError(String(e));
    } finally {
      gate.current = false;
      setBusy(false);
    }
  }
  const allKeys = [
    ...keys,
    ...(state?.localKeys ?? []).filter(
      (k) => !keys.some((remote) => remote.id === k.id),
    ),
  ];
  return (
    <>
      <AppPageHeader
        title="AIHubMix 账户"
        icon={<Globe className="h-5 w-5" />}
        actions={
          <Button variant="outline" disabled={busy} onClick={() => void run()}>
            刷新
          </Button>
        }
      />
      <main
        id="main-content"
        className="min-h-0 flex-1 overflow-y-auto scroll-stable px-6 py-4"
      >
        <div className="max-w-4xl space-y-6 text-body">
          {error && (
            <p role="alert" className="text-destructive">
              {error}
            </p>
          )}
          {state?.storageError && (
            <p role="alert" className="text-destructive">
              安全存储：{state.storageError}
            </p>
          )}
          <section className="space-y-3 rounded-panel border border-border p-4">
            <h2 className="font-semibold">服务地址</h2>
            <form
              className="flex gap-2"
              onSubmit={(e) => {
                e.preventDefault();
                void run("set-base", { baseUrl: base });
              }}
            >
              <Input
                aria-label="服务地址"
                value={base}
                onChange={(e) => setBase(e.target.value)}
                required
                type="url"
              />
              <Button disabled={busy}>保存地址</Button>
            </form>
          </section>
          <section className="space-y-3 rounded-panel border border-border p-4">
            <h2 className="font-semibold">平台账户</h2>
            {state?.account.loggedIn ? (
              <>
                <p>{state.account.email}</p>
                <p>
                  钱包余额：
                  {wallet?.balance == null
                    ? "—"
                    : `${wallet.currency} ${wallet.balance.toFixed(2)}`}
                </p>
                <div className="flex gap-2">
                  <Button onClick={() => setRecharge(true)} disabled={busy}>
                    充值与订单
                  </Button>
                  <Button
                    variant="outline"
                    disabled={busy}
                    onClick={() => void run("logout")}
                  >
                    退出登录
                  </Button>
                </div>
              </>
            ) : (
              <form
                className="space-y-3"
                onSubmit={(e) => {
                  e.preventDefault();
                  void run(register ? "register" : "login", {
                    email,
                    password,
                    ...(register ? { invitationCode } : {}),
                  });
                }}
              >
                <Input
                  aria-label="邮箱"
                  type="email"
                  placeholder="邮箱"
                  autoComplete="username"
                  required
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                />
                <Input
                  aria-label="密码"
                  type="password"
                  placeholder="密码"
                  autoComplete={register ? "new-password" : "current-password"}
                  required
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                />
                {register && (
                  <Input
                    aria-label="邀请码"
                    placeholder="邀请码（选填）"
                    value={invitationCode}
                    onChange={(e) => setInvitation(e.target.value)}
                  />
                )}
                <div className="flex gap-2">
                  <Button disabled={busy || !state}>
                    {register ? "注册" : "登录"}
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    onClick={() => setRegister(!register)}
                  >
                    {register ? "已有账户，登录" : "创建账户"}
                  </Button>
                </div>
              </form>
            )}
          </section>
          <section className="space-y-3 rounded-panel border border-border p-4">
            <h2 className="font-semibold">平台 Key 与本机 Key</h2>
            <form
              className="flex flex-wrap gap-2"
              onSubmit={(e) => {
                e.preventDefault();
                void keyAction("save-key", { apiKey, name });
              }}
            >
              <Input
                className="flex-1"
                aria-label="本机 Key"
                type="password"
                placeholder="添加本机 API Key"
                value={apiKey}
                required
                onChange={(e) => setApiKey(e.target.value)}
              />
              <Input
                className="flex-1"
                aria-label="Key 名称"
                placeholder="名称（选填）"
                value={name}
                onChange={(e) => setName(e.target.value)}
              />
              <Button disabled={busy}>添加本机 Key</Button>
            </form>
            {state?.account.loggedIn && (
              <form
                className="flex gap-2"
                onSubmit={(e) => {
                  e.preventDefault();
                  void keyAction("create-key", {
                    groupId: Number(groupId),
                    name,
                  });
                }}
              >
                <select
                  aria-label="Key 分组"
                  className="flex-1 rounded-control border border-border bg-surface p-2"
                  value={groupId}
                  onChange={(e) => setGroup(e.target.value)}
                >
                  {groups.map((g) => (
                    <option key={g.id} value={g.id}>
                      {g.name} · {g.platform}
                    </option>
                  ))}
                </select>
                <Button disabled={busy || !groupId}>创建平台 Key</Button>
              </form>
            )}
            {!allKeys.length && (
              <p className="text-fg-3">{busy ? "正在加载…" : "暂无 Key"}</p>
            )}
            {allKeys.map((k) => (
              <div
                key={`${k.local}-${k.id}`}
                className="flex flex-wrap items-center gap-3 border-t border-border py-3"
              >
                <div className="min-w-0 flex-1">
                  <p>
                    {k.name}{" "}
                    <span className="text-fg-3">
                      {k.local ? "本机" : k.groupName} · {k.status}
                    </span>
                  </p>
                  <code>{k.keyMasked}</code>
                  <p className="text-fg-3">
                    Key 配额：{k.quotaUsed ?? "—"} / {k.quota ?? "—"}
                  </p>
                </div>
                <Button
                  variant="outline"
                  disabled={busy}
                  onClick={() => {
                    void dispatch<string>("get-key", { id: k.id })
                      .then((key) => navigator.clipboard.writeText(key))
                      .catch((e) => setError(String(e)));
                  }}
                >
                  复制
                </Button>
                <Button disabled={busy} onClick={() => setProvider(k)}>
                  添加供应商
                </Button>
                <Button
                  variant="ghost"
                  disabled={busy}
                  onClick={() => {
                    if (
                      window.confirm(
                        `确认${k.local ? "移除本机" : "删除平台"} Key「${k.name}」？`,
                      )
                    )
                      void keyAction(
                        k.local ? "remove-local-key" : "delete-key",
                        { id: k.id },
                      );
                  }}
                >
                  {k.local ? "移除" : "删除"}
                </Button>
              </div>
            ))}
          </section>
          <section className="space-y-3 rounded-panel border border-border p-4">
            <h2 className="font-semibold">旧版数据</h2>
            <p className="text-fg-3">
              导入旧服务地址、可用登录和本机 Key。旧文件与备份保留，工具配置由
              CC 显式应用。
            </p>
            {!!state?.legacyMappings?.length && (
              <div className="space-y-2">
                <h3 className="font-medium">已导入的工具与模型映射</h3>
                {state.legacyMappings.map((mapping) => (
                  <div
                    key={mapping.app}
                    className="flex flex-wrap items-center justify-between gap-2 rounded-control border border-border p-2"
                  >
                    <span>
                      {mapping.appLabel} · <code>{mapping.model}</code>
                    </span>
                    <Button
                      variant="outline"
                      disabled={busy}
                      onClick={() => setLegacyMapping(mapping)}
                    >
                      预填 {mapping.appLabel} 供应商
                    </Button>
                  </div>
                ))}
                {legacyMapping && (
                  <p role="status">
                    已选择 {legacyMapping.appLabel} · {legacyMapping.model}
                    。请在上方选择 Key 并点击“添加供应商”，审核后保存。
                  </p>
                )}
              </div>
            )}
            <div className="flex gap-2">
              {state?.legacyData && (
                <Button
                  disabled={busy}
                  onClick={() => {
                    if (window.confirm("确认导入旧版账户与本机 Key 数据？"))
                      void run("legacy-import");
                  }}
                >
                  导入旧版数据
                </Button>
              )}
              <Button
                variant="outline"
                onClick={() =>
                  void dispatch("open-legacy-backups").catch((e) =>
                    setError(String(e)),
                  )
                }
              >
                打开旧版备份目录
              </Button>
            </div>
          </section>
        </div>
      </main>
      {provider && (
        <ProviderDialog
          item={provider}
          initialMapping={legacyMapping}
          onClose={() => setProvider(null)}
        />
      )}
      {state?.account.loggedIn && (
        <RechargeDialog
          open={recharge}
          key={`${state.relayBaseUrl}:${state.account.userId ?? state.account.email}`}
          onClose={() => setRecharge(false)}
          onPaid={() => {
            if (state) void loadDetails(state, revision.current);
          }}
        />
      )}
    </>
  );
}
