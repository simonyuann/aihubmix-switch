//! 工具版本检查:本地版本探测编排 + 各渠道最新版本获取(npm dist-tags/GitHub
//! releases/PyPI/agy/hermes)+ 语义化版本比较与"最新版被本地落后"判定。
//!
//! Single Source of Truth:get_tool_versions 命令(`commands::misc`,薄壳)与
//! 版本比较规则都消费本模块。(治理 T4,见 docs/governance/TASKS.md。)

use crate::child_process::{decode_command_output, CommandDeadline};
use crate::services::tool_lifecycle::*;
use crate::tool_runtime::*;
use std::path::Path;

use once_cell::sync::Lazy;
use regex::Regex;
#[derive(serde::Serialize)]
pub struct ToolVersion {
    name: String,
    version: Option<String>,
    latest_version: Option<String>, // 新增字段：最新版本
    error: Option<String>,
    /// 已定位到可执行文件、但 `--version` 报错退出（装了却跑不起来，如 Node 版本不达标）。
    /// 供前端区分"未安装"与"已安装·无法运行"，无需匹配 error 文案反推语义。
    installed_but_broken: bool,
    /// 工具运行环境: "windows", "wsl", "macos", "linux", "unknown"
    env_type: String,
    /// 当 env_type 为 "wsl" 时，返回该工具绑定的 WSL distro（用于按 distro 探测 shells）
    wsl_distro: Option<String>,
    /// 有新版本、但新版本要求的 Node 比这处安装用的新：前端不给升级按钮，提示先升级 Node。
    update_blocked_by_node: Option<NodeRequirementGap>,
}

pub(crate) async fn get_single_tool_version_impl(
    tool: &str,
    wsl_shell: Option<&str>,
    wsl_shell_flag: Option<&str>,
) -> ToolVersion {
    debug_assert!(
        VALID_TOOLS.contains(&tool),
        "unexpected tool name in get_single_tool_version_impl: {tool}"
    );

    // 判断该工具的运行环境 & WSL distro（如有）
    let (env_type, wsl_distro) = tool_env_type_and_wsl_distro(tool);

    // 使用全局 HTTP 客户端（已包含代理配置）
    let client = crate::proxy::http_client::get();

    // 1. 获取本地版本
    let probe = probe_local_version(tool, wsl_distro.as_deref(), wsl_shell, wsl_shell_flag);
    let (local_version, local_error, installed_but_broken) = match probe {
        ShellProbe::Found(v) => (Some(v), None, false),
        ShellProbe::FoundButFailed(e) => (None, Some(e), true),
        // 纯粹没装不带 error：版本列已显示「未安装」，再显示一行英文原因是重复的。
        // WSL 的 shell 参数非法、执行失败等其它原因仍照常上报。
        ShellProbe::NotFound(e) if e.ends_with(NOT_INSTALLED) => (None, None, false),
        ShellProbe::NotFound(e) => (None, Some(e), false),
    };

    // 2. 获取远程最新版本（npm 工具在本地领先 latest 时会按预发布通道补查，见
    //    fetch_npm_latest_for_tool / npm_prerelease_tags）
    let local = local_version.as_deref();
    let latest_version = match tool {
        "claude" => {
            fetch_npm_latest_for_tool(&client, "@anthropic-ai/claude-code", tool, local).await
        }
        "codex" => fetch_npm_latest_for_tool(&client, "@openai/codex", tool, local).await,
        "gemini" => fetch_npm_latest_for_tool(&client, "@google/gemini-cli", tool, local).await,
        "grok" => fetch_npm_latest_for_tool(&client, "@xai-official/grok", tool, local).await,
        "opencode" => {
            if let Some(version) =
                fetch_npm_latest_for_tool(&client, "opencode-ai", tool, local).await
            {
                Some(version)
            } else {
                fetch_github_latest_version(&client, "anomalyco/opencode").await
            }
        }
        "openclaw" => fetch_npm_latest_for_tool(&client, "openclaw", tool, local).await,
        "hermes" => fetch_hermes_latest_version(&client, local).await,
        "pi" => {
            fetch_npm_latest_for_tool(&client, "@earendil-works/pi-coding-agent", tool, local).await
        }
        "mcode" => fetch_npm_latest_for_tool(&client, "@minimax-ai/code", tool, local).await,
        "kimi" => fetch_npm_latest_for_tool(&client, "@moonshot-ai/kimi-code", tool, local).await,
        "dsh" => fetch_npm_latest_for_tool(&client, "@deepseek-ai/dsh", tool, local).await,
        "agy" => fetch_agy_latest_version(&client).await,
        "omp" => fetch_npm_latest_for_tool(&client, "@oh-my-pi/pi-coding-agent", tool, local).await,
        _ => None,
    };

    // 3. 有新版本时，看新版本要求的 Node 与这处安装用的对不对得上
    let update_blocked_by_node = match (local, latest_version.as_deref()) {
        (Some(local), Some(latest))
            if compare_semver(local, latest) == Some(std::cmp::Ordering::Less) =>
        {
            update_node_gap(&client, tool, latest).await
        }
        _ => None,
    };

    ToolVersion {
        name: tool.to_string(),
        version: local_version,
        latest_version,
        error: local_error,
        installed_but_broken,
        env_type,
        wsl_distro,
        update_blocked_by_node,
    }
}

/// 该工具在 npm 上的预发布通道 tag(靠前者优先)。仅当本地版本已**严格领先**
/// `latest` 时才会被补查 —— 让主动在抢先通道的用户(如走 Claude Code 的 `next`)
/// 看到与所在通道对齐的"最新版本",同时绝不把稳定通道用户暴露给预发布版。
/// 返回空切片表示该工具只看 `latest`、不补查。
///
/// 为何不通用覆盖所有工具:各家预发布 tag 命名互不统一(codex=alpha/beta/native、
/// gemini=nightly/preview、openclaw=alpha/beta),且 codex 的 beta/native 是
/// `0.1.x` 时间戳式版本、gemini 有误发的 `false` tag —— 这些脏值虽会被
/// `pick_latest_version` 的版本比较挡掉,但维护成本与误报风险不值当,故暂只为
/// Claude Code 启用。
pub(crate) fn npm_prerelease_tags(tool: &str) -> &'static [&'static str] {
    match tool {
        "claude" => &["next"],
        _ => &[],
    }
}

/// 解析 "2.1.156" / "2.1.156-beta.1" → (主版本三段, 预发布段)。无法解析返回 None。
/// 与前端 `src/lib/version.ts` 的 parseVersion 语义对称(跨语言各实现一份)。
/// patch 用 u64 以容纳 codex 的 `0.1.2505172116` 时间戳式版本而不溢出。
pub(crate) fn parse_semver(v: &str) -> Option<([u64; 3], Vec<String>)> {
    let v = v.trim();
    // `+local` 段命中开发构建形态(git describe `238.g<sha>`,或含 canary /
    // nightly / dev 等开发通道标识,如 Hermes main/canary 自报的
    // `0.22.1+238.g<sha>`)时,版本号可能低于最新 release、内容反而更新,与
    // release 不可比 —— 返回 None,调用方据此保守处理。普通 semver build
    // metadata(如 `1.0.0+linux.1`)按 semver 规范忽略后照常比较。
    if let Some((_, local)) = v.split_once('+') {
        if is_dev_local(local) {
            return None;
        }
    }
    let core_and_pre = v.split('+').next().unwrap_or("");
    let (core, pre) = match core_and_pre.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (core_and_pre, None),
    };
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None; // 多于三段,非法
    }
    let pre_segments = pre
        .map(|p| p.split('.').map(|s| s.to_string()).collect())
        .unwrap_or_default();
    Some(([major, minor, patch], pre_segments))
}

/// local version 段的开发构建形态:git describe(`238.g<sha>`)或常见开发通道标识
/// (canary / nightly / dev / dirty / snapshot)。普通 semver build metadata
/// (如 `linux.1`)不命中 —— 与前端 `src/lib/version.ts` 的 DEV_LOCAL_RE 语义对称。
pub(crate) static DEV_LOCAL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^\d+\.g[0-9a-f]{4,}|(^|\.)(canary|nightly|dev|dirty|snapshot)(\.|$)")
        .expect("Invalid dev-local regex")
});

pub(crate) fn is_dev_local(local: &str) -> bool {
    DEV_LOCAL_RE.is_match(local)
}

/// 比较两个版本号(遵循 semver:主版本三段优先;core 相等时有预发布 < 无预发布;
/// 预发布段逐段比 —— 数字段按数值、数字段 < 非数字段、非数字段按 ASCII、前缀相同
/// 则段更多者更大)。任一无法解析返回 None,调用方据此保守处理。
pub(crate) fn compare_semver(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    use std::cmp::Ordering;
    let (ac, ap) = parse_semver(a)?;
    let (bc, bp) = parse_semver(b)?;
    for i in 0..3 {
        match ac[i].cmp(&bc[i]) {
            Ordering::Equal => continue,
            other => return Some(other),
        }
    }
    match (ap.is_empty(), bp.is_empty()) {
        (true, true) => return Some(Ordering::Equal),
        (true, false) => return Some(Ordering::Greater),
        (false, true) => return Some(Ordering::Less),
        (false, false) => {}
    }
    for (x, y) in ap.iter().zip(bp.iter()) {
        let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(xv), Ok(yv)) => xv.cmp(&yv),
            (Ok(_), Err(_)) => Ordering::Less, // 数字段 < 非数字段
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => x.as_str().cmp(y.as_str()),
        };
        if ord != Ordering::Equal {
            return Some(ord);
        }
    }
    Some(ap.len().cmp(&bp.len()))
}

/// 从一次 registry 请求得到的完整 dist-tags 出发,挑选要展示的"最新版本"。
///
/// 规则:默认就是 `latest`;仅当本地版本已**严格领先** `latest`(说明用户主动在
/// 抢先通道)时,才把 `prerelease_tags` 指向的版本纳入比较,取其中能被解析、且
/// 高于 `latest` 的最高者。无法解析或不高于 latest 的脏 tag 一律落选。
pub(crate) fn pick_latest_version(
    dist_tags: &serde_json::Map<String, serde_json::Value>,
    prerelease_tags: &[&str],
    local_version: Option<&str>,
) -> Option<String> {
    use std::cmp::Ordering;
    let latest = dist_tags.get("latest").and_then(|v| v.as_str())?;

    // 本地是否严格领先 latest;任一无法解析则按"未领先"保守处理(只看 latest)。
    let local_ahead = local_version
        .and_then(|local| compare_semver(local, latest))
        .map(|ord| ord == Ordering::Greater)
        .unwrap_or(false);
    if prerelease_tags.is_empty() || !local_ahead {
        return Some(latest.to_string());
    }

    let mut best = latest.to_string();
    for tag in prerelease_tags {
        if let Some(candidate) = dist_tags.get(*tag).and_then(|v| v.as_str()) {
            if compare_semver(candidate, &best) == Some(Ordering::Greater) {
                best = candidate.to_string();
            }
        }
    }
    Some(best)
}

/// npm 包 dist-tags 专用端点的 URL。
///
/// 该端点的响应体就是 dist-tags 对象本身(几十到几千字节);而 `/{package}` 返回的是
/// 含每个历史版本元数据的完整 packument,codex / opencode / openclaw 这类高频发版的包
/// 已有十几到二十几 MB,一次刷新要下几十 MB(#7339)。scoped 包名的 `/` 按 registry
/// 约定转义成 `%2f`。
pub(crate) fn npm_dist_tags_url(package: &str) -> String {
    format!(
        "https://registry.npmjs.org/-/package/{}/dist-tags",
        package.replace('/', "%2f")
    )
}

/// 拉取 npm 包的完整 dist-tags(单次请求即含 latest/next/beta/...)。
///
/// 与 GitHub / PyPI 两条来源一样套 `LATEST_PROBE_TIMEOUT`:取不到就返回 None,由调用方
/// 显示「未知」,而不是沿用全局客户端的 600s 总超时让卡片一直「加载中」。包不存在时
/// 端点返回 404 与一个 JSON 字符串体,解析成 Map 失败,同样落到 None。
pub(crate) async fn fetch_npm_dist_tags(
    client: &reqwest::Client,
    package: &str,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    let resp = client
        .get(npm_dist_tags_url(package))
        .timeout(LATEST_PROBE_TIMEOUT)
        .send()
        .await
        .ok()?;
    resp.json::<serde_json::Map<String, serde_json::Value>>()
        .await
        .ok()
}

/// 查询某 npm 工具要展示的"最新版本":取 `latest`,并在本地版本领先时按工具的
/// 预发布通道(见 `npm_prerelease_tags`)补查 —— 复用同一次 registry 响应,无额外请求。
pub(crate) async fn fetch_npm_latest_for_tool(
    client: &reqwest::Client,
    package: &str,
    tool: &str,
    local_version: Option<&str>,
) -> Option<String> {
    let dist_tags = fetch_npm_dist_tags(client, package).await?;
    pick_latest_version(&dist_tags, npm_prerelease_tags(tool), local_version)
}

/// 版本探测请求的请求级超时。全局客户端是给代理转发用的（总超时 600s / 连接 30s），
/// 探测 latest 若沿用它，api.github.com / pypi.org 被阻断或握手后挂起时，Hermes 卡片
/// 与「刷新 / 全部升级」按钮会一直等；探测拿不到就退到下一来源或显示未知即可。
pub(crate) const LATEST_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Helper function to fetch latest version from GitHub releases
/// Antigravity CLI 的 installer 和自升级共用的发布清单：`manifests/<平台>.json` 的 `version`。
pub(crate) async fn fetch_agy_latest_version(client: &reqwest::Client) -> Option<String> {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        "linux" => "linux",
        "windows" => "windows",
        _ => return None,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        _ => return None,
    };
    let url = format!(
        "https://antigravity-cli-auto-updater-974169037036.us-central1.run.app/manifests/{os}_{arch}.json"
    );
    let resp = client
        .get(&url)
        .timeout(LATEST_PROBE_TIMEOUT)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    let json = resp.json::<serde_json::Value>().await.ok()?;
    json.get("version")
        .and_then(|v| v.as_str())
        .and_then(release_display_version)
}

pub(crate) async fn fetch_github_latest_version(
    client: &reqwest::Client,
    repo: &str,
) -> Option<String> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let resp = client
        .get(&url)
        .header("User-Agent", "cc-switch")
        .header("Accept", "application/vnd.github+json")
        .timeout(LATEST_PROBE_TIMEOUT)
        .send()
        .await
        .ok()?;
    let json = resp.json::<serde_json::Value>().await.ok()?;
    github_release_version_from_json(&json)
}

/// 从 GitHub latest release 的 JSON 中提取展示用版本号。
///
/// 优先取 release `name` 里能解析为语义版本的数字，其次才是 `tag_name`（去 `v` 前缀）：
/// Hermes 的 tag 是日历式（`v2026.8.31`），CLI 自报的语义版本 `0.21.0` 只出现在
/// name（"Hermes Agent v0.21.0 (v2026.8.31)"）里。两条路都经 `release_display_version`
/// 过滤——日历式数字若被当成版本号，前端三段解析会成功（2026 > 0）并永久判定
/// "有可用更新"，点升级后版本不变又触发"版本未变"误报；拿不到语义版本宁可返回
/// None，让调用方退到别的来源。限流（403 JSON 只有 message）同样落到 None。
pub(crate) fn github_release_version_from_json(json: &serde_json::Value) -> Option<String> {
    let from_name = json
        .get("name")
        .and_then(|v| v.as_str())
        .and_then(|name| release_display_version(&extract_version(name)));
    from_name.or_else(|| {
        json.get("tag_name")
            .and_then(|v| v.as_str())
            .and_then(|tag| release_display_version(tag.strip_prefix('v').unwrap_or(tag)))
    })
}

/// 只接受能按语义版本解析、且首段不像年份（< 1000）的字符串作展示版本。
pub(crate) fn release_display_version(candidate: &str) -> Option<String> {
    match parse_semver(candidate) {
        Some(([major, ..], _)) if major < 1000 => Some(candidate.to_string()),
        _ => None,
    }
}

/// 兜底来源给出的 latest 若已被本地版本严格超过，则不展示（返回 None）——
/// 展示一个比当前还旧的"最新版本"只会复现用户报告的"最新 < 当前"矛盾。
/// 任一侧无法解析时按"未领先"保守处理，照常展示。
pub(crate) fn drop_latest_behind_local(
    latest: Option<String>,
    local_version: Option<&str>,
) -> Option<String> {
    let latest = latest?;
    let local_leads = local_version
        .and_then(|local| compare_semver(local, &latest))
        .is_some_and(|ord| ord == std::cmp::Ordering::Greater);
    (!local_leads).then_some(latest)
}

/// Hermes 的「最新版本」：GitHub Releases 为主，PyPI 兜底。
///
/// cc-switch 安装/升级 Hermes 走的是官方 install.sh（`git clone` main 分支）与
/// `hermes update`（`git pull`），整条链路与 PyPI 无关；而 PyPI 的 `hermes-agent`
/// 自 0.19.0（2026-07-20）起停更，上游只在 GitHub Releases 发版
/// （#6475 / #6618 / #7033：「最新版本」长期停在 0.19.0、比当前还旧、升级按钮不出现）。
/// 仅当 GitHub 不可达或被限流时才退到 PyPI，且该值已被本地超过时不展示，宁可显示未知。
pub(crate) async fn fetch_hermes_latest_version(
    client: &reqwest::Client,
    local_version: Option<&str>,
) -> Option<String> {
    if let Some(version) = fetch_github_latest_version(client, "NousResearch/hermes-agent").await {
        return Some(version);
    }
    let pypi = fetch_pypi_latest_version(client, "hermes-agent").await;
    drop_latest_behind_local(pypi, local_version)
}

/// Helper function to fetch latest version from PyPI
pub(crate) async fn fetch_pypi_latest_version(
    client: &reqwest::Client,
    package: &str,
) -> Option<String> {
    let url = format!("https://pypi.org/pypi/{package}/json");
    match client.get(&url).timeout(LATEST_PROBE_TIMEOUT).send().await {
        Ok(resp) => {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                json.get("info")
                    .and_then(|info| info.get("version"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

// ==================== Node 升级门槛 ====================

/// 新版本要求的 Node 和升级会用到的 Node 对不上。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct NodeRequirementGap {
    /// 目标版本的 `engines.node`，原样展示。
    required: String,
    /// 升级会用到的 node 的版本（不带 `v`）。
    current: String,
}

/// 升级到 `target` 会不会被 Node 版本挡住。只看 npm 包、且默认那处由 node 运行的安装。
pub async fn update_node_gap(
    client: &reqwest::Client,
    tool: &str,
    target: &str,
) -> Option<NodeRequirementGap> {
    let package = npm_package_for(tool)?;
    // MiniMax Code 的官方 installer 在系统 Node 不达标时会自带一份 Node。
    if tool == "mcode" {
        return None;
    }
    #[cfg(target_os = "windows")]
    if wsl_distro_for_tool(tool).is_some() {
        return None;
    }
    let owned = tool.to_string();
    let current = tokio::task::spawn_blocking(move || update_node_version(&owned))
        .await
        .ok()??;
    let required = fetch_engines_node(client, package, target).await?;
    match node_range_satisfied(&required, &current) {
        Some(false) => Some(NodeRequirementGap { required, current }),
        _ => None,
    }
}

/// 执行升级前的检查：目标是 `latest`（`npm i -g <pkg>@latest` 与各家 self-update 的稳定通道）。
pub async fn ensure_update_node(tool: &str) -> Result<(), String> {
    let Some(package) = npm_package_for(tool) else {
        return Ok(());
    };
    let client = crate::proxy::http_client::get();
    let Some(latest) = fetch_npm_dist_tags(&client, package)
        .await
        .and_then(|tags| tags.get("latest")?.as_str().map(str::to_string))
    else {
        return Ok(());
    };
    match update_node_gap(&client, tool, &latest).await {
        Some(gap) => {
            let name = tool_display_name(tool);
            Err(format!(
                "{name} {latest} requires Node {}, but this installation runs on Node {}. Upgrade Node first, then update {name}.",
                gap.required, gap.current
            ))
        }
        None => Ok(()),
    }
}

/// 升级默认那处时用到的 node 版本：与锚定升级同源，先找安装旁边的 node，再按 PATH 找。
/// 默认那处不由 node 运行（原生二进制、Homebrew 管、bun）时返回 None。
fn update_node_version(tool: &str) -> Option<String> {
    let installs = enumerate_tool_installations(tool);
    let inst = default_install(&installs)?;
    let real = inst.real.to_string_lossy();
    if !real.replace('\\', "/").contains("/node_modules/") || inst.source == "bun" {
        return None;
    }
    #[cfg(not(target_os = "windows"))]
    if is_brew_managed_path(&real) {
        return None;
    }

    let sibling = Path::new(&inst.path)
        .parent()
        .map(|dir| {
            dir.join(if cfg!(target_os = "windows") {
                "node.exe"
            } else {
                "node"
            })
        })
        .filter(|node| node.is_file());
    let node = match sibling {
        Some(node) => node,
        None => resolve_path_default(
            "node",
            CommandDeadline::from_timeout(Some(INSTALL_PROBE_TIMEOUT)),
        )
        .ok()
        .flatten()?,
    };
    let dir = node.parent()?;
    #[cfg(not(target_os = "windows"))]
    let path = prepend_search_dir_to_path(dir, &effective_path_os().unwrap_or_default());
    #[cfg(target_os = "windows")]
    let path = format!("{};{}", dir.display(), effective_path_string());
    let output = run_probe_version_command(&node, &path).ok()?;
    if !output.status.success() {
        return None;
    }
    let text = decode_command_output(&output.stdout);
    VERSION_RE.find(&text).map(|m| m.as_str().to_string())
}

/// 读 `<package>@<version>` 的 `engines.node`。单个版本的元数据，不是整份 packument。
async fn fetch_engines_node(
    client: &reqwest::Client,
    package: &str,
    version: &str,
) -> Option<String> {
    let url = format!(
        "https://registry.npmjs.org/{}/{version}",
        package.replace('/', "%2f")
    );
    let json = client
        .get(url)
        .timeout(LATEST_PROBE_TIMEOUT)
        .send()
        .await
        .ok()?
        .json::<serde_json::Value>()
        .await
        .ok()?;
    let range = json.get("engines")?.get("node")?.as_str()?.trim();
    (!range.is_empty()).then(|| range.to_string())
}

type Core = [u64; 3];

#[derive(Debug, Clone, Copy)]
enum Bound {
    Ge(Core),
    Gt(Core),
    Le(Core),
    Lt(Core),
}

impl Bound {
    fn admits(self, v: Core) -> bool {
        match self {
            Bound::Ge(b) => v >= b,
            Bound::Gt(b) => v > b,
            Bound::Le(b) => v <= b,
            Bound::Lt(b) => v < b,
        }
    }
}

/// `version` 是否满足 npm 版本范围 `range`。支持比较符、`^`、`~`、x 通配、`a - b` 与 `||`；
/// 认不出的写法返回 None，调用方按「不确定」放行。
fn node_range_satisfied(range: &str, version: &str) -> Option<bool> {
    let (version, _) = parse_semver(version.trim().trim_start_matches('v'))?;
    let mut unknown = false;
    for set in range.split("||") {
        match comparator_set_admits(set, version) {
            Some(true) => return Some(true),
            Some(false) => {}
            None => unknown = true,
        }
    }
    (!unknown).then_some(false)
}

fn comparator_set_admits(set: &str, version: Core) -> Option<bool> {
    let tokens: Vec<&str> = set.split_whitespace().collect();
    let mut bounds = Vec::new();
    if let [low, "-", high] = tokens.as_slice() {
        bounds.extend(comparator(">=", &parse_partial(low)?)?);
        bounds.extend(comparator("<=", &parse_partial(high)?)?);
    } else {
        // `>= 24` 这种比较符与版本之间带空格的写法也合法。
        let mut pending: Option<&str> = None;
        for token in tokens {
            let (op, rest) = split_operator(token);
            let op = match pending.take() {
                Some(_) if !op.is_empty() => return None,
                Some(pending) => pending,
                None => op,
            };
            if rest.is_empty() {
                pending = Some(op);
                continue;
            }
            bounds.extend(comparator(op, &parse_partial(rest)?)?);
        }
        if pending.is_some() {
            return None;
        }
    }
    Some(bounds.into_iter().all(|bound| bound.admits(version)))
}

fn split_operator(token: &str) -> (&str, &str) {
    for op in [">=", "<=", ">", "<", "=", "^", "~"] {
        if let Some(rest) = token.strip_prefix(op) {
            return (op, rest);
        }
    }
    ("", token)
}

/// `24` / `24.16` / `24.16.0` / `24.x` / `*` → 写出来的那几段；`v` 前缀与预发布段忽略。
fn parse_partial(raw: &str) -> Option<Vec<u64>> {
    let core = raw.trim_start_matches('v').split(['-', '+']).next()?;
    let mut parts = Vec::new();
    for segment in core.split('.') {
        if matches!(segment, "x" | "X" | "*") {
            break;
        }
        parts.push(segment.parse().ok()?);
    }
    (parts.len() <= 3).then_some(parts)
}

fn comparator(op: &str, parts: &[u64]) -> Option<Vec<Bound>> {
    let at = |i: usize| parts.get(i).copied().unwrap_or(0);
    let floor = [at(0), at(1), at(2)];
    // 最后写出的那段 +1、其后归零：`24` → 25.0.0，`24.16` → 24.17.0。
    let next = || {
        let mut core = floor;
        core[parts.len() - 1] += 1;
        for slot in core.iter_mut().skip(parts.len()) {
            *slot = 0;
        }
        core
    };
    let exact = parts.len() == 3;
    if parts.is_empty() {
        return matches!(op, "" | "=" | ">=" | "<=" | "^" | "~").then(Vec::new);
    }
    Some(match op {
        "" | "=" if exact => vec![Bound::Ge(floor), Bound::Le(floor)],
        "" | "=" => vec![Bound::Ge(floor), Bound::Lt(next())],
        ">=" => vec![Bound::Ge(floor)],
        ">" if exact => vec![Bound::Gt(floor)],
        ">" => vec![Bound::Ge(next())],
        "<" => vec![Bound::Lt(floor)],
        "<=" if exact => vec![Bound::Le(floor)],
        "<=" => vec![Bound::Lt(next())],
        "~" if parts.len() == 1 => vec![Bound::Ge(floor), Bound::Lt([at(0) + 1, 0, 0])],
        "~" => vec![Bound::Ge(floor), Bound::Lt([at(0), at(1) + 1, 0])],
        "^" => {
            let upper = if at(0) > 0 || parts.len() == 1 {
                [at(0) + 1, 0, 0]
            } else if at(1) > 0 || parts.len() == 2 {
                [0, at(1) + 1, 0]
            } else {
                [0, 0, at(2) + 1]
            };
            vec![Bound::Ge(floor), Bound::Lt(upper)]
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openclaw_range_rejects_node_22_and_admits_24_and_26() {
        let range = ">=24.16.0 <25 || >=26.1.0";
        assert_eq!(node_range_satisfied(range, "22.23.2"), Some(false));
        assert_eq!(node_range_satisfied(range, "v24.15.9"), Some(false));
        assert_eq!(node_range_satisfied(range, "24.16.0"), Some(true));
        assert_eq!(node_range_satisfied(range, "25.0.0"), Some(false));
        assert_eq!(node_range_satisfied(range, "26.1.0"), Some(true));
    }

    #[test]
    fn common_engine_ranges() {
        assert_eq!(node_range_satisfied(">=20", "20.0.0"), Some(true));
        assert_eq!(node_range_satisfied(">=20", "18.20.8"), Some(false));
        assert_eq!(node_range_satisfied(">= 22.0.0", "22.1.0"), Some(true));
        assert_eq!(
            node_range_satisfied("^20.19.0 || >=22.12.0", "21.0.0"),
            Some(false)
        );
        assert_eq!(
            node_range_satisfied("^20.19.0 || >=22.12.0", "20.19.4"),
            Some(true)
        );
        assert_eq!(node_range_satisfied("~18.17", "18.18.0"), Some(false));
        assert_eq!(node_range_satisfied("18.x", "18.20.8"), Some(true));
        assert_eq!(node_range_satisfied("18 - 20", "20.9.0"), Some(true));
        assert_eq!(node_range_satisfied("18 - 20", "21.0.0"), Some(false));
        assert_eq!(node_range_satisfied(">18", "18.9.0"), Some(false));
        assert_eq!(node_range_satisfied("<=20", "20.9.0"), Some(true));
        assert_eq!(node_range_satisfied("*", "4.0.0"), Some(true));
        assert_eq!(node_range_satisfied("^0.2.3", "0.3.0"), Some(false));
    }

    #[test]
    fn unknown_syntax_never_blocks() {
        assert_eq!(node_range_satisfied("node >= 20", "18.0.0"), None);
        assert_eq!(node_range_satisfied(">=", "18.0.0"), None);
        // 一侧认不出、另一侧满足时仍算满足。
        assert_eq!(node_range_satisfied("lts/* || >=18", "20.0.0"), Some(true));
        assert_eq!(node_range_satisfied(">=20", "not-a-version"), None);
    }
}
