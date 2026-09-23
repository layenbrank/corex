//! `corex paths`：把本次运行实际使用的目录与端点交给宿主，让它不必自己猜。
//!
//! 宿主（编辑器、启动器、agent）过去只能复刻一遍 `data_dir` 的平台规则去拼指令路径；
//! 拼错时它读到的是一份**看起来完全正常、其实是另一个世界**的数据——Windows 上
//! `%LOCALAPPDATA%\corex` 里恰好也躺着同名指令，于是「明明改了却没生效」这类问题
//! 得靠翻两棵树才能看出来。
//!
//! 这里交出的是同一批事实的机器可读形态（`--json`），值全部由 corex 自己算：
//! 数据目录用 [`data_dir`]，端点走连接方的发现顺序（[`crate::resolve_endpoint`]），
//! 指令目录与 `run` / `schedule` 用的是同一处 [`Paths::dir`]。
//!
//! 这里只报**位置**，不报 token 本身：路径可以随便贴进 issue，密钥不行。

use crate::output::outln;
use crate::resolve_endpoint;
use crate::scheduler::Paths;
use anyhow::Result;
use corex_core::VERSION;
use corex_ipc::data_dir;
use corex_ipc::endpoint;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// 给宿主看的一份路径清单。
#[derive(Serialize)]
struct Listing {
    /// corex 版本；宿主据此判断对面支不支持某个命令或字段。
    version: &'static str,
    data_dir: PathBuf,
    directives_dir: PathBuf,
    endpoint: PathBuf,
    /// 端点的形态：`pipe`（Windows 命名管道）或 `socket`（Unix 域套接字）。
    kind: &'static str,
    /// daemon 的 token 文件；两种情况为 `None`：记录里没有它（token 来自环境变量 / 配置，
    /// 那属于调用方），或没有任何记录且默认文件也不存在。
    token_file: Option<PathBuf>,
}

/// `json` 为真时打机器可读的 JSON，否则打给人看的一行一项。
pub(crate) fn run(json: bool, dir: Option<&Path>) -> Result<()> {
    let data = data_dir()?;
    let endpoint = resolve_endpoint()?;
    let listing = Listing {
        version: VERSION,
        directives_dir: Paths::dir(dir)?,
        kind: endpoint::kind_of(&data).as_str(),
        token_file: token_file(&data),
        data_dir: data,
        endpoint,
    };

    if json {
        return emit(&listing);
    }
    outln!("{:<10} {}", "数据目录", listing.data_dir.display());
    outln!("{:<10} {}", "指令目录", listing.directives_dir.display());
    outln!(
        "{:<10} {} ({})",
        "IPC 端点",
        listing.endpoint.display(),
        listing.kind
    );
    match &listing.token_file {
        Some(path) => outln!("{:<10} {}", "token 文件", path.display()),
        None => outln!(
            "{:<10} 无（token 来自 COREX_TOKEN / 配置，或还没有 daemon 的记录）",
            "token 文件"
        ),
    }
    Ok(())
}

/// daemon 的 token 文件：记录里怎么写就怎么用，没有记录就是数据目录下那个默认文件。
///
/// 记录里没有 `token_file` 是**有意义**的——那次 daemon 的 token 来自环境变量或配置，
/// 此时不能退回 `<data>/token`：那个文件属于上一个 daemon，拿它去连只会得到一句 401。
/// 压根没有记录（还没跑过 daemon）时才看默认文件，且**存在才算**：不存在就是真的没有，
/// 而不是「有一个但读不到」。
fn token_file(data: &Path) -> Option<PathBuf> {
    match endpoint::discover(data) {
        Some(record) => record.token_file,
        None => {
            let default = data.join("token");
            default.is_file().then_some(default)
        }
    }
}

fn emit(listing: &Listing) -> Result<()> {
    outln!("{}", serde_json::to_string_pretty(listing)?);
    Ok(())
}
