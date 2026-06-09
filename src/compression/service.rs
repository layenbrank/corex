use std::{
    fs::{File, create_dir_all},
    path::Path,
    time::Instant,
};
use walkdir::WalkDir;
use zip::{CompressionMethod, ZipWriter, write::FileOptions};

use crate::compression::controller::{Args, Exception};
use crate::schedule::pipeline::Context as PipelineContext;
use crate::utils::{file, notify::Notification, progress::Progress};

pub fn run(args: &Args) {
    match bootstrap(args) {
        Ok(_) => {
            let _ = Notification::success("压缩成功", "压缩操作已成功完成");
        }
        Err(e) => {
            let _ = Notification::error("压缩失败", &format!("压缩过程中发生错误: {}", e));
        }
    }
}

/// 纯压缩：将 `from` 目录下所有文件打包为 ZIP 写入 `to`
pub fn bootstrap(args: &Args) -> Result<(), Exception> {
    let from = Path::new(&args.from);
    let to = Path::new(&args.to);

    // 确保输出目录存在
    if let Some(parent) = to.parent() {
        create_dir_all(parent)?;
    }

    // 预扫描文件数
    let spinner = Progress::spinner("正在扫描文件...");
    let file_count = WalkDir::new(from)
        .min_depth(1)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .count();
    spinner.finish_and_clear();

    if file_count == 0 {
        println!("没有文件需要压缩");
        return Ok(());
    }

    println!("找到 {} 个文件", file_count);

    let pb = Progress::progress(file_count as u64);
    pb.set_message("正在压缩...");
    pb.tick(); // 强制初始渲染
    let start = Instant::now();
    let mut total_bytes: u64 = 0;

    let output_file = File::create(to)?;
    let mut zip = ZipWriter::new(output_file);
    let options: FileOptions<()> = FileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(6));

    for entry in WalkDir::new(from)
        .min_depth(1)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        // 只处理文件，跳过目录
        if path.is_dir() {
            continue;
        }

        // 更新进度条显示当前文件
        if let Some(name) = path.file_name() {
            pb.set_message(file::truncate(&name.to_string_lossy(), 30));
        }

        // 计算相对路径（ZIP 内路径）
        let rel_path = path.strip_prefix(from).map_err(|_| {
            Exception::PathError(format!("路径计算失败: {}", path.display()))
        })?;

        // Windows 路径分隔符统一为 '/'（ZIP 规范要求）
        let zip_path = rel_path
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");

        let file_size = path.metadata().map(|m| m.len()).unwrap_or(0);
        total_bytes += file_size;

        zip.start_file(&zip_path, options)?;
        let mut file = File::open(path)?;
        let _ = std::io::copy(&mut file, &mut zip)?;

        pb.inc(1);
    }

    zip.finish()?;

    let elapsed = start.elapsed();
    let avg_speed = file::speed(total_bytes, elapsed);
    pb.finish_with_message(format!(
        "完成 {} 个文件, {}, 用时 {}, 平均 {}",
        file_count,
        file::size(total_bytes),
        file::duration(elapsed),
        format!("{}/s", file::size(avg_speed))
    ));

    Ok(())
}

/// Pipeline 调用入口：
/// - 若 `args.from` 为 `$last_output`，则从 ctx 读取上一步的输出路径作为源目录
/// - 执行后将 `to` 路径写入 ctx.last_output
pub fn execute(args: &Args, ctx: &mut PipelineContext) -> Result<(), Exception> {
    let resolved_from = if args.from == "$last_output" {
        ctx.last_output
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| args.from.clone())
    } else {
        args.from.clone()
    };

    let resolved_args = Args {
        from: resolved_from,
        to: args.to.clone(),
        description: args.description.clone(),
        id: args.id.clone(),
    };

    bootstrap(&resolved_args)?;

    ctx.set_output(std::path::PathBuf::from(&resolved_args.to));
    Ok(())
}
