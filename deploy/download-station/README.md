# CSMY 下载站更新源

这些文件运行在现有下载站 `/srv/hdd/download-station`，不需要客户端访问 GitHub。只从正式 Release 同步，SHA256 全部通过且对应版本要求的更新格式、清单版本、包地址与签名文件一致后才切换 `data/current`。客户端继续使用固定公钥验证签名及版本。

部署时先备份现有 `server.py`、`sync_release.py`、`Dockerfile`，再应用 `integration.patch`，将本目录 `sync_release.py`、`update_feed.py` 复制到下载站根目录。将下载站 `package-source.py` 的源码文件列表加入 `update_feed.py`，重新生成公开源码包，然后 `docker compose up -d --build`。原服务的数据目录、持久化文件和凭据无需改动。

`/updates/aihubmix-switch/latest-v2.json` 返回当前完整的 1.1.0+ 重构版清单（沿用已有路径名）；旧 `/updates/aihubmix-switch/latest.json` 只返回已经校验入库的最高 1.0.x 版本（缺失时为 404，绝不回退到 1.1.0+）。`/updates/aihubmix-switch/v<版本>/<文件名>` 返回相应签名包，均直接读取本地磁盘，不代理上游。一般旧版本目录保留 7 天；最高已验证 1.0.x 快照及其包永久豁免清理，公开目录仍只展示当前版本。服务器启动后同步一次，以补齐已有 Release 的清单和签名；同步失败保持上一个完整版本。

未配置认证时，同步器先读取不消耗 API 配额的 `<repo>/releases/latest` 重定向（必要时退回 `releases.atom`）得到最新正式版本，再按该 tag 下载公开的 `SHA256SUMS.txt` 和版本对应清单（1.0.x 为 `latest.json`，1.1.0+ 为 `latest-v2.json`）组装清单并校验。`api.github.com` 仅在两者都失败时兜底。配置 `GITHUB_TOKEN` 后直接通过认证 API 读取最新正式 Release，并通过该仓库的 Release Assets API 下载附件，支持私有仓库；不再尝试公开页面。预发布和草稿会被拒绝。单个资产若被代理截断或校验失败，会在同一次运行内重新下载（最多 3 次），不必等下一个轮询周期；`Content-Length` 与实际字节数不符时直接判定为截断。

## 私有仓库认证

创建 Fine-grained personal access token，仅选择 `simonyuann/aihubmix-switch`，授予 `Contents: Read-only`。将 `GITHUB_TOKEN=<生成的值>` 保存在服务器 `/srv/hdd/download-station/.secrets/github-release.env`，目录权限 0700、文件权限 0600。仅在 `compose.yaml` 的 `release-sync` 服务增加 `env_file: .secrets/github-release.env`，不要给公开下载服务注入该凭据。将 `.secrets` 加入 `.dockerignore`；凭据文件不得加入仓库或公开源码包。

更新同步器后运行 `docker compose build release-sync`，再运行 `docker compose up -d --no-deps --force-recreate release-sync`。单纯 `restart` 不会加载新的环境变量。后续轮换 Token 时替换凭据文件并重新创建该服务即可。

认证头只发送给该仓库的 Release API；附件请求使用 `Accept: application/octet-stream`。GitHub 跳转至签名文件存储地址时移除认证头，拒绝 HTTP 降级。认证或下载失败保持已验证的当前版本，错误信息不包含 Token。客户端及 DL 公网下载仍使用下载站的本地文件，不需要 GitHub 凭据。

本地回归：`python3 -m unittest discover -s deploy/download-station -p 'test_*.py'`。测试覆盖校验失败不切换、异常清单不切换、版本切换后旧包可下载、保留期限、目录限制及手工持久化文件保留。

## macOS 11 兼容通道切换

必须先部署本目录的新版 feed 路由与清理逻辑，再发布 1.1.0。1.1.0+ 客户端改用 `latest-v2.json`，服务 origin、公钥及签名包 URL 规则均不变。旧版客户端无法识别最低系统版本，不能通过增加清单字段来阻止 macOS 11 安装 1.1.0+，因此旧静态路径明确冻结在兼容的 1.0.x。此策略同时让其他平台 1.0.x 客户端继续留在旧通道；首次升级至重构版需手动下载安装，macOS 用户先确认系统为 12.0+。

部署前确认当前已验证的 1.0.x 快照含 `updater.json`、原始清单、签名和六种包；同步器按已验证快照选取最高 1.0.x，清理保留该目录。若镜像尚无此快照，应先从已发布的 1.0.x 用相同 `sync_once` 校验流程导入；不得复制一份未验证清单充作兼容源。未完成引导时旧路径安全返回 404，不允许重定向到 GitHub latest 或重构版。`node scripts/verify-mirror.mjs <版本>` 检查四种重构版更新格式的签名，并要求旧路径返回 1.0.x 或 404。发布前部署并核验路由，再由完整 Release 触发自动同步。

1.1.0+ 当前仅要求 macOS arm64 / x64 App 更新归档、Windows x64 NSIS 与便携版四种格式和各自签名，并保留两种 macOS DMG；不等待 Linux。1.0.x 兼容快照仍要求原六种格式全部有效。缺失任何本轮必需格式或签名时保持原 current。
