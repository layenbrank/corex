//! 精简的 GitHub Releases 客户端，刚好覆盖 `corex update` 的需要。
//!
//! 只用三个端点：`GET /releases/latest`、`GET /releases`，以及 release 产物本身。
//! 产物下载走
//! `browser_download_url`，它是 CDN 跳转，
//! 不消耗核心 REST 配额。

use crate::error::{Error, Result, describe};
use crate::proxy;
use corex_core::{UpdateChannel, UpdateConfig};
use semver::Version;
use serde::Deserialize;
use std::time::Duration;

/// GitHub 要求每个请求都带的 `User-Agent`。
const USER_AGENT: &str = concat!("corex-updater/", env!("CARGO_PKG_VERSION"));

/// 缓冲 API 响应的安全上限。
const MAX_API_BYTES: u64 = 8 * 1024 * 1024;

/// 缓冲 release 产物的安全上限。
const MAX_ASSET_BYTES: u64 = 256 * 1024 * 1024;

/// 校验清单的安全上限。
const MAX_SUMS_BYTES: u64 = 1024 * 1024;

/// 每次查找扫描的 release 数。
///
/// 选择在客户端做，所以这个值必须明显大于同一安装两次升级之间
/// 发布出来的 release 数量。
const RELEASE_PAGE: u32 = 30;

/// 产物传输放弃前的尝试次数。
const DOWNLOAD_ATTEMPTS: usize = 3;

/// 两次传输尝试之间的基础退避，按尝试次数倍增。
const RETRY_BACKOFF: Duration = Duration::from_millis(700);

/// GitHub API 返回的一个 release 产物。
#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    /// 上传的文件名。
    pub name: String,
    /// 字节大小。
    #[serde(default)]
    pub size: u64,
    /// GitHub 发布的 `sha256:<hex>` 摘要。
    ///
    /// 这只是一道完整性检查：托管方会在产物被替换时重新算它，
    /// 所以它说明不了真实性。
    #[serde(default)]
    pub digest: Option<String>,
    /// CDN 下载地址（不消耗 REST 配额）。
    pub browser_download_url: String,
    /// 上传内容的 MIME 类型。
    #[serde(default)]
    pub content_type: Option<String>,
}

/// 已解析好的 release。
#[derive(Debug, Clone)]
pub struct Release {
    /// 发布的原始 tag，如 `v6.0.2` 或 `v6.0.2-beta.1`。
    pub tag: String,
    /// 解析出的语义化版本。
    pub version: Version,
    /// 面向人的 release 页面。
    pub html_url: String,
    /// 全部已上传的产物。
    pub assets: Vec<Asset>,
}

impl Release {
    /// 名为 `name` 的产物（若该 release 有）。
    pub fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(name))
    }
}

/// 一次条件式 release 查找的结果。
#[derive(Debug, Clone)]
pub enum Lookup {
    /// `304 Not Modified`——缓存的 tag 仍是通道头部。
    NotModified,
    /// 通道头部。
    Found {
        /// 解析好的 release。
        release: Box<Release>,
        /// 留给下次条件请求的 `ETag`。
        etag: Option<String>,
    },
}

/// 把 `v6.0.2` / `6.0.2-beta.1` 解析成 [`Version`]。
pub fn parse_tag(tag: &str) -> Option<Version> {
    Version::parse(tag.trim().strip_prefix('v').unwrap_or(tag.trim())).ok()
}

/// GitHub release 原始负载；只声明我们真正用到的字段。
#[derive(Debug, Clone, Deserialize)]
struct RawRelease {
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

/// GitHub Releases API 客户端。
pub struct Client {
    http: reqwest::Client,
    /// API 根地址，不带尾部斜杠，如 `https://api.github.com`。
    api_base: String,
    /// `owner/repo`。
    repo: String,
    /// 要发送的 token，已对照 `api_base` 审查过。
    token: Option<String>,
    /// token 可能来自的环境变量名，用于限流提示。
    token_env: String,
}

impl Client {
    /// 从 `[update]` 构造客户端。
    pub fn new(config: &UpdateConfig) -> Result<Self> {
        let repo = normalize_repo(&config.repository)?;
        let api_base = config
            .api_base_url
            .as_deref()
            .map(|s| s.trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "https://api.github.com".to_string());
        let token_env = config.token_env.clone().unwrap_or_default();
        let token = resolve_token(&token_env, config.api_base_url.is_some(), &api_base);
        let per_request = Duration::from_secs(config.timeout.clamp(1, 600));
        let mut builder = reqwest::Client::builder()
            // 刻意不用 `timeout()`：它会卡住整个请求，
            // 于是大产物在慢链路上会中途死掉。改为约束连接建立与单次读取停顿，
            // 这样真正卡死的传输仍会失败，但不限制总传输时长。
            // 真正卡死的传输仍会失败，但不限制总传输时长。
            .connect_timeout(per_request)
            .read_timeout(per_request)
            .user_agent(USER_AGENT);
        // 只在环境变量沉默时才查平台设置，
        // 使显式设置的 HTTPS_PROXY 总是优先于 Windows 系统代理。
        if config.use_system_proxy
            && !proxy_env_set()
            && let Some(url) = proxy::system()
        {
            match reqwest::Proxy::all(&url) {
                Ok(proxy) => {
                    // 不记 URL：它可能内嵌凭据。
                    tracing::debug!("使用 Windows 系统代理");
                    builder = builder.proxy(proxy);
                }
                Err(err) => {
                    tracing::warn!(error = %describe(&err), "系统代理地址无效，已忽略");
                }
            }
        }
        let http = builder
            .build()
            .map_err(|e| Error::HttpTransport(describe(&e)))?;
        Ok(Self {
            http,
            api_base,
            repo,
            token,
            token_env,
        })
    }

    /// `channel` 目前指向的 release。
    ///
    /// 每个通道都读同一页 releases，并用同一条选择规则（[`select_head`]）。
    /// 刻意不用 GitHub 的 `releases/latest` 端点：它排除预发布版，
    /// 而且按提交日期而非版本排序，于是一个回移的补丁会被当成
    /// “latest”。一个端点 + 一条规则，比每个通道一个端点更小
    /// 也更可预测。
    /// 而不是每通道一套。
    ///
    /// `etag` 让请求变成条件式的；`304` 会复用上次的答案，
    /// 不用重新下载这一页。
    pub async fn head(&self, channel: UpdateChannel, etag: Option<&str>) -> Result<Lookup> {
        let url = format!(
            "{}/repos/{}/releases?per_page={RELEASE_PAGE}",
            self.api_base, self.repo
        );
        let resp = self.get(&url, etag, "application/vnd.github+json").await?;
        if resp.status().as_u16() == 304 {
            return Ok(Lookup::NotModified);
        }
        let next_etag = response_etag(resp.headers());
        let raw: Vec<RawRelease> = self.json(resp, &url).await?;
        select_head(raw, channel)
            .map(|release| Lookup::Found {
                release: Box::new(release),
                etag: next_etag,
            })
            .ok_or(Error::NoRelease {
                channel: channel.as_str(),
            })
    }

    /// 显式 `--version` 时走 `GET /repos/{repo}/releases/tags/{tag}`。
    pub async fn by_version(&self, version: &Version) -> Result<Release> {
        let tag = format!("v{version}");
        let url = format!(
            "{}/repos/{}/releases/tags/{}",
            self.api_base, self.repo, tag
        );
        let resp = self.get(&url, None, "application/vnd.github+json").await?;
        let raw: RawRelease = self.json(resp, &url).await?;
        // tag 是由 `Version` 构造出来的，版本已经知道；
        // 再解析一遍响应里的 tag 只会多一个失败点。
        Ok(Release {
            tag: raw.tag_name,
            version: version.clone(),
            html_url: raw.html_url,
            assets: raw.assets,
        })
    }

    /// 小型文本产物，用于 `SHA256SUMS.txt` 清单。
    pub async fn fetch_text(&self, url: &str) -> Result<String> {
        let resp = self.get(url, None, "text/plain").await?;
        if resp
            .content_length()
            .is_some_and(|len| len > MAX_SUMS_BYTES)
        {
            return Err(Error::DownloadTooLarge(MAX_SUMS_BYTES));
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| Error::HttpTransport(describe(&e)))?;
        if bytes.len() as u64 > MAX_SUMS_BYTES {
            return Err(Error::DownloadTooLarge(bytes.len() as u64));
        }
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// 把产物流式读进内存，过程中上报进度。
    ///
    /// 传输失败会重试；之所以安全，正是因为 body 先整个缓冲、
    /// 之后再校验和：截断的读取永远不会被当成完整文件。release 压缩包约 20 MB，
    /// 缓冲还能把校验压成“先哈希再一次写盘”。
    /// 缓冲还能把校验压成“先哈希再一次写盘”。
    pub async fn download(
        &self,
        url: &str,
        on_progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<Vec<u8>> {
        let mut attempt = 0usize;
        loop {
            // 让进度条重开，重试时进度就不会往回跳。
            on_progress(0, None);
            match self.transfer(url, on_progress).await {
                Ok(bytes) => return Ok(bytes),
                Err(err) => {
                    if !matches!(err, Error::HttpTransport(_)) || attempt + 1 >= DOWNLOAD_ATTEMPTS {
                        return Err(err);
                    }
                    attempt += 1;
                    tracing::warn!(attempt, error = %err, "下载中断，重试");
                    tokio::time::sleep(RETRY_BACKOFF * attempt as u32).await;
                }
            }
        }
    }

    /// 一次传输尝试。
    async fn transfer(
        &self,
        url: &str,
        on_progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<Vec<u8>> {
        let mut response = self.get(url, None, "application/octet-stream").await?;
        let total = response.content_length();
        if let Some(len) = total.filter(|len| *len > MAX_ASSET_BYTES) {
            return Err(Error::DownloadTooLarge(len));
        }
        let mut buf: Vec<u8> = Vec::with_capacity(total.unwrap_or(0).min(MAX_API_BYTES) as usize);
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| Error::HttpTransport(describe(&e)))?
        {
            buf.extend_from_slice(&chunk);
            if buf.len() as u64 > MAX_ASSET_BYTES {
                return Err(Error::DownloadTooLarge(buf.len() as u64));
            }
            on_progress(buf.len() as u64, total);
        }
        Ok(buf)
    }

    /// `GET url`，并把 GitHub 的限流信号映射成 [`Error::RateLimited`]。
    async fn get(&self, url: &str, etag: Option<&str>, accept: &str) -> Result<reqwest::Response> {
        let mut request = self.http.get(url).header(reqwest::header::ACCEPT, accept);
        if let Some(tag) = etag {
            request = request.header(reqwest::header::IF_NONE_MATCH, tag);
        }
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request.send().await.map_err(|e| transport_error(&e))?;
        let status = response.status().as_u16();
        if response.status().is_success() || status == 304 {
            return Ok(response);
        }
        Err(classify(status, response.headers(), url, &self.token_env))
    }

    /// 解码 JSON body，上限为 [`MAX_API_BYTES`]。
    async fn json<T: serde::de::DeserializeOwned>(
        &self,
        response: reqwest::Response,
        url: &str,
    ) -> Result<T> {
        let bytes = response
            .bytes()
            .await
            .map_err(|e| Error::HttpTransport(describe(&e)))?;
        if bytes.len() as u64 > MAX_API_BYTES {
            return Err(Error::DownloadTooLarge(bytes.len() as u64));
        }
        serde_json::from_slice(&bytes).map_err(|e| Error::Api {
            url: url.to_string(),
            why: e.to_string(),
        })
    }
}

/// `ETag` 响应头（若服务器发了）。
fn response_etag(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get(reqwest::header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

/// 描述传输失败，并补上 Windows 用户需要的代理提示。
///
/// 单纯的连接超时是用户最先撞上的症状，而它通常的成因——
/// 代理配在 Windows 设置里而不是环境变量里——
/// 底层消息本身说不出来。
fn transport_error(err: &reqwest::Error) -> Error {
    let mut text = describe(err);
    if err.is_connect() && !proxy_env_set() && crate::proxy::system().is_none() {
        text.push_str("。若本机通过代理上网，请设置 HTTPS_PROXY");
    }
    Error::HttpTransport(text)
}

/// 是否设置了任何标准的代理环境变量。
///
/// `reqwest` 自己会读这些变量，所以显式设置必须优先于
/// 平台配置。
fn proxy_env_set() -> bool {
    const NAMES: [&str; 6] = [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ];
    NAMES
        .iter()
        .any(|name| std::env::var(name).is_ok_and(|value| !value.trim().is_empty()))
}

/// `raw` 中属于 `channel` 的最高版本 release。
///
/// 一条规则服务所有通道，所以 `/releases` 不需要按通道分别处理：
/// 草稿永不算数，UI 的 `prerelease` 开关必须与 tag 一致（打错标记的 release
/// 两个通道都进不去），而 tag 后缀决定通道（`v6.0.2` → `stable`、
/// `v6.0.2-rc.1` → `rc`）。不是 semver 的 tag 会被跳过，
/// 而不是让查找失败，于是仓库里一个无关的 tag 不会弄坏更新。
/// 仓库里一个无关的 tag 不会弄坏更新。
fn select_head(raw: Vec<RawRelease>, channel: UpdateChannel) -> Option<Release> {
    raw.into_iter()
        .filter(|entry| {
            !entry.draft
                && entry.prerelease == channel.is_prerelease()
                && channel.accepts_tag(&entry.tag_name)
        })
        .filter_map(to_release)
        .max_by(|a, b| a.version.cmp(&b.version))
}

/// 把原始负载转换过来；tag 不是 semver 时返回 `None`。
fn to_release(raw: RawRelease) -> Option<Release> {
    Some(Release {
        version: parse_tag(&raw.tag_name)?,
        tag: raw.tag_name,
        html_url: raw.html_url,
        assets: raw.assets,
    })
}

impl std::fmt::Debug for Client {
    /// 隐去 token，打印客户端时不会泄露 CI 凭据。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("api_base", &self.api_base)
            .field("repo", &self.repo)
            .field("token", &self.token.as_ref().map(|_| "<redacted>"))
            .field("token_env", &self.token_env)
            .finish()
    }
}

/// 把非成功状态码映射成尽可能具体的错误。
///
/// 对齐 GitHub 文档里的信号：`429` 永远是限流，而 `403` 只有在剩余配额为零或
/// 服务器发了 `Retry-After`（二级限流的形态）时才算限流。裸的 `403` 是
/// 凭据问题，保持 [`Error::Http`]。
/// 凭据问题，保持 [`Error::Http`]。
fn classify(
    status: u16,
    headers: &reqwest::header::HeaderMap,
    url: &str,
    token_env: &str,
) -> Error {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
    };
    let retry_after = header("retry-after");
    let exhausted = header("x-ratelimit-remaining") == Some("0");
    let limited = |reason: &str| Error::RateLimited {
        why: match retry_after {
            Some(secs) => format!("{reason}，Retry-After {secs}s"),
            None => reason.to_string(),
        },
        token_env: if token_env.is_empty() {
            "COREX_GITHUB_TOKEN".to_string()
        } else {
            token_env.to_string()
        },
    };
    match status {
        404 => Error::ReleaseNotFound {
            url: url.to_string(),
        },
        429 => limited("HTTP 429"),
        403 if exhausted => limited("HTTP 403，配额已用尽"),
        403 if retry_after.is_some() => limited("HTTP 403"),
        _ => Error::Http {
            status,
            url: url.to_string(),
        },
    }
}

/// 校验 `owner/repo` 形式的 slug。
fn normalize_repo(slug: &str) -> Result<String> {
    let slug = slug.trim();
    let (owner, name) = slug
        .split_once('/')
        .ok_or_else(|| Error::InvalidRepository(slug.to_string()))?;
    if owner.is_empty() || name.is_empty() || name.contains('/') {
        return Err(Error::InvalidRepository(slug.to_string()));
    }
    Ok(format!("{owner}/{name}"))
}

/// 依次从 `token_env`、`GH_TOKEN`、`GITHUB_TOKEN` 解析 API token。
///
/// CI 环境里现成的凭据只发给 GitHub 本身、回环镜像，
/// 或者运维通过 `api_base_url` 显式指定的主机——
/// `self_update` 与 `axoupdater` 都提醒过的那个坑。
fn resolve_token(token_env: &str, explicit_host: bool, api_base: &str) -> Option<String> {
    if !(explicit_host || is_canonical_host(api_base)) {
        return None;
    }
    let mut names: Vec<&str> = Vec::new();
    if !token_env.is_empty() {
        names.push(token_env);
    }
    names.extend(["GH_TOKEN", "GITHUB_TOKEN"]);
    names.into_iter().find_map(non_empty_env)
}

/// 从环境读 `name`，空值视作未设置。
fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// `api_base` 是 GitHub 本身还是本地镜像。
fn is_canonical_host(api_base: &str) -> bool {
    let rest = api_base.split("://").nth(1).unwrap_or(api_base);
    let authority = rest.split('/').next().unwrap_or(rest);
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = if let Some(inner) = host.strip_prefix('[') {
        inner.split(']').next().unwrap_or(inner)
    } else {
        host.split(':').next().unwrap_or(host)
    };
    matches!(
        host.to_ascii_lowercase().as_str(),
        "api.github.com" | "localhost" | "127.0.0.1" | "::1"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tags_with_and_without_prefix() {
        assert_eq!(parse_tag("v6.0.2").unwrap(), Version::new(6, 0, 2));
        assert_eq!(parse_tag("6.0.2").unwrap(), Version::new(6, 0, 2));
        assert_eq!(
            parse_tag("v6.0.2-beta.1").unwrap(),
            Version::parse("6.0.2-beta.1").unwrap()
        );
        assert!(parse_tag("not-a-version").is_none());
    }

    /// 按 API 实际发送的样子构造原始负载。
    fn raw(tag: &str, prerelease: bool) -> RawRelease {
        RawRelease {
            tag_name: tag.to_string(),
            html_url: format!("https://example.invalid/{tag}"),
            draft: false,
            prerelease,
            assets: Vec::new(),
        }
    }

    #[test]
    fn stable_channel_takes_the_highest_stable_tag() {
        let page = vec![
            raw("v6.0.0", false),
            raw("v6.0.2", false),
            raw("v6.0.3-rc.1", true),
            raw("v6.0.1", false),
        ];
        let head = select_head(page, UpdateChannel::Stable).expect("head");
        assert_eq!(head.tag, "v6.0.2");
    }

    #[test]
    fn prerelease_channel_takes_its_own_highest_tag() {
        let page = vec![
            raw("v6.0.2", false),
            raw("v6.0.3-alpha.1", true),
            raw("v6.0.3-beta.2", true),
            raw("v6.0.3-rc.1", true),
        ];
        let beta = select_head(page.clone(), UpdateChannel::Beta).expect("beta");
        assert_eq!(beta.tag, "v6.0.3-beta.2");
        let rc = select_head(page, UpdateChannel::Rc).expect("rc");
        assert_eq!(rc.tag, "v6.0.3-rc.1");
    }

    #[test]
    fn selection_ignores_drafts_mistagged_releases_and_non_semver_tags() {
        let page = vec![
            raw("v6.0.9", false),      // draft
            raw("v6.0.8", true),       // tagged stable but flagged prerelease
            raw("v9.9.9-rc.1", false), // tagged rc but flagged stable
            raw("nightly", false),     // not semver
            raw("v6.0.1", false),
            raw("v6.0.2", false),
        ];
        let mut page = page;
        page[0].draft = true;
        let head = select_head(page, UpdateChannel::Stable).expect("head");
        assert_eq!(head.tag, "v6.0.2");
    }

    #[test]
    fn a_channel_with_no_matching_release_selects_nothing() {
        let page = vec![raw("v6.0.2", false), raw("v6.0.3-beta.1", true)];
        assert!(select_head(page.clone(), UpdateChannel::Rc).is_none());
        assert!(select_head(page, UpdateChannel::Alpha).is_none());
    }

    #[test]
    fn accepts_only_owner_repo_slugs() {
        assert_eq!(
            normalize_repo(" layenbrank/corex ").unwrap(),
            "layenbrank/corex"
        );
        assert!(normalize_repo("layenbrank").is_err());
        assert!(normalize_repo("layenbrank/").is_err());
        assert!(normalize_repo("/corex").is_err());
        assert!(normalize_repo("a/b/c").is_err());
    }

    #[test]
    fn recognises_canonical_hosts() {
        assert!(is_canonical_host("https://api.github.com"));
        assert!(is_canonical_host("https://api.github.com/"));
        assert!(is_canonical_host("http://localhost:8080"));
        assert!(is_canonical_host("http://127.0.0.1:9999/api/v3"));
        assert!(!is_canonical_host("https://github.example.com/api/v3"));
    }

    #[test]
    fn rate_limit_shapes_are_classified() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ratelimit-remaining", "0".parse().unwrap());
        let err = classify(
            403,
            &headers,
            "https://api.github.com/x",
            "COREX_GITHUB_TOKEN",
        );
        assert!(matches!(err, Error::RateLimited { .. }), "{err}");

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, "30".parse().unwrap());
        let err = classify(403, &headers, "https://api.github.com/x", "");
        assert!(matches!(err, Error::RateLimited { .. }), "{err}");

        let err = classify(429, &reqwest::header::HeaderMap::new(), "u", "");
        assert!(matches!(err, Error::RateLimited { .. }), "{err}");

        // 带配额头缺失的裸 403 属于凭据问题，不是限流。
        let err = classify(403, &reqwest::header::HeaderMap::new(), "u", "");
        assert!(matches!(err, Error::Http { status: 403, .. }), "{err}");
    }

    #[test]
    fn not_found_gets_its_own_hint() {
        let err = classify(
            404,
            &reqwest::header::HeaderMap::new(),
            "https://api.github.com/x",
            "",
        );
        assert!(matches!(err, Error::ReleaseNotFound { .. }), "{err}");
    }
}
