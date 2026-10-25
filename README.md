# AIHubMix Switch

白色、简洁的 API 与智能体接入助手。基于 **Tauri 2 + Rust + React**，支持 Windows 与 macOS，运行时使用系统 WebView，无需安装 Node.js。

[下载安装包](https://github.com/simonyuann/aihubmix-switch/releases/latest) · [更新说明](docs/releases/v0.2.0.md)

## 使用

1. 注册或登录 AIHubMix；已有 API Key 可直接添加，不必登录。
2. 在「我的 API」创建、查看或复制 Key。
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
| Windows 免安装 | `AIHubMix-Switch_<版本>_windows-x64-portable.exe` |

macOS：将应用拖入 Applications。当前使用 ad-hoc 签名，未通过 Apple Developer ID 公证，首次运行可能需要在「系统设置 → 隐私与安全性」允许打开。Windows 安装包暂未签名；免安装版需要系统已有 Edge WebView2 Runtime。Release 附 SHA256 校验文件。

## 配置与安全

- 每个智能体独立记录所用 Key 指纹，不把「当前选择的 Key」当作所有智能体的配置。
- 查看、复制和选择 Key 不修改智能体配置；明确点击导入后才写入。
- 只修改接入所需字段，保留无关配置及 TOML / JSONC 注释。写入采用临时文件 + 原子替换，多文件操作失败时回滚已修改文件。
- 导入前自动备份；再次更换 Key 或模型仍保留首次导入前的基线。还原时只恢复仍与本工具写入值一致的字段，保留后续手动修改。
- API Key 与登录凭据通过 ChaCha20-Poly1305 加密存入本机 vault，主密钥由 macOS Keychain / Windows Credential Manager 保管。安全存储无法访问时不会回退为明文。
- 完整 Key 默认隐藏，主动查看 60 秒后自动隐藏。导入后目标工具的配置文件及其备份可能包含原始凭据，备份目录属于本机敏感数据。
- Claude Code 需要 Key 所属服务支持 Anthropic `/v1/messages`；不支持时阻止导入并提示更换 Key。

默认服务地址：`https://csmy.fun:23457`。

| 智能体 | 默认配置位置 | 可选环境变量 |
| --- | --- | --- |
| Codex | `~/.codex/config.toml`、`auth.json` | `CODEX_HOME` |
| Claude Code | `~/.claude/settings.json` | `CLAUDE_CONFIG_DIR` |
| OpenCode | `~/.config/opencode/opencode.json(c)` | `OPENCODE_CONFIG`、`OPENCODE_CONFIG_DIR`、`XDG_CONFIG_HOME` |
| Pi | `~/.pi/agent/models.json`、`settings.json` | `PI_CODING_AGENT_DIR` |

应用数据：macOS 为 `~/Library/Application Support/aihubmix-switch/`，Windows 为 `%APPDATA%/aihubmix-switch/`。新版设置 `state-v2.json`、加密凭据 `vault-v2.bin`、备份 `backups-v2/`。

从 Electron 升级时需要重新登录或粘贴 Key；旧 `state.json` 和 `backups/` 完整保留，已写入的智能体配置不会自动更改。旧备份需通过数据目录访问，不会自动导入新版还原记录。

## 开发

需要 Node.js 22.12+（CI 使用 24）、Rust stable；macOS 安装 Xcode Command Line Tools，Windows 安装 MSVC Build Tools 与 WebView2。参见 [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)。

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

GitHub Actions 运行类型检查、前端交互测试、Rust 格式/Clippy/配置适配器测试，随后构建 macOS arm64、macOS Intel、Windows x64 安装包及压缩包。推送与三个版本文件一致的 `v<版本>` tag 后，只有全部平台构建成功才发布 Release，避免残缺发布。

```bash
npm run tauri -- build --target aarch64-apple-darwin --bundles app,dmg -- --locked
npm run tauri -- build --target x86_64-apple-darwin --bundles app,dmg -- --locked
npm run tauri -- build --target x86_64-pc-windows-msvc --bundles nsis -- --locked
```

跨架构 macOS 编译需先 `rustup target add x86_64-apple-darwin`。Windows 安装包在 Windows runner 构建。

MIT License。
