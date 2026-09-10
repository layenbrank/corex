//! 校验下载内容的摘要从哪里来。
//!
//! 同一个事实，一个 Release 最多可以在三个地方声明。这里把它们收集起来，
//! 好让 `run` 要求它们全部一致，而不是碰到第一个就信。

use std::collections::HashMap;

use crate::Updater;
use crate::asset::SUMS;
use crate::github::{Asset, Release};
use crate::verify::{self, is_sha256, parse_sidecar};

impl Updater {
    /// `release` 为 `asset` 发布的全部摘要，带来源标注。
    ///
    /// 被查的每个来源都必须一致：它们是对同一串字节的独立声明，
    /// 所以过期或被篡改的那份会被抳出来，而不是被多数票静默盖过。
    /// 新增一个来源 = 加一个 [`Source`] 变体 + 一条 [`Source::ALL`] 条目，这里不用动。
    pub(crate) async fn digests(
        &self,
        release: &Release,
        asset: &Asset,
        sums: Option<&HashMap<String, String>>,
    ) -> Vec<(String, String)> {
        let mut found = Vec::new();
        for source in Source::ALL {
            if source.costs_request() && !found.is_empty() {
                continue;
            }
            if let Some(entry) = self.digest(source, release, asset, sums).await {
                found.push(entry);
            }
        }
        found
    }

    /// 解析某一个摘要来源；该来源对 `asset` 没说法时返回 `None`。
    async fn digest(
        &self,
        source: Source,
        release: &Release,
        asset: &Asset,
        sums: Option<&HashMap<String, String>>,
    ) -> Option<(String, String)> {
        match source {
            Source::Api => {
                let digest = asset.digest.as_deref().map(verify::strip_algo)?;
                is_sha256(digest).then(|| {
                    (
                        format!("GitHub asset digest ({})", asset.name),
                        digest.to_ascii_lowercase(),
                    )
                })
            }
            Source::Manifest => {
                let digest = sums?.get(&verify::basename(&asset.name))?;
                Some((format!("{SUMS} ({})", asset.name), digest.clone()))
            }
            Source::Sidecar => {
                let name = format!("{}.sha256", asset.name);
                let upload = release.asset(&name)?;
                match self.github.fetch_text(&upload.browser_download_url).await {
                    Ok(text) => parse_sidecar(&text).map(|digest| (name, digest)),
                    Err(err) => {
                        tracing::debug!(error = %err, "获取 .sha256 旁车失败");
                        None
                    }
                }
            }
        }
    }
}

/// 已发布摘要的可能来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    /// GitHub API 随每个上传返回的单产物摘要。
    Api,
    /// Release 的 `SHA256SUMS.txt` 中该产物的条目。
    Manifest,
    /// 与产物并排发布的 `<asset>.sha256` 旁车文件。
    Sidecar,
}

impl Source {
    /// 全部来源，按可信度从高到低。
    const ALL: [Self; 3] = [Self::Api, Self::Manifest, Self::Sidecar];

    /// 解析该来源是否需要额外一次 HTTP 请求。
    ///
    /// 这类来源只在前面免费的手段都没覆盖到该产物时才查，
    /// 使常见路径仍只花掉取 Release 的那一次请求。
    fn costs_request(self) -> bool {
        matches!(self, Self::Sidecar)
    }
}
