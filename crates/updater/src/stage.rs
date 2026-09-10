//! 把下载到的字节变成一个可运行的可执行文件。
//!
//! 解包、校验、冒烟测试——全程已安装的二进制都完好无损，
//! 所以这里任何一步失败，只需重新跑旧版本就能恢复。

use semver::Version;
use std::path::{Path, PathBuf};

use crate::asset::{Kind, binary_name};
use crate::error::{Error, Result};
use crate::install;
use crate::plan::Plan;
use crate::verify::{self, sha256_file};

/// 在 `staging` 里落地出一个二进制，需要时解包压缩文件。
///
/// 按 plan 已经带上的产物类型分派，`run` 不可能与 `plan` 做的选择不一致。
pub(crate) fn binary(plan: &Plan, staging: &Path, bytes: &[u8]) -> Result<PathBuf> {
    let binary = match plan.kind {
        Kind::Binary => install::write_staged(staging, binary_name(), bytes)?,
        Kind::Zip => {
            let archive = install::write_staged(staging, &plan.asset.name, bytes)?;
            let binary = install::extract_from_zip(&archive, binary_name(), staging)?;
            verify_extracted(plan, &binary)?;
            binary
        }
    };
    // zip 不带权限位，裸下载也不带，没有这一步下面的冒烟测试
    // 在非 Windows 主机上会失败。
    install::mark_executable(&binary)?;
    Ok(binary)
}

/// 把暂存好的二进制跑一次，免得一个连版本号都报不出来的下载被装到可用版本头上。
pub(crate) fn smoke_test(binary: &Path, want: &Version) -> Result<()> {
    let got = install::probe_version(binary)?;
    if got.trim() == want.to_string() {
        return Ok(());
    }
    Err(Error::SmokeTestFailed {
        path: binary.to_path_buf(),
        got,
        want: want.to_string(),
    })
}

/// 用覆盖该压缩文件的清单校验解包出来的二进制。
///
/// 产物摘要只能证明压缩文件本身完整；这一步额外证明从中取出的那个成员
/// 就是当初发布的那个。
fn verify_extracted(plan: &Plan, binary: &Path) -> Result<()> {
    let Some(expected) = plan
        .sums
        .as_ref()
        .and_then(|sums| sums.get(&verify::basename(binary_name())))
    else {
        return Ok(());
    };
    let actual = sha256_file(binary)?;
    verify::require_digest(
        &format!("{} 中的 {}", plan.asset.name, binary_name()),
        expected,
        &actual,
    )
}
