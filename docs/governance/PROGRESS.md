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
