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
