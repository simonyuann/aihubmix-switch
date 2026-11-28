# 治理进度日志(Progress Log)

> 每轮治理一轮记录:范围 / 关键动作 / 验证结果 / 提交。

## Round 0 — 基础设施与基线(2026-11-28)

**范围**:治理环境搭建、验证基线、架构地图初版、任务池初版。

**动作**:
1. 创建隔离 worktree(`governance-wt`,分支 `Z_code_work`,基于 main `5c306a9`)。
2. 工具链:node v26.12.1 + pnpm 10.12.3(本机缺 pnpm,已装);Rust:rustup + 1.95.0 + rustfmt(clippy 按 rust-toolchain.toml)。本机无 MSVC Build Tools → Rust 链接能力待 `cargo check` 实证(后台进行中)。
3. 基线:
   - `pnpm install` ✅(26s)
   - `pnpm typecheck` ✅ 绿
   - `pnpm test:unit` ⏳ 后台运行中
   - `cargo check` ⏳ 后台运行中
4. 三个并行勘察代理(Rust 后端/前端/工具链)发出;Rust 代理因并发限额失败 → Rust 侧改为本会话直接勘察,已完成:模块行数地图、config 家族平行形状、misc.rs 构成、378 命令注册面、跨层引用计数。
5. 治理文档:`README.md`(框架+验证矩阵)、`TASKS.md`(T1-T10 任务池)、`DECISIONS.md`(ADR-001~004 + 待确认 P-ADR-001~003)、`ARCHITECTURE-MAP.md`(初版,E1-E10 熵点总表)。

**验证**:见上;两个后台基线结果落地后更新本节。

**结论**:治理循环可以开始;第一批可执行任务为 T1/T2/T8/T10(可验证、低风险)。

### 基线补充(第一轮实测)

- `pnpm test:unit`(main 状态,两次完整运行):**失败集中在同样 5 个文件**(`AppsPage`×4、`PiProviderForm`×10、`SwitchModePanel`×2、`ClaudeDesktopProviderForm`×1、`IconPicker`×1),但失败测试数在两次运行间从 13 漂移到 18(总测试 3554)——**负载敏感的 flaky 测试**(单测耗时 5-8s 触发超时,两次运行分别与 cargo 工具链安装并发)。基线判定:这 5 个文件在 Windows 本地环境不稳定;重构批次的行为基线以"失败文件集合"为对照物(不得扩大),精确失败数需静默复跑归因(见 T17)。
- `cargo check`(MSVC 路径)❌:rustc 找到的 `link.exe` 是 Git Bash 的 coreutils link(`link: extra operand ...`),本机无 MSVC Build Tools。**处置**:改用 GNU 目标工具链 `RUSTUP_TOOLCHAIN=1.95.0-x86_64-pc-windows-gnu` —— **✅ `cargo check` 通过(7m49s),Rust 验证能力解锁**;`cargo test` 首跑进行中。
- 工具链勘察报告落地,重大发现:仓库为 `farion1231/cc-switch` 的 fork(Cargo 包名仍是 cc-switch);三份死配置(已删,见 Round 1);版本 SSOT 已有 CI 闸门(T2 关闭);新增 T2b/T11-T15 任务。

## Round 1 — T1 死配置归一(2026-11-28)

**范围**:TASKS T1。

**动作**:删除 `vite.config.mts` / `vitest.config.mts` / `tsconfig.web.json`(Tauri 模板残留,零引用,`.ts` 版本按解析顺序遮蔽);清除 `pnpm-workspace.yaml` 非法 `allowBuilds` 占位块。

**验证**:`pnpm typecheck` ✅;`git show --stat` 确认 4 文件纯删除 59 行,无意外文件入提交。

**提交**:`4cd65a4 build: 删除 Tauri 模板残留的死配置(vite/vitest .mts 与 tsconfig.web.json)及 pnpm-workspace 占位符`

**防回归**:死配置删除后不可再生——由 `pnpm typecheck` 的 include 范围与 vite/vitest 解析顺序天然保证;不另设 lint(过度)。

## Round 2 — T7 死代码删除批次1 + T20 format.ts 下沉(2026-11-28)

**T7 批次1**(`650c3ea`):删除 8 个零引用文件(useDebouncedValue/tomlUtils/toggle-row/ui/card/AppCountBar/AppToggleGroup/ListItemRow/ManagementListSearch)+ 3 个配套测试,-864 行。修正勘察漏项:tomlUtils 并非全零引用(其测试引用),连带修剪测试。验证:typecheck 绿;textNormalization 测试通过;动态引用(import.meta.glob/模板 import)排查无残留。

**T20**(`fb004c5`):`components/usage/format.ts`(474 行通用显示格式化)被 5 个非 usage 区域消费,层级倒挂 → 迁移 `utils/format.ts`,23 处导入点全量更新,旧路径删除。验证:typecheck 绿;324 相关测试通过。

**待办**:全量 vitest 静默对照(cargo test 完成后跑,对照 5 文件 flaky 基线)。

## Round 3 — invoke 边界归一 + 依赖方向检查器 + build.rs 平台门控(2026-11-28)

**范围**:T3(部分)/ T8 / T9 + Rust 验证基建。

**动作与提交**:
1. `c058dbc`:组件/hooks/入口的 **9 处散落 invoke** 收敛到 lib/api;新增 window/init/tray/codexImagegen 四个单一职责 api 模块;updater.ts 补齐 2 个命令契约;**dependency-cruiser 边界规则 `no-raw-invoke-outside-api-layer` 落地(error 级)+ `pnpm lint:arch` 脚本**。当前 631 模块 3464 依赖 0 违规。
2. `build.rs` 修复:Common Controls manifest 的 `/MANIFEST*` 链接参数按 `CARGO_CFG_TARGET_ENV == "msvc"` 门控(MSVC/CI 行为不变;GNU 目标跳过,使无 MSVC 的 Windows 开发者可编译)。
3. 判定记录:T18(mock 双轨)——12 个文件的 inline `vi.mock` 各有语义(isTauri 常量/固定返回/代理 spy),非复制粘贴,**不机械合并**;T3 余项(lib 根桌面集成四件套)——claudeDesktop.ts 内含 React hooks 状态机,整体搬入 lib/api 会污染 api 层纯度,后续以 lint 边界(已完成)+ 拆分纯 invoke 契约的方式处理,不做无价值目录搬迁。

**验证**:tsc 绿;prettier 绿;相关测试 74+90 通过;depcruise 0 违规;cargo check(GNU)在 build.rs 修改后通过。

**发现**:`cargo test` 在 GNU 目标硬性受阻(`webview2_com_sys` E0463,COM 栈 MSVC 导入库导向),clean 后复现 → 行为级 Rust 验证以 CI 为准;本地用 `cargo check --all-targets` 做类型级验证。安装 MSVC Build Tools 可解锁(待用户决定)。

## Round 4 — 行为基线抓回归 + T17 静默基线 + R1 预研(2026-11-28)

**T17 数据**:静默全量 vitest = **7 失败/3533 通过**(负载下 13↔18)——证实漂移是负载型 flaky;真实稳定失败在 `PiProviderForm`(6)与此前集合对比,`SessionMarkdown`×1 为**治理引入的回归,已修复**(见下)。

**回归案例(治理循环价值的实证)**:`c058dbc` 把 SessionMarkdown 的 openSessionLink 迁到 settingsApi.openExternal 后,静默基线立刻暴露 1 个新失败——`settingsApi.openExternal` 的前端校验只放行 http/https,而后端 `validate_external_url`(SSOT)放行 http/https/**mailto**。修复:前端白名单对齐后端契约(`fix` 提交),SessionMarkdown/AppsPage/help 共 105 测试复绿。**教训:api 方法迁移=契约审查,不只是路径搬迁。**

**T18 判定**:维持"不机械合并"(各 mock 有独立语义)。

**R1 预研**(进行中):misc.rs 进程原语抽离为 `child_process.rs`;验证手段 = cargo check --all-targets(GNU)+ linux target 交叉检查 `cfg(not(windows))` 代码。

## Round 5 — R1 落地:child_process 深模块(2026-11-28)

**提交**:`89ddd92`(misc.rs -290 行)。

**内容**:进程执行原语(CommandDeadline/terminate_child_tree×2/isolate_child_process_group/wait_child_output/apply_extra_env/decode_command_output+Windows 多代码页解码/merge_path_segments+win 变体/CREATE_NO_WINDOW)从 commands/misc.rs 抽为 `src-tauri/src/child_process.rs`;两个原测试逐字随迁;services 的 **6 处**反向引用(mcp_connectivity×2、claude_account×2、codex_client_catalog×2)改指新模块;commands/mod.rs 的 cfg 再导出收窄为 login_shell_path(R2 迁移)。

**验证**:cargo fmt --check 绿;cargo check --all-targets(GNU)0 错误 0 警告(1m12s)。

**边界记录**:linux 交叉检查被 glib/gdk 等 `-sys` 构建脚本阻塞(Windows 无 GTK dev);posix 分支为逐字搬运 + CI 最终门禁。`CommandDeadline::{remaining,timeout_error}` 因 misc 有调用者改为 pub(crate)。

**T6 进度**:17+2 处反向引用 → 已闭 6 处;剩余 11 处 = R2(工具发现/执行族:detected_tool_command×3、run_detected_tool_command_with_timeout、local_tool_version)+ R3(oauth 编排×5、initialize_codex_imagegen)+ R4(proxy 类型)。

## Round 6 — R2a/R2b:tool_runtime 深模块落地(2026-11-28)

**提交**:`322053b`(R2a,执行族)+ `28f781f`(R2b,探测/搜索路径族)。

**内容**:新增 `src-tauri/src/tool_runtime.rs`(1967 行)——工具运行时的唯一权威实现:
- VALID_TOOLS 白名单、detected_tool_command / run_detected_tool_command_with_timeout / local_tool_version 三大执行入口
- 工具定位(locate_default_tool / scan_cli_version / probe_path_default_version)、版本探测(try_get_version±wsl、ShellProbe)、搜索路径组装(push_unique_path/extend_from_*×8/pathext)
- Windows PATH 注册表重组+ %VAR% 展开、cmd 批处理构造、WSL 分发执行
- `commands/misc.rs` 10952→8742 行(-2210);**tool_runtime 对 commands 的依赖归零**(编译器驱动 9 轮迭代,scripts/extract-items.mjs 随批提交为治理工具)

**T6 进度**:19 处反向引用 → **已闭 11 处**(R1×6 + R2×4+1);剩余 8 处 = R3(oauth 编排×5 + initialize_codex_imagegen)+ R4(aihubmix set_global_proxy_url×1 + proxy 类型×1)。

**验证**:cargo fmt --check 绿;cargo check --all-targets(GNU)0 错误(仅存既有 agy dead_code 警告)。

**方法记录**:千行级函数族迁移用"脚本提取+编译器驱动迭代"替代手工搬移——`scripts/extract-items.mjs` 按名称提取顶层项(含 cfg/doc 前缀、enum/const 边界),每轮编译错误揭示遗漏成员,直到闭包完整。教训:attr 链前向收集会误吞相邻项(本例 WslShellPreferenceInput——恰好是正确归属,核实后保留);fmt 后的 use 块格式变化会使命令行 patch 失配,须重读再改。

## Round 7 — R3/R4:T6 收敛,生产反向依赖归零(2026-11-28)

**提交**:`83ea7c1`(R3)+ `6a85e40`(R4)。

**R3 内容**:
1. `services/codex_imagegen.rs`(214 行):initialize/install/ImagegenPolicyPatch 及测试逐字下沉;`commands/codex_imagegen.rs` 变薄壳(命令 + DTO)。
2. `services/global_proxy.rs`(47 行):set_global_proxy_url 编排(校验→持久化→应用)权威实现;Tauri 命令与 aihubmix auth-reset-proxy 都消费它。
3. **勘察修正**:provider/mod.rs 的 oauth×5 调用全部位于 `mod tests`——集成测试直接调用命令层入口属合法模式,从 T6 生产反向引用清单**重分类移除**,不做无价值搬迁。教训:反向依赖清单必须区分生产/测试代码。

**R4 内容**:CodexOAuthState/CopilotAuthState/XaiOAuthState 三个 newtype 从 commands 迁至各自 Manager 所在的 proxy/providers/*_auth.rs(类型与实现同址);消费方 ~10 文件改指 proxy::providers。

**T6 最终状态**:**生产反向依赖 19 → 0**。
- R1(child_process)×6 + R2(tool_runtime)×5 + R3(imagegen/global_proxy)×2 + R4(State 类型)×1 = 闭 14 处
- oauth×5 重分类为测试代码(非熵点)
- 依赖方向最终形态:`commands(编排) → services(领域) → {tool_runtime, child_process}(基础设施) → database/proxy`,`proxy → providers`(自包含),`aihubmix → services`。反向引用全部消除。

**验证**:cargo check --all-targets(GNU)0 错误;cargo fmt --check 绿。

**插曲**:R4 迁移时 extract-items.mjs 对单行分号结尾的元组结构体按列 0 `}` 找边界导致过采(把后续 Tauri 命令一起搬走)——通过 /tmp 提取块比对发现并恢复;脚本仍需对分号结尾 struct 加边界规则(TODO:脚本补丁,下批处理)。

**下一步**:T4(misc 剩余 ~8700 行:生命周期/更新/剪贴板按域拆分)、T14(env 读取归一)、T5(config 家族,待 P-ADR-002)。


## Round 8 — T4 批次1:工具生命周期族 → services::tool_lifecycle(2026-11-28)

**提交**:`0963ea6`。

**内容**:新增 `services/tool_lifecycle.rs`(1747 行)——生命周期域的权威实现:
- ToolLifecycleCoordinator(防并发)+ ToolLifecycleAction + normalize_requested_tools
- 13 工具安装/更新命令族(UNIX 脚本常量、Windows PowerShell 构造、npm 参数、
  CodexStandaloneInstall、mcode/grok/hermes 原生更新、包管理器锚定命令)
- 安装枚举(ToolInstallation/enumerate_tool_installations/default_install)与更新解析
  (resolve_update_command/UpdateCommand/prefers_official_update)
- shell 命令行构造(build_tool_action_line±wsl、tool_action_shell_command±for_shell)
- `commands/misc.rs` 8742→7013 行(自治理开始累计 10952→7013,-3939);
  prepare_tool_uninstall / run_tool_lifecycle_action 保留薄壳(P-ADR-003)
- commands::tool_uninstall / tool_node_requirement 经 misc 的 `pub(crate) use` glob
  消费(沿用 `pub use misc::*` 既有模式)

**验证**:cargo check --all-targets(GNU)0 错误;cargo fmt --check 绿。

**方法教训**(本批 9 轮编译迭代):
1. Node 脚本读 Git Bash /tmp 路径必须用 `cygpath -w` 换算(两次空跑)。
2. 脚本对"单行分号元组结构体"曾按列 0 `}` 找边界导致过采(R4 发现后已修);
   本批 attr 链前向收集又误吞相邻项——提取后必须以编译错误核对闭包,而非信任清单。
3. 字段可见性脚本因"doc 注释在前、正则误锚 ^
"连续三轮未生效——heredoc 内嵌 JS 的
   正则转义要实测;改用 indexOf 切片 + 字符串 join 一次成功。
4. 后台 cargo check 完成后立即读日志,不要叠加多个顺序 check(超时 2 次的根源)。

**T4 剩余**:misc ~7000 行 = 剪贴板/应用更新/便携模式/迁移结果/配置读写命令等
app-misc 域 + get_tool_versions 命令 + 测试;后续按域继续或保持现状。
