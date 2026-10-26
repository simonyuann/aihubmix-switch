# CSMY 下载站更新源

这些文件运行在现有下载站 `/srv/hdd/download-station`，不需要客户端访问 GitHub。只从正式 Release 同步，SHA256 全部通过且六种更新格式、清单版本、包地址与签名文件一致后才切换 `data/current`。客户端继续使用固定公钥验证签名及版本。

部署时先备份现有 `server.py`、`sync_release.py`、`Dockerfile`，再应用 `integration.patch`，将本目录 `sync_release.py`、`update_feed.py` 复制到下载站根目录。将下载站 `package-source.py` 的源码文件列表加入 `update_feed.py`，重新生成公开源码包，然后 `docker compose up -d --build`。原服务的数据目录、持久化文件和凭据无需改动。

`/updates/aihubmix-switch/latest-v2.json` 返回当前完整的 2.x+ 清单；旧 `/updates/aihubmix-switch/latest.json` 只返回已经校验入库的最高 1.x 版本（缺失时为 404，绝不回退到 2.x）。`/updates/aihubmix-switch/v<版本>/<文件名>` 返回相应签名包，均直接读取本地磁盘，不代理上游。一般旧版本目录保留 7 天；最高已验证 1.x 快照及其包永久豁免清理，公开目录仍只展示当前版本。服务器启动后同步一次，以补齐已有 Release 的清单和签名；同步失败保持上一个完整版本。

同步器先读取不消耗 API 配额的 `<repo>/releases/latest` 重定向（必要时退回 `releases.atom`）得到最新正式版本，再按该 tag 下载公开的 `SHA256SUMS.txt` 和版本对应清单（1.x 为 `latest.json`，2.x+ 为 `latest-v2.json`）组装清单并校验，因此发布后几十秒内即可发现新版本，正常轮询不占用未认证 API 的 60 次/小时配额。`api.github.com` 仅在两者都失败时兜底；在 Compose 中设置 `GITHUB_TOKEN` 可把兜底配额提升到 5000 次/小时。预发布 tag 会被跳过。单个资产若被代理截断或校验失败，会在同一次运行内重新下载（最多 3 次），不必等下一个轮询周期；`Content-Length` 与实际字节数不符时直接判定为截断。

本地回归：`python3 -m unittest discover -s deploy/download-station -p 'test_*.py'`。测试覆盖校验失败不切换、异常清单不切换、版本切换后旧包可下载、保留期限、目录限制及手工持久化文件保留。

## macOS 11 兼容通道切换

必须先部署本目录的新版 feed 路由与清理逻辑，再发布 2.0.0。v2 客户端改用 `latest-v2.json`，服务 origin、公钥及六种安装格式目标、签名包 URL 均不变。旧版客户端无法识别最低系统版本，不能通过增加清单字段来阻止 macOS 11 安装 2.x，因此旧静态路径明确冻结在兼容的 1.x。此策略同时让其他平台 1.x 客户端继续留在旧通道；首次跨至 v2 需手动下载安装，macOS 用户先确认系统为 12.0+。

部署前确认当前已验证的 1.x 快照含 `updater.json`、原始清单、签名和六种包；同步器按已验证快照选取最高 1.x，清理保留该目录。若镜像尚无此快照，应先从已发布的 1.x 用相同 `sync_once` 校验流程导入；不得复制一份未验证清单充作兼容源。未完成引导时旧路径安全返回 404，不允许重定向到 GitHub latest 或 v2。`node scripts/verify-mirror.mjs <v2版本>` 检查 v2 六格式签名，并要求旧路径返回 1.x 或 404。以上变更仅为待部署源码，本分支未发布或修改线上服务。
