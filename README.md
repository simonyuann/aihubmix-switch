# AIHubMix Switch

白色、简洁的 API 与智能体接入助手。基于 **Tauri 2 + Rust + React**，支持 Windows、macOS 与 Linux，运行时使用系统 WebView，无需安装 Node.js。

[下载安装包](https://github.com/simonyuann/aihubmix-switch/releases/latest) · [更新说明](docs/releases/v0.3.0.md)

## 使用

1. 注册或登录 AIHubMix；已有 API Key 可直接添加，不必登录。
2. 在「我的 API」创建、查看、复制或删除 Key。
3. 点击某个 Key 的「导入」，选择智能体和模型。通过真实流式与工具调用验证后，自动备份并写入配置。
4. 「已接入」只显示配置过的工具，可更换配置、还原。

0.3.0 覆盖 CC Switch 当前的 13 种智能体配置入口，导入时可搜索，首页只展示 API 与已接入项。导入前会发送两次简短测试请求，可能产生少量模型费用；验证失败时保留原配置。未定位到 CLI 不等于未安装，可重新扫描或在高级设置填写完整路径。Windows 原生和 WSL 的安装与配置相互独立。

Gemini CLI / Antigravity CLI 需要 Gemini 原生接口；Claude Desktop 需要支持第三方网关的新版本和 Claude 模型。MiniMax Code / Oh My Pi 导入后在原工具选择 csmy 下的模型。Antigravity CLI 需填写原生显示名并使用生成的专用启动命令；普通 agy 和 IDE 登录不变。常用 IDE 插件另外提供手动接入指南。

「会话」支持本机 Claude Code、Codex、Pi、Grok Build、Gemini CLI、OpenClaw 的记录浏览、标题/项目/预览搜索与复制继续命令，默认只读，不上传内容。**Codex CLI 统一续聊历史开关安排在 0.3.1**，与此会话浏览页不同。

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

Linux 安装：Debian / Ubuntu 优先使用 `sudo apt install ./AIHubMix-Switch_<版本>_linux-x64.deb` 自动安装系统依赖；AppImage 需 `chmod +x` 后运行，部分发行版需安装 FUSE。运行环境需提供 WebKitGTK 4.1、GTK 3 与 Secret Service。Linux 构建及配置测试由 Ubuntu 22.04 runner 验证。

## 在线更新

带更新入口的版本启动时后台检查新版本，也可在「设置 → 软件更新」手动检查。下载并通过签名校验后，确认「安装并重启」即可覆盖升级，API Key、登录信息和备份保留。重启会短暂中断本地 Claude 兼容服务。没有更新入口的旧版本需先手动覆盖安装一次 v0.2.2，无需卸载。

Windows 安装版、macOS App、Linux AppImage / deb 支持对应格式的原生更新；deb 可能需要系统授权。Windows 免安装版检测到更新后直接下载对应免安装文件，不会自动安装安装版。更新包的独立签名与版本绑定均由官方 Tauri Updater 验证，禁止降级。

发布新版本：同步 package.json、Cargo.toml、Cargo.lock、tauri.conf.json 版本，添加 docs/releases/v<版本>.md 并推送 v<版本> tag。CI 在四平台测试和打包后，对六种格式签名、校验并生成 latest.json，先上传草稿，核对全部 16 个文件的摘要后才公开发布。普通源码 push 只运行检查，不向用户推送未发布变更。

更新签名使用仓库 Secrets `TAURI_SIGNING_PRIVATE_KEY` 和 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`，客户端公钥固定在 tauri.conf.json。首次配置后需长期保存私钥备份；不应为每个版本重新生成密钥。开发构建默认不允许应用内安装更新。

## 配置与安全

- 每个智能体独立记录所用 Key 指纹，不把「当前选择的 Key」当作所有智能体的配置。
- 查看、复制和选择 Key 不修改智能体配置；明确点击导入后才写入。
- 只修改接入所需字段，保留无关配置及 TOML / JSONC 注释。写入采用临时文件 + 原子替换，多文件操作失败时回滚已修改文件。
- 导入前自动备份；再次更换 Key 或模型仍保留首次导入前的基线。还原时只恢复仍与本工具写入值一致的字段，保留后续手动修改。
- API Key 与登录凭据通过 ChaCha20-Poly1305 加密存入本机 vault，主密钥由 macOS Keychain / Windows Credential Manager / Linux Secret Service 保管。安全存储无法访问时不会回退为明文。
- 完整 Key 默认隐藏，主动查看 60 秒后自动隐藏。导入后目标工具的配置文件及其备份可能包含原始凭据，备份目录属于本机敏感数据。
- Antigravity CLI 的专用启动脚本也包含凭据；Unix 下使用私有目录与 0600 文件。不要分享配置、脚本或备份。还原仅移除本工具仍未被用户改动的配置。
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
| Grok Build | `~/.grok/config.toml` | — |
| Kimi Code | `~/.kimi-code/config.toml` | `KIMI_CODE_HOME` |
| OpenClaw | `~/.openclaw/openclaw.json` | `OPENCLAW_CONFIG_PATH`、`OPENCLAW_STATE_DIR` |
| Gemini CLI | `~/.gemini/settings.json`、`.env` | `GEMINI_CLI_HOME`（用户根目录，配置在其 `.gemini` 下） |
| Claude Desktop | 系统应用配置中的 `Claude`、`Claude-3p/configLibrary` | Windows `LOCALAPPDATA`、Linux `XDG_CONFIG_HOME` |
| Hermes Agent | `~/.hermes/config.yaml`；Windows LocalAppData 下 `hermes` | `HERMES_HOME` |
| MiniMax Code | `~/.minimax/config.yaml` | `MINIMAX_DATA_DIR`、`MAVIS_DATA_DIR` |
| Oh My Pi | `~/.omp/agent/models.yml` | `PI_CONFIG_DIR`、`OMP_PROFILE` / `PI_PROFILE`、`PI_CODING_AGENT_DIR` |
| Antigravity CLI | 本应用数据目录 `agent-profiles/agy` | 专用脚本经环境变量传递 Key，不改 shell 启动配置 |

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
npm run tauri -- build --target x86_64-unknown-linux-gnu --bundles deb,appimage -- --locked
```

跨架构 macOS 编译需先 `rustup target add x86_64-apple-darwin`。Windows 安装包在 Windows runner 构建。

MIT License。


Claude Code 兼容层已使用真实 CLI 完成烟雾验证，并使用现有 AIHubMix 服务完成流式的两轮工具调用。可选开发验证为 `AIHUBMIX_TEST_CLAUDE_CLI=1 cargo test --manifest-path src-tauri/Cargo.toml live_claude_tool_round_trip -- --ignored --nocapture`；该命令使用已指向同一服务的本机智能体配置，产生少量真实 API 请求，不修改原配置或会话记录。普通测试与 CI 使用模拟服务，不接触真实凭据。

### v0.3.1：Codex CLI 统一续聊历史

在设置中开启「Codex 续聊历史」，官方登录与本工具 API 连接共用 csmy 历史分组。默认关闭，旧官方记录需主动勾选迁移；同步会话 JSONL 和 SQLite 索引，迁移前备份。关闭时可按账本还原原分组，保留后续消息与手动修改。操作前关闭 Codex CLI / App，之后重启 CLI；profile 或命令行 provider 覆盖需单独处理。跨服务加密推理可能无法续用，桌面版通常无需开启。

v0.3.1 同时启用 Rust 系统代理和系统根证书，改善网页正常但桌面客户端连接失败的情况。服务入口本身不可达时仍需恢复服务，软件不会降低 TLS 校验或猜测替换 API 地址。
