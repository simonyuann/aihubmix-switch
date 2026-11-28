# 架构地图(Architecture Map)

> 持续更新。本版基于初步勘察 + 并行勘察代理报告,标注"待补"处随治理推进补全。

## 1. 技术栈概览

- **产品**:AIHub Switch — AIHubMix 接入助手,为 Codex / Claude Code / OpenCode / Gemini CLI / Grok / Kimi / Pi / Hermes / OpenClaw / mcode / omp / agy 等多种 AI 工具做 API Key 一键接入、分组管理、代理与用量统计(Windows/Linux/macOS,Tauri 桌面应用)。
- **前端**:React + TypeScript + Vite + Tailwind(约 16 万行 TS/TSX,515 文件)。
- **后端**:Rust / Tauri 2(约 33 万行,375 文件),本地 SQLite(database/)、内置代理(proxy/)、系统托盘(tray)。
- **测试**:vitest(前端组件/集成)+ Rust 文件内 `mod tests` + 少量 node 脚本测试(.mjs)与 python 验证脚本。
- **命令面**:`lib.rs` 单个 `generate_handler!` 注册 **378 个 invoke 命令**。

## 2. 系统拓扑与依赖方向(现状骨架)

```
React UI (src/components)
   │  invoke() — 期望经 lib/api 封装(散落程度待补,见 T3)
   ▼
Rust commands/ (378 命令,编排层) ──► mode/ (切换控制器,13.5K) ──► aihubmix/ (平台服务,8.5K)
   │                                        │
   ▼                                        ▼
services/ (领域层, 82.9K) ──► database/ (SQLite) + store.rs (settings/secrets)
   │
   ├── proxy/ (内置 HTTP 代理, providers/ 内含 codex/claude 等协议转换)
   ├── *_config.rs 家族 (14 文件 ~25K,各工具配置文件读写)
   └── session_manager / tray / sync(s3/webdav)

⚠ 已知反向引用(待治理,T6): services/→commands:: ×17, aihubmix/→commands:: ×1, proxy/→commands:: ×1
```

## 3. Rust 模块地图(顶层)

| 模块 | 行数 | 职责 | 备注 |
| --- | --- | --- | --- |
| `commands/` | 25.4K(50 文件) | invoke 命令编排层 | `misc.rs` 10952 行垃圾抽屉(T4) |
| `services/` | 82.9K | 领域服务 | `session_usage_*` 5 个平行实现 ~14K(熵点);`provider/` 子家族 ~14K |
| `mode/` | 13.5K | 分组切换控制器 | `controller.rs` 9694 行(巨型) |
| `proxy/` | ~25K(估) | 内置代理 + 协议转换 | `forwarder.rs` 8895、`transform_codex_chat.rs` 6803、`transform_responses.rs` 5706 |
| `*_config.rs` 家族 | ~25K(14 文件) | 各工具配置读写 | 平行 `get_*_dir/path/env` 形状(T5,收敛目标) |
| `aihubmix/` | 8.5K | AIHubMix 平台协议 | dispatch 风格命令分发 |
| `database/` | 待补 | SQLite 访问 | |
| `mcp/` `session_manager/` `deeplink/` `live/` `pi_config/` | 待补 | 子系统 | |
| 顶层单文件 | ~15K+ | `tray.rs` 4984、`codex_history_migration.rs`、`settings.rs`、`store.rs` 等 | 待补 |

## 4. 前端模块地图(待勘察代理报告补全)

- `components/` 316 文件 97K 行;`lib/` 81 文件 10.6K;`config/` 含 7215 行 `openclawProviderPresets.ts`、4929 行 `codexProviderPresets.ts`;`contexts/` 仅 2 文件(状态所有权待补)。

## 5. 已知熵点总表(详细证据见 TASKS.md)

| # | 熵点 | 规模 | 任务 |
| --- | --- | --- | --- |
| E1 | `commands/misc.rs` 混装 8 工具生命周期/版本/更新/剪贴板 | 10952 行 | T4 |
| E2 | `*_config.rs` 14 文件平行实现 | ~25K 行 | T5 / P-ADR-002 |
| E3 | `session_usage_{codex,pi,grokbuild,opencode}` + `session_usage` 平行实现 | ~14K 行 | 待拆任务 |
| E4 | 跨层反向引用 services→commands | 17+1+1 处 | T6 |
| E5 | vite/vitest/tsconfig 双份配置 | 根目录 | T1 |
| E6 | 版本号 3 处定义(package/tauri.conf/Cargo) | 3 处 | T2 |
| E7 | 无 lint/无架构约束自动化 | 全局 | T8/T9 |
| E8 | 巨型文件:mode/controller.rs 9694、proxy/forwarder.rs 8895、services/provider/mod.rs 8717、services/skill.rs 8140 | >30K 行 | 待拆任务 |
| E9 | `#[cfg]` 平台重复函数族(misc.rs 内 4 组) | misc.rs | T4 |
| E10 | 前端散落 invoke 调用(规模待补) | 待补 | T3 |

## 6. 状态所有权地图(待补)

- Rust: `store.rs`(settings/secrets)、`database/`(SQLite)——读写边界待补。
- 前端: `contexts/AccountContext`(+1)——其余状态归属待补。
