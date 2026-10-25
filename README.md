# AIHubMix Switch

白色、简洁的 API 与智能体接入助手。基于 **Tauri 2 + Rust + React**，支持 Windows、macOS 与 Linux，运行时使用系统 WebView，无需安装 Node.js。

[下载安装包](https://github.com/simonyuann/aihubmix-switch/releases/latest) · [更新说明](docs/releases/v0.2.0.md)

## 使用

1. 注册或登录 AIHubMix；已有 API Key 可直接添加，不必登录。
2. 在「我的 API」创建、查看、复制或删除 Key。
3. 点击某个 Key 的「导入」，选择智能体和模型，完成配置。
4. 「已接入」只显示配置过的工具，可更换配置、还原。

支持 Codex、Claude Code、OpenCode、Pi。思考强度与配置路径放在导入面板的「高级设置」，接口兼容格式放在设置中。新建 Key 后可直接导入多个智能体，无需手动复制粘贴。

充值入口已保留，当前为「即将开放」占位，不发起任何付款。Key 配额与账户钱包余额是不同概念，未获取到的数据不会显示为零。

## 安装

| 系统 | 下载文件 |
| --- | --- |
| macOS 11+，Apple Silicon | `AIHubMix-Switch_<版本>_macos-arm64.dmg` |
| macOS 11+，Intel | `AIHubMix-Switch_<版本>_macos-x64.dmg` |
| Windows 10/11 x64 | `AIHubMix-Switch_<版本>_windows-x64-setup.exe` |
| Linux x64，Ubuntu 22.04+ 等 | `AIHubMix-Switch_<版本>_linux-x64.AppImage` / `.deb` |
| Windows 免安装 | `AIHubMix-Switch_<版本>_windows-x64-portable.exe` |

macOS：将应用拖入 Applications。当前使用 ad-hoc 签名，未通过 Apple Developer ID 公证，首次运行可能需要在「系统设置 → 隐私与安全性」允许打开。Windows 安装包暂未签名；免安装版需要系统已有 Edge WebView2 Runtime。Release 附 SHA256 校验文件。

## 配置与安全

- 每个智能体独立记录所用 Key 指纹，不把「当前选择的 Key」当作所有智能体的配置。
- 查看、复制和选择 Key 不修改智能体配置；明确点击导入后才写入。
- 只修改接入所需字段，保留无关配置及 TOML / JSONC 注释。写入采用临时文件 + 原子替换，多文件操作失败时回滚已修改文件。
- 导入前自动备份；再次更换 Key 或模型仍保留首次导入前的基线。还原时只恢复仍与本工具写入值一致的字段，保留后续手动修改。
- API Key 与登录凭据通过 ChaCha20-Poly1305 加密存入本机 vault，主密钥由 macOS Keychain / Windows Credential Manager / Linux Secret Service 保管。安全存储无法访问时不会回退为明文。
- 完整 Key 默认隐藏，主动查看 60 秒后自动隐藏。导入后目标工具的配置文件及其备份可能包含原始凭据，备份目录属于本机敏感数据。
- Claude Code：原生 Anthropic 模型可直接连接；OpenAI 兼容 Key 通过本机 Rust 兼容层使用 Responses / Chat Completions，支持流式输出、图片、工具调用和多轮工具结果。使用的仍是你选择的模型。
- 本地兼容层只监听 `127.0.0.1`，使用独立随机令牌鉴权，上游 Key 不写入 Claude 配置。启用兼容连接后关闭窗口会保留托盘运行，通过托盘菜单退出才停止服务。使用时请保持应用运行。
- 兼容连接通过 Claude Code 官方自定义模型选项注册所选模型，并将 MCP 工具提前加载（`ENABLE_TOOL_SEARCH=false`），不转发 Anthropic 服务端专用工具；`count_tokens` 是本地估算。Responses 推理上下文以不透明签名保留，支持后续工具调用。
- 删除平台 Key 会调用真实删除接口，操作前确认并提示关联智能体；不会自动改写或还原这些工具的配置。本机 Key 的移除只作用于当前设备。

默认服务地址：`https://csmy.fun:23457`。

| 智能体 | 默认配置位置 | 可选环境变量 |
| --- | --- | --- |
| Codex | `~/.codex/config.toml`、`auth.json` | `CODEX_HOME` |
| Claude Code | `~/.claude/settings.json` | `CLAUDE_CONFIG_DIR` |
| OpenCode | `~/.config/opencode/opencode.json(c)` | `OPENCODE_CONFIG`、`OPENCODE_CONFIG_DIR`、`XDG_CONFIG_HOME` |
| Pi | `~/.pi/agent/models.json`、`settings.json` | `PI_CODING_AGENT_DIR` |

应用数据：macOS 为 `~/Library/Application Support/aihubmix-switch/`，Windows 为 `%APPDATA%/aihubmix-switch/`，Linux 为 `~/.local/share/aihubmix-switch/`。新版设置 `state-v2.json`、加密凭据 `vault-v2.bin`、备份 `backups-v2/`。

从 Electron 升级时需要重新登录或粘贴 Key；旧 `state.json` 和 `backups/` 完整保留，已写入的智能体配置不会自动更改。旧备份需通过数据目录访问，不会自动导入新版还原记录。

## 开发

需要 Node.js 22.12+（CI 使用 24）、Rust stable；macOS 安装 Xcode Command Line Tools，Windows 安装 MSVC Build Tools 与 WebView2。Linux 需要 WebKitGTK 4.1、GTK 3、DBus、AppIndicator 及可用的 Secret Service 密钥环（例如 GNOME Keyring）。参见 [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)。

```bash
npm ci
npm run dev             # Tauri 桌面开发
npm run typecheck
npm test                # React 关键交互测试
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run build:desktop   # 本机原生安装包
```

只预览界面：`npm run dev:web`，打开 `http://127.0.0.1:1420/?preview=1`。此模式使用明确标注的演示数据，不读写真实账户和智能体配置。可用 `&scenario=guest` 或 `&scenario=empty` 检查空状态。演示模块不会打包进生产客户端。

## 构建与发布

GitHub Actions 运行类型检查、前端交互测试、Rust 格式/Clippy/配置适配器测试，随后构建 macOS arm64、macOS Intel、Windows x64、Linux x64 安装包及压缩包。推送与三个版本文件一致的 `v<版本>` tag 后，只有全部平台构建成功才发布 Release，避免残缺发布。

```bash
npm run tauri -- build --target aarch64-apple-darwin --bundles app,dmg -- --locked
npm run tauri -- build --target x86_64-apple-darwin --bundles app,dmg -- --locked
npm run tauri -- build --target x86_64-pc-windows-msvc --bundles nsis -- --locked
```

跨架构 macOS 编译需先 `rustup target add x86_64-apple-darwin`。Windows 安装包在 Windows runner 构建。

MIT License。

Linux 安装：Debian / Ubuntu 优先使用 `sudo apt install ./AIHubMix-Switch_<版本>_linux-x64.deb` 自动安装系统依赖；AppImage 需 `chmod +x` 后运行，部分发行版需安装 FUSE。运行环境需提供 WebKitGTK 4.1、GTK 3 与 Secret Service。Linux 构建及配置测试由 Ubuntu 22.04 runner 验证。

Claude Code 兼容层已使用真实 CLI 完成烟雾验证，并使用现有 AIHubMix 服务完成流式的两轮工具调用。可选开发验证为 `AIHUBMIX_TEST_CLAUDE_CLI=1 cargo test --manifest-path src-tauri/Cargo.toml live_claude_tool_round_trip -- --ignored --nocapture`；该命令使用已指向同一服务的本机智能体配置，产生少量真实 API 请求，不修改原配置或会话记录。普通测试与 CI 使用模拟服务，不接触真实凭据。
