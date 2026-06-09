use std::{
    env, io,
    path::PathBuf,
    process::Command,
    sync::{Arc, mpsc},
    time::Instant,
};

use thiserror::Error;
use tokio::sync::Semaphore;
use walkdir::WalkDir;

use crate::scrub::controller::Args;
use crate::utils::{file, progress::Progress};

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

/// 收集目录下所有子项（文件+子目录），并计算文件总大小
/// 返回: (待删除项列表, 文件总字节数)
fn collect_descendants(paths: Vec<(PathBuf, bool)>) -> (Vec<(PathBuf, bool)>, u64) {
    let mut result = Vec::new();
    let mut total_size: u64 = 0;
    for (path, is_dir) in &paths {
        if *is_dir {
            for entry in WalkDir::new(path)
                .min_depth(1)
                .follow_links(false)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let p = entry.path().to_path_buf();
                let d = p.is_dir();
                if !d {
                    total_size += p.metadata().map(|m| m.len()).unwrap_or(0);
                }
                result.push((p, d));
            }
            // 匹配目录本身也加入删除列表（内容清空后删除空目录）
            result.push((path.clone(), true));
        } else {
            total_size += path.metadata().map(|m| m.len()).unwrap_or(0);
            result.push((path.clone(), false));
        }
    }
    (result, total_size)
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
        let spinner = Progress::spinner("正在扫描匹配项...");
        let fname = target.to_string();
        let matches: Vec<(PathBuf, bool)> = tokio::task::spawn_blocking({
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

        spinner.finish_and_clear();

        if matches.is_empty() {
            println!("未找到匹配项: {}", target);
            return Ok(());
        }

        println!("找到 {} 个匹配项: {}", matches.len(), target);

        // 展开匹配目录的所有子项，用于精确的删除进度统计
        let scan_spinner = Progress::spinner("正在统计待删除项...");
        let (mut all_items, total_size) = tokio::task::spawn_blocking(move || collect_descendants(matches))
            .await
            .map_err(|e| ScrubError::Traverse(format!("collect descendants error: {}", e)))?;
        scan_spinner.finish_and_clear();

        if all_items.is_empty() {
            println!("没有需要删除的项");
            return Ok(());
        }

        let file_count = all_items.iter().filter(|(_, d)| !d).count();
        let dir_count = all_items.iter().filter(|(_, d)| *d).count();
        println!(
            "共 {} 项待删除 ({} 个文件, {} 个目录, 总大小 {})",
            all_items.len(),
            file_count,
            dir_count,
            file::size(total_size)
        );

        // 按深度降序排列：先删最深的内容，最后删空目录
        all_items.sort_by(|a, b| b.0.components().count().cmp(&a.0.components().count()));

        let total_items: u64 = all_items.len() as u64;
        let pb = Progress::progress(total_items);
        pb.set_message(format!("正在删除 {} 项...", total_items));
        pb.tick(); // 强制初始渲染，避免操作过快时进度条从未显示
        let start = Instant::now();
        let semaphore = Arc::new(Semaphore::new(64)); // 并发上限，可调
        let mut first_err: Option<ScrubError> = None;

        // 从深到浅逐层并发删除
        let mut tasks = Vec::with_capacity(all_items.len());
        for (path, is_dir) in all_items {
            let semaphore = semaphore.clone();
            let p = path.clone();
            let task = tokio::spawn(async move {
                let _permit = semaphore.acquire().await;
                if is_dir {
                    if let Err(_e_async) = tokio::fs::remove_dir_all(&p).await {
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
                    if let Err(_e_async) = tokio::fs::remove_file(&p).await {
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
                Ok(Ok(())) => {
                    pb.inc(1);
                }
                Ok(Err(e)) => {
                    eprintln!("删除失败: {}", e);
                    pb.inc(1);
                    if first_err.is_none() {
                        first_err = Some(e);
                    }
                }
                Err(join_err) => {
                    eprintln!("删除任务被取消或失败: {}", join_err);
                    pb.inc(1);
                    if first_err.is_none() {
                        first_err = Some(ScrubError::Task(format!("join error: {}", join_err)));
                    }
                }
            }
        }

        let elapsed = start.elapsed();
        let avg_speed = file::speed(total_size, elapsed);
        pb.finish_with_message(format!("删除完成，共处理 {} 项", total_items));
        println!(
            "删除完成，共处理 {} 项: {} | 用时 {} | 平均 {}",
            total_items,
            target,
            file::duration(elapsed),
            format!("{}/s", file::size(avg_speed))
        );

        if let Some(e) = first_err {
            return Err(e);
        }
    } else {
        // shallow 模式：只删除 `source` 下直接子项名为 `target` 的条目
        let spinner = Progress::spinner("正在扫描匹配项...");
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

        spinner.finish_and_clear();

        if children.is_empty() {
            println!("未找到匹配项: {}", target);
            return Ok(());
        }

        println!("找到 {} 个匹配项: {}", children.len(), target);

        // shallow 模式并发删除匹配的直接子项
        let child_count = children.len() as u64;
        let pb = Progress::progress(child_count);
        pb.set_message(format!("正在删除 {} 个匹配项...", child_count));
        pb.tick(); // 强制初始渲染
        let start = Instant::now();
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
                Ok(Ok(())) => {
                    pb.inc(1);
                }
                Ok(Err(e)) => {
                    eprintln!("删除失败: {}", e);
                    pb.inc(1);
                    if first_err.is_none() {
                        first_err = Some(e);
                    }
                }
                Err(join_err) => {
                    eprintln!("删除任务被取消或失败: {}", join_err);
                    pb.inc(1);
                    if first_err.is_none() {
                        first_err = Some(ScrubError::Task(format!("join error: {}", join_err)));
                    }
                }
            }
        }

        let elapsed = start.elapsed();
        let items_per_sec = file::speed(child_count, elapsed);
        pb.finish_with_message(format!("删除完成，共处理 {} 项", child_count));
        println!(
            "删除完成，共处理 {} 项: {} | 用时 {} | 平均 {} 项/s",
            child_count,
            target,
            file::duration(elapsed),
            items_per_sec
        );

        if let Some(e) = first_err {
            return Err(e);
        }
    }

    Ok(())
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
