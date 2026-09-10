//! 自更新操作的错误类型。

use std::path::PathBuf;

/// 本 crate 的 Result 别名。
pub type Result<T> = std::result::Result<T, Error>;

/// 把错误与它的因果链一起渲染出来。
///
/// `reqwest` 与 `self-replace` 都只给一句笼统的顶层消息（“error decoding response
/// body”），把真正可行动的信息——超时、TLS 失败、Windows 共享冲突——放在 source 链里，
/// 而单靠 `Display` 会丢掉它们。
pub(crate) fn describe(err: &dyn std::error::Error) -> String {
    let mut text = err.to_string();
    let mut cause = err.source();
    while let Some(current) = cause {
        text.push_str(": ");
        text.push_str(&current.to_string());
        cause = current.source();
    }
    text
}

/// 检查或应用更新时可能出现的各种错误。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// `[update].enabled = false`（企业 / 离线环境锁定）。
    #[error("自更新已被配置禁用（[update].enabled = false）")]
    Disabled,

    /// 配置的 repository 不是 `owner/repo` 形式。
    #[error("repository 配置无效：`{0}`（应为 owner/repo）")]
    InvalidRepository(String),

    /// 版本字符串不是合法语义化版本。
    #[error("版本号无效：`{0}`（应为 SemVer，如 6.0.2 / 6.0.2-beta.1）")]
    InvalidVersion(String),

    /// 当前目标平台没有对应的产物命名方案。
    #[error("不支持当前平台：{os}/{arch}（目前仅发布 Windows x64 产物）")]
    UnsupportedPlatform { os: String, arch: String },

    /// 该通道目前指向不了可用的 Release。
    #[error("未找到符合通道 `{channel}` 的 Release")]
    NoRelease { channel: &'static str },

    /// 仓库或 tag 不存在（私有仓库 GitHub 同样回 404，
    /// 所以这一条也包含“本 token 看不到”）。
    #[error("GitHub 返回 404：{url}（检查 [update].repository 拼写，或为私有仓库配置 token）")]
    ReleaseNotFound { url: String },

    /// 该 Release 没有本平台可安装的产物。
    #[error("Release {tag} 中没有适用于 `{platform}` 的资产（可用：{available}）")]
    NoAsset {
        tag: String,
        platform: String,
        available: String,
    },

    /// 校验和不匹配。
    #[error("{from} 校验失败：期望 {expected}，实际 {actual}")]
    ChecksumMismatch {
        /// 哪个摘要来源不一致，如 `SHA256SUMS.txt (corex.exe)`。
        ///
        /// 刻意不叫 `source`：`thiserror` 会把 `source` 当作因果链字段，
        /// 那样就要求这里的值实现 `std::error::Error`。
        from: String,
        expected: String,
        actual: String,
    },

    /// 没有任何可用于校验下载的凭据。
    #[error("无法校验 {0}：Release 未提供 digest，且 SHA256SUMS.txt 中无对应条目")]
    ChecksumUnavailable(String),

    /// 安装目录不接受新文件。
    #[error("安装目录不可写：{path}（{why}）。请改用用户可写目录，或以管理员身份运行")]
    InstallPathNotWritable { path: PathBuf, why: String },

    /// GitHub 对请求做了限流。
    #[error("GitHub API 限流（{why}）；请稍后重试，或设置 {token_env} 提高配额")]
    RateLimited { why: String, token_env: String },

    /// 服务器返回了非成功状态码。
    #[error("HTTP {status} 请求 {url} 失败")]
    Http { status: u16, url: String },

    /// 传输 / TLS / 超时失败。
    #[error("网络请求失败：{0}")]
    HttpTransport(String),

    /// 响应体超过了安全上限。
    #[error("响应内容超过上限（{0} 字节）")]
    DownloadTooLarge(u64),

    /// 压缩包里没有期望的二进制。
    #[error("压缩包 `{archive}` 中未找到 `{entry}`")]
    ArchiveMissingEntry { archive: String, entry: String },

    /// 压缩包打不开或读不了。
    #[error("读取压缩包失败：{0}")]
    Archive(String),

    /// GitHub API 响应无法解码。
    #[error("解析 GitHub 响应失败（{url}）：{why}")]
    Api { url: String, why: String },

    /// 暂存的二进制报出的版本不是期望值。
    #[error("更新自检失败：`{path}` 报告版本 `{got}`，期望 `{want}`")]
    SmokeTestFailed {
        path: PathBuf,
        got: String,
        want: String,
    },

    /// 暂存的二进制根本执行不了。
    #[error("无法执行暂存的二进制 `{path}`：{why}")]
    StagedBinaryUnusable { path: PathBuf, why: String },

    /// 替换运行中的可执行文件失败。
    #[error("替换 `{path}` 失败：{why}")]
    Replace { path: PathBuf, why: String },

    /// 文件系统错误。
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// 状态文件或 GitHub 负载序列化 / 反序列化失败。
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
