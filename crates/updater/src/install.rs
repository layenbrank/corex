//! 暂存、冒烟测试，以及原子地替换运行中的可执行文件。

use crate::error::{Error, Result, describe};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Windows `ERROR_SHARING_VIOLATION`：另一个进程把文件开着。
const ERROR_SHARING_VIOLATION: i32 = 32;
/// Windows `ERROR_LOCK_VIOLATION`：字节区间锁把重命名挡住了。
const ERROR_LOCK_VIOLATION: i32 = 33;
/// 目标仍被锁住时，两次 `self_replace` 尝试之间的延迟。
const RETRY_DELAYS_MS: [u64; 3] = [250, 750, 2_000];

/// 运行中可执行文件的路径。
pub fn current_exe() -> Result<PathBuf> {
    std::env::current_exe().map_err(Error::Io)
}

/// 在目标二进制旁边建一个暂存目录。
///
/// 同一文件系统是硬性要求：最后的替换是一次 `rename`，
/// 而它跨不了卷。返回的 guard 被丢弃时，该目录会被删掉。
/// 返回的 guard 被丢弃时，该目录会被删掉。
pub fn stage(install_dir: &Path) -> Result<tempfile::TempDir> {
    tempfile::Builder::new()
        .prefix(".corex-update-")
        .tempdir_in(install_dir)
        .map_err(|e| Error::InstallPathNotWritable {
            path: install_dir.to_path_buf(),
            why: e.to_string(),
        })
}

/// `install_dir` 不会写文件时提前失败，免得白花一次下载——
///
/// 这里失败就是常见的“装在 `Program Files` 下”的情形，需要提权，
/// 而本 crate 自己从不做提权。
pub fn ensure_writable(install_dir: &Path) -> Result<()> {
    if !install_dir.is_dir() {
        return Err(Error::InstallPathNotWritable {
            path: install_dir.to_path_buf(),
            why: "不是目录".into(),
        });
    }
    tempfile::Builder::new()
        .prefix(".corex-writetest-")
        .tempfile_in(install_dir)
        .map(|_| ())
        .map_err(|e| Error::InstallPathNotWritable {
            path: install_dir.to_path_buf(),
            why: e.to_string(),
        })
}

/// 把 `bytes` 以 `name` 写入 `staging`。
pub fn write_staged(staging: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf> {
    let path = staging.join(name);
    std::fs::write(&path, bytes)?;
    Ok(path)
}

/// 从 `.zip` release 压缩包里把 `entry` 取到 `staging`。
///
/// 只按文件名匹配条目：工作流可能把它们放在压缩包根目录，
/// 也可能放在带版本号的子目录下。
pub fn extract_from_zip(archive: &Path, entry: &str, staging: &Path) -> Result<PathBuf> {
    let file = std::fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| Error::Archive(e.to_string()))?;
    let index = (0..zip.len()).find(|&i| {
        zip.by_index(i)
            .map(|member| {
                !member.is_dir()
                    && crate::verify::basename(member.name()).eq_ignore_ascii_case(entry)
            })
            .unwrap_or(false)
    });
    let Some(index) = index else {
        return Err(Error::ArchiveMissingEntry {
            archive: archive.display().to_string(),
            entry: entry.to_string(),
        });
    };
    let mut member = zip
        .by_index(index)
        .map_err(|e| Error::Archive(e.to_string()))?;
    let dest = staging.join(entry);
    let mut out = std::fs::File::create(&dest)?;
    std::io::copy(&mut member, &mut out)?;
    Ok(dest)
}

/// 恢复可执行位：`.zip` 不带它，裸下载也从来没有过。
/// 等二进制就位后由调用方调用，使每套获取策略最后都得到一个可运行的文件。
/// 每套获取策略最后都得到可运行文件。
pub(crate) fn mark_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms)?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

/// 跑一次 `<binary> --version`，返回它报出的版本。
///
/// 这是替换前最后一道门：连自己的版本都报不出来的暂存二进制，
/// 永远不会被装上。
pub fn probe_version(binary: &Path) -> Result<String> {
    let output = std::process::Command::new(binary)
        .arg("--version")
        .output()
        .map_err(|e| Error::StagedBinaryUnusable {
            path: binary.to_path_buf(),
            why: e.to_string(),
        })?;
    if !output.status.success() {
        return Err(Error::StagedBinaryUnusable {
            path: binary.to_path_buf(),
            why: format!("`--version` 退出码 {}", output.status),
        });
    }
    // clap 打印的是 “corex 6.0.2”；版本是最后一个 token。
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next_back()
        .map(str::to_string)
        .ok_or_else(|| Error::StagedBinaryUnusable {
            path: binary.to_path_buf(),
            why: "`--version` 没有输出".into(),
        })
}

/// 用 `new_binary` 替换运行中的可执行文件。
///
/// 替换总是针对本进程启动时的那份镜像；
/// `install_path` 在调用方那边就是 `current_exe()`，只用来标注错误，
/// 使失败信息里出现的是用户认得出来的那个文件名。
///
/// `self-replace` 会把运行中的镜像改名让开，把新的拷进去，
/// 然后把旧文件交给本进程的一个短命副本去删除——
/// 所以清理要等本进程退出后才完成。
///
/// 只要还有别的进程（杀毒软件、另一个实例、已加载的 DLL）握着句柄，
/// Windows 就拒绝重命名，所以共享冲突是带退避重试，
/// 而不是直接判更新失败。
pub fn replace_running_exe(new_binary: &Path, install_path: &Path) -> Result<()> {
    let mut attempt = 0usize;
    loop {
        match self_replace::self_replace(new_binary) {
            Ok(()) => return Ok(()),
            Err(err) => {
                if !is_transient_lock(&err) || attempt >= RETRY_DELAYS_MS.len() {
                    return Err(Error::Replace {
                        path: install_path.to_path_buf(),
                        why: describe(&err),
                    });
                }
                let delay = RETRY_DELAYS_MS[attempt];
                attempt += 1;
                tracing::warn!(attempt, delay_ms = delay, error = %err, "目标文件被占用，重试替换");
                std::thread::sleep(Duration::from_millis(delay));
            }
        }
    }
}

/// 失败是否属于可重试的 Windows 临时锁，而非硬错误。
fn is_transient_lock(err: &std::io::Error) -> bool {
    matches!(
        err.raw_os_error(),
        Some(ERROR_SHARING_VIOLATION) | Some(ERROR_LOCK_VIOLATION)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writable_probe_accepts_a_temp_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert!(ensure_writable(dir.path()).is_ok());
    }

    #[test]
    fn writable_probe_rejects_a_missing_dir() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope");
        let err = ensure_writable(&missing).unwrap_err();
        assert!(matches!(err, Error::InstallPathNotWritable { .. }), "{err}");
    }

    #[test]
    fn staged_files_land_in_the_staging_dir() {
        let dir = tempfile::tempdir().unwrap();
        let staging = stage(dir.path()).unwrap();
        let path = write_staged(staging.path(), "corex.exe", b"payload").unwrap();
        assert!(path.starts_with(staging.path()));
        assert_eq!(std::fs::read(&path).unwrap(), b"payload");
    }

    #[test]
    fn missing_zip_entry_is_reported_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("empty.zip");
        let file = std::fs::File::create(&archive).unwrap();
        zip::ZipWriter::new(file).finish().unwrap();
        let err = extract_from_zip(&archive, "corex.exe", dir.path()).unwrap_err();
        assert!(matches!(err, Error::ArchiveMissingEntry { .. }), "{err}");
    }

    #[test]
    fn probe_reports_a_missing_binary() {
        let dir = tempfile::tempdir().unwrap();
        let err = probe_version(&dir.path().join("absent.exe")).unwrap_err();
        assert!(matches!(err, Error::StagedBinaryUnusable { .. }), "{err}");
    }
}
