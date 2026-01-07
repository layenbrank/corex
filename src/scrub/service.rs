use crate::scrub::controller::Args;
use std::collections::BTreeMap;
use std::env;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, mpsc};
use thiserror::Error;
use tokio::sync::Semaphore;
use walkdir::WalkDir;

#[derive(Debug, Error)]
pub enum ScrubError {
    #[error("路径不存在：{0}")]
    NotFound(String),
    #[error("路径不是目录：{0}")]
    NotDirectory(String),
    #[error("遍历失败：{0}")]
    Traverse(String),
    #[error("IO 错误 ({0}): {1}")]
    Io(String, #[source] io::Error),
    #[error("任务失败：{0}")]
    Task(String),
}

/*
Module notes (中文说明):

这个模块实现了高性能的删除（scrub）逻辑，并在保持外部同步 API
`pub fn run(&Args)` 不变的前提下，内部使用 Tokio 做并发异步删除。

设计要点：
- 遍历文件树使用 `walkdir`（这是同步 IO 密集型操作），因此在
    Tokio 环境下使用 `tokio::task::spawn_blocking` 将其移动到阻塞池，
    避免阻塞异步运行时的 reactor。
- 删除操作优先尝试 `tokio::fs` 的异步接口以获得更高吞吐；若遇到
    Windows 权限/路径限制导致失败，则回退到 `std::fs`（通过
    `spawn_blocking` 执行）以提高兼容性。
- 为避免产生过多并发 IO，使用 `tokio::sync::Semaphore` 对并发删除数
    进行限制（分配合理的上限以平衡吞吐与系统压力）。
- 为了安全地从外部同步上下文调用异步逻辑：
    - 若当前线程已有 Tokio 运行时（例如程序其它部分已使用 Tokio），
        则在该运行时中 spawn 异步任务并在阻塞上下文中用 `block_in_place`
        等待其通过 `mpsc` 返回结果；这样可以避免在运行时外层创建多余的
        运行时。
    - 否则创建一个临时多线程运行时并 `block_on` 运行异步逻辑。

命名与实现注意事项：内部异步函数命名为 `run_async`，外部入口保持
`run` 不变以兼容调用者（`main.rs` 的匹配分支无需更改）。
*/
/// 同步入口：保持 `run(&Args)` 签名不变，内部在有/无 Tokio 运行时时
/// 采用不同策略调度并等待 `run_async` 完成。
pub fn run(args: &Args) {
    let source = args.source.clone();
    let recursive = args.recursive;
    let target = args.target.clone();

    // 如果当前线程已有 Tokio 运行时，则 spawn 异步任务并在阻塞上下文内等待结果；
    // 否则新建临时运行时并 block_on
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            let (tx, rx) = mpsc::channel();
            let tgt = source.clone();
            handle.spawn(async move {
                let res = launcher(&tgt, recursive, &target).await;
                let _ = tx.send(match res {
                    Ok(()) => Ok(()),
                    Err(e) => Err(e.to_string()),
                });
            });

            // 在运行时线程上阻塞等待异步任务完成（安全地进入阻塞上下文）
            tokio::task::block_in_place(|| match rx.recv() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => eprintln!("scrub failed: {}", e),
                Err(_) => eprintln!("scrub task canceled"),
            });
        }
        Err(_) => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("failed to create tokio runtime");

            if let Err(e) = rt.block_on(launcher(&source, recursive, &target)) {
                eprintln!("scrub failed: {}", e);
            }
        }
    }
}

// Windows 权限修复 + 重试删除（在 blocking 线程中调用）
fn win_takeown_and_remove(p: &PathBuf, is_dir: bool) -> Result<(), io::Error> {
    // 先尝试清除只读标志
    if let Ok(meta) = std::fs::metadata(p) {
        let mut perm = meta.permissions();
        if perm.readonly() {
            perm.set_readonly(false);
            let _ = std::fs::set_permissions(p, perm);
        }
    }

    // 调用 takeown 授予所有权（需要管理员权限）
    let _ = Command::new("cmd")
        .args(["/C", &format!("takeown /F \"{}\" /R /A", p.display())])
        .status();

    // 授予当前用户完全控制权限
    if let Ok(user) = env::var("USERNAME") {
        if !user.is_empty() {
            let _ = Command::new("cmd")
                .args([
                    "/C",
                    &format!("icacls \"{}\" /grant {}:F /T", p.display(), user),
                ])
                .status();
        }
    }

    // 最后再次尝试删除
    if is_dir {
        std::fs::remove_dir_all(p)
    } else {
        std::fs::remove_file(p)
    }
}

/// 内部异步实现（私有）：执行遍历与并发删除逻辑。
/// - `source`：起始目录路径字符串
/// - `recursive`：是否递归查找子目录
/// - `target`：要删除的名称（文件或目录名）
async fn launcher(source: &str, recursive: bool, target: &str) -> Result<(), ScrubError> {
    // 根路径 PathBuf
    let root = PathBuf::from(source);
    // 验证根路径
    if !root.exists() {
        return Err(ScrubError::NotFound(source.to_string()));
    }

    if recursive && !root.is_dir() {
        return Err(ScrubError::NotDirectory(source.to_string()));
    }

    if recursive {
        // 在 blocking 池中遍历文件树并收集匹配的路径和类型（避免在 async 上做大量 sync IO）
        let fname = target.to_string();
        let mut matches: Vec<(PathBuf, bool)> = tokio::task::spawn_blocking({
            let root = root.clone();
            let fname = fname.clone();
            move || {
                WalkDir::new(&root)
                    .follow_links(false)
                    .into_iter()
                    .filter_map(|entry| entry.ok())
                    .filter_map(|entry| {
                        if entry.file_name().to_string_lossy() == fname {
                            let path: PathBuf = entry.path().to_path_buf();
                            let is_dir = path.is_dir();
                            Some((path, is_dir))
                        } else {
                            None
                        }
                    })
                    .collect()
            }
        })
        .await
        .map_err(|e| ScrubError::Traverse(format!("traverse join error: {}", e)))?;

        if matches.is_empty() {
            println!("未找到匹配项: {}", target);
            return Ok(());
        }

        // 从内向外删除：按路径深度分组并分层处理，确保子目录先完成
        matches.sort_by(|a, b| b.0.components().count().cmp(&a.0.components().count()));

        let mut by_depth: BTreeMap<usize, Vec<(PathBuf, bool)>> = BTreeMap::new();
        for (p, is_dir) in matches {
            let depth = p.components().count();
            by_depth.entry(depth).or_default().push((p, is_dir));
        }

        let semaphore = Arc::new(Semaphore::new(64)); // 并发上限，可调
        let mut first_err: Option<ScrubError> = None;

        // 从深到浅逐层并发删除，每层等待完成后再处理更浅一层
        for (_depth, group) in by_depth.into_iter().rev() {
            let mut tasks = Vec::with_capacity(group.len());
            for (path, is_dir) in group {
                let semaphore = semaphore.clone();
                let p = path.clone();
                let task = tokio::spawn(async move {
                    let _permit = semaphore.acquire().await;
                    if is_dir {
                        if let Err(e_async) = tokio::fs::remove_dir_all(&p).await {
                            // 异步失败后尝试 blocking 删除
                            let p_clone = p.clone();
                            match tokio::task::spawn_blocking(move || {
                                std::fs::remove_dir_all(&p_clone)
                            })
                            .await
                            {
                                Ok(Ok(())) => Ok(()),
                                Ok(Err(e_block)) => match e_block.raw_os_error() {
                                    Some(2) | Some(3) => Ok(()), // 不存在 => 可忽略
                                    Some(5) => {
                                        // 权限问题：尝试修复权限并重试删除（blocking）
                                        let p_retry = p.clone();
                                        match tokio::task::spawn_blocking(move || {
                                            win_takeown_and_remove(&p_retry, true)
                                        })
                                        .await
                                        {
                                            Ok(Ok(())) => Ok(()),
                                            Ok(Err(e3)) => Err(ScrubError::Io(
                                                p.to_string_lossy().to_string(),
                                                e3,
                                            )),
                                            Err(join_err) => Err(ScrubError::Task(format!(
                                                "join error: {}",
                                                join_err
                                            ))),
                                        }
                                    }
                                    _ => Err(ScrubError::Io(
                                        p.to_string_lossy().to_string(),
                                        e_block,
                                    )),
                                },
                                Err(join_err) => {
                                    Err(ScrubError::Task(format!("join error: {}", join_err)))
                                }
                            }
                        } else {
                            Ok(())
                        }
                    } else {
                        if let Err(e_async) = tokio::fs::remove_file(&p).await {
                            let p_clone = p.clone();
                            match tokio::task::spawn_blocking(move || {
                                std::fs::remove_file(&p_clone)
                            })
                            .await
                            {
                                Ok(Ok(())) => Ok(()),
                                Ok(Err(e_block)) => match e_block.raw_os_error() {
                                    Some(2) | Some(3) => Ok(()),
                                    Some(5) => {
                                        let p_retry = p.clone();
                                        match tokio::task::spawn_blocking(move || {
                                            win_takeown_and_remove(&p_retry, false)
                                        })
                                        .await
                                        {
                                            Ok(Ok(())) => Ok(()),
                                            Ok(Err(e3)) => Err(ScrubError::Io(
                                                p.to_string_lossy().to_string(),
                                                e3,
                                            )),
                                            Err(join_err) => Err(ScrubError::Task(format!(
                                                "join error: {}",
                                                join_err
                                            ))),
                                        }
                                    }
                                    _ => Err(ScrubError::Io(
                                        p.to_string_lossy().to_string(),
                                        e_block,
                                    )),
                                },
                                Err(join_err) => {
                                    Err(ScrubError::Task(format!("join error: {}", join_err)))
                                }
                            }
                        } else {
                            Ok(())
                        }
                    }
                });
                tasks.push(task);
            }

            for t in tasks {
                match t.await {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        eprintln!("删除失败: {}", e);
                        if first_err.is_none() {
                            first_err = Some(e);
                        }
                    }
                    Err(join_err) => {
                        eprintln!("删除任务被取消或失败: {}", join_err);
                        if first_err.is_none() {
                            first_err = Some(ScrubError::Task(format!("join error: {}", join_err)));
                        }
                    }
                }
            }
        }

        if let Some(e) = first_err {
            return Err(e);
        }
    } else {
        // shallow 模式：只删除 `source` 下直接子项名为 `target` 的条目
        let fname = target.to_string();
        let children: Vec<PathBuf> = tokio::task::spawn_blocking({
            let tgt = root.clone();
            let fname = fname.clone();
            move || -> Vec<PathBuf> {
                match std::fs::read_dir(&tgt) {
                    Ok(rd) => rd
                        .filter_map(|e| e.ok().map(|ent| ent.path()))
                        .filter(|p| {
                            p.file_name()
                                .map(|n| n.to_string_lossy() == fname)
                                .unwrap_or(false)
                        })
                        .collect(),
                    Err(_) => Vec::new(),
                }
            }
        })
        .await
        .map_err(|e| ScrubError::Traverse(format!("read_dir join error: {}", e)))?;

        // shallow 模式并发删除匹配的直接子项
        let semaphore = Arc::new(Semaphore::new(32));
        let mut tasks = Vec::with_capacity(children.len());
        for child in children {
            let semaphore = semaphore.clone();
            let p = child.clone();
            let task = tokio::spawn(async move {
                let _permit = semaphore.acquire().await;
                if p.is_dir() {
                    if let Err(_e) = tokio::fs::remove_dir_all(&p).await {
                        let p_clone = p.clone();
                        match tokio::task::spawn_blocking(move || std::fs::remove_dir_all(&p_clone))
                            .await
                        {
                            Ok(Ok(())) => Ok(()),
                            Ok(Err(e2)) => Err(ScrubError::Io(p.to_string_lossy().to_string(), e2)),
                            Err(join_err) => {
                                Err(ScrubError::Task(format!("join error: {}", join_err)))
                            }
                        }
                    } else {
                        Ok(())
                    }
                } else {
                    if let Err(_e) = tokio::fs::remove_file(&p).await {
                        let p_clone = p.clone();
                        match tokio::task::spawn_blocking(move || std::fs::remove_file(&p_clone))
                            .await
                        {
                            Ok(Ok(())) => Ok(()),
                            Ok(Err(e2)) => Err(ScrubError::Io(p.to_string_lossy().to_string(), e2)),
                            Err(join_err) => {
                                Err(ScrubError::Task(format!("join error: {}", join_err)))
                            }
                        }
                    } else {
                        Ok(())
                    }
                }
            });
            tasks.push(task);
        }
        let mut first_err: Option<ScrubError> = None;
        for t in tasks {
            match t.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    eprintln!("删除失败: {}", e);
                    if first_err.is_none() {
                        first_err = Some(e);
                    }
                }
                Err(join_err) => {
                    eprintln!("删除任务被取消或失败: {}", join_err);
                    if first_err.is_none() {
                        first_err = Some(ScrubError::Task(format!("join error: {}", join_err)));
                    }
                }
            }
        }

        if let Some(e) = first_err {
            return Err(e);
        }
    }

    Ok(())
}

// 保留（但目前未使用）的忽略规则函数
#[allow(dead_code)]
fn _is_ignored(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|s| s.starts_with('.') || ["node_modules", ".git"].contains(&s))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use tempfile::tempdir;

    #[test]
    fn test_shallow_delete() {
        let dir = tempdir().unwrap();
        let root = dir.path().to_path_buf();

        // create node_modules as direct child
        let node = root.join("node_modules");
        fs::create_dir_all(node.join("pkg")).unwrap();
        File::create(node.join("pkg").join("file.txt")).unwrap();

        // another directory to ensure it's untouched
        fs::create_dir_all(root.join("other")).unwrap();

        let args = crate::scrub::controller::Args {
            source: root.to_string_lossy().to_string(),
            target: "node_modules".to_string(),
            recursive: false,
        };

        run(&args);

        assert!(
            !node.exists(),
            "node_modules should be removed in shallow mode"
        );
        assert!(root.join("other").exists(), "other should remain");
    }

    #[test]
    fn test_recursive_delete() {
        let dir = tempdir().unwrap();
        let root = dir.path().to_path_buf();

        // create nested node_modules deep
        let nested = root.join("a").join("b").join("node_modules").join("pkg");
        fs::create_dir_all(&nested).unwrap();
        File::create(nested.join("f.txt")).unwrap();

        let args = crate::scrub::controller::Args {
            source: root.to_string_lossy().to_string(),
            target: "node_modules".to_string(),
            recursive: true,
        };

        run(&args);

        assert!(
            !root.join("a").join("b").join("node_modules").exists(),
            "nested node_modules should be removed"
        );
    }
}
