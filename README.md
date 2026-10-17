# AIHubMix 接入助手

一个面向 [AIHubMix](https://csmy.fun:23457) 用户的 Windows 桌面客户端。输入在平台申请的 API Key，即可一键接入 Codex、Pi、OpenCode、Claude Code，并支持在客户端内注册账号、申请新的 API Key。

参考了 [cc-switch](https://github.com/farion1231/cc-switch) 的配置管理思路，但只保留「连接 + 一键接入 + 还原」这条最短路径。

## 功能

- **一个 Key 接入多个 Agent**：Codex、Pi、OpenCode、Claude Code。
- **客户端内申请 API Key**：登录/注册后读取可用分组（DeepSeek / OpenAI / GLM 等），创建 Key 并自动测试、保存。
- **连接检测**：读取模型列表、余额、分组倍率和 `/v1/messages` 支持情况。
- **安全写入**：读取-合并-原子写入，只修改本工具负责的字段，写入前自动备份。
- **一键还原**：恢复到接入前的配置（Codex 为整文件恢复，其余为字段级恢复）。
- **模型选择**：从 `/v1/models` 拉取模型，自动合并 Codex 模型清单里的上下文长度、推理能力、图片能力。
- **简洁界面**：单窗口，一步连接、一步接入，不需要手改 JSON/TOML。

## 当前接入方式

| Agent | 配置路径 | 写入内容 |
|---|---|---|
| Claude Code | `~/.claude/settings.json` | `ANTHROPIC_BASE_URL`、`ANTHROPIC_AUTH_TOKEN`、默认模型（需要分组开启 `/v1/messages` 调度） |
| Codex | `~/.codex/config.toml` + `auth.json` | `model_providers.csmy`、`model_provider`、`model`、`model_reasoning_effort`、API Key |
| Pi | `~/.pi/agent/models.json` + `settings.json` | `providers.csmy`、`defaultProvider`、`defaultModel` |
| OpenCode | `~/.config/opencode/opencode.json(c)` | `provider.csmy`、顶层 `model` |

默认中转地址：`https://csmy.fun:23457`

- OpenAI 兼容：`https://csmy.fun:23457/v1`
- Anthropic 兼容：`https://csmy.fun:23457`（Claude Code 自己拼 `/v1/messages`）

## 重要说明

- **Claude Code 需要平台分组开启 `allow_messages_dispatch`**。当前 openai/deepseek 分组默认未开启时，客户端会显示「Claude Code 未开启」并禁止写入，避免产生无法工作的配置。
- Codex 使用 `wire_api = "responses"`；如遇服务端不支持，可在设置里切换为 `chat`。
- Pi 默认使用 `openai-responses`；如遇兼容问题，可在设置里切换为 `openai-completions`。
- 备份目录：`%APPDATA%/aihubmix-switch/backups/<agent>/<时间戳>/`
- API Key 使用 Electron `safeStorage`（Windows DPAPI）加密保存在本机；写入 Agent 配置时才会出现在对应配置文件中。

## 开发

```bash
npm install
npm run icon      # 生成 build/icon.png
npm run dev       # 开发模式（需要图形环境）
npm run typecheck
npm test
npm run build
```

## 打包 Windows

在 Windows 上：

```bash
npm run build
npx electron-builder --win
```

产物在 `release/`：

- `AIHubMix Switch-<version>-x64.exe`（NSIS 安装包）
- `AIHubMix Switch-<version>-portable.exe`（免安装版）

在 Linux 上交叉打包 NSIS 需要安装 wine；没有 wine 时可以用：

```bash
npx electron-builder --win --dir
```

生成 `release/win-unpacked/`，压缩后即可在 Windows 上直接运行。

推荐使用 GitHub Actions：推送 tag（如 `v0.1.0`）后自动在 `windows-latest` 上构建并上传 Release。

## 测试

```bash
npm test
```

包含：

- 4 个 Agent 配置适配器的读写/还原测试
- URL 规范化、模型排序、模型清单解析
- React 界面冒烟测试（jsdom）
- 可选的真实中转站探测（设置 `TEST_API_KEY` 后运行 `tests/live.test.ts`）
