use std::{
    fs::{File, create_dir_all},
    path::Path,
};
use walkdir::WalkDir;
use zip::{CompressionMethod, ZipWriter, write::FileOptions};

use crate::utils::notify::Notification;

use crate::compression::controller::{Args, Exception};

pub fn run(args: &Args) {
    match bootstrap(&args) {
        Ok(_) => {
            let _ = Notification::success("压缩成功", "压缩操作已成功完成");
        }
        Err(e) => {
            let _ = Notification::error("压缩失败", &format!("压缩过程中发生错误: {}", e));
        }
    }
}

pub fn bootstrap(args: &Args) -> Result<(), Exception> {
    let from = Path::new(&args.from);
    let to = Path::new(&args.to);

    if !from.join("index.html").exists() {
        return Err(Exception::InvalidInput(format!(
            "输入目录缺少 index.html，这不是有效的 H5+ 构建产物: {}",
            from.display()
        )));
    }

    // 确保输出目录存在
    if let Some(parent) = to.parent() {
        create_dir_all(parent)?;
    }

    // 4. 创建 ZIP 文件
    let output_file = File::create(to)?;

    let mut zip = ZipWriter::new(output_file);
    let options: FileOptions<()> = FileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(6));

    let mut file_count = 0u64;
    let mut total_bytes = 0u64;

    // 5. 遍历目录，保持相对路径
    for entry in WalkDir::new(from)
        .min_depth(1) // 不包含根目录本身
        .follow_links(false) // 不跟随符号链接（安全）
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        // 跳过目录（只加文件）
        if path.is_dir() {
            continue;
        }

        // 计算相对路径（ZIP 内路径）
        let rel_path = path
            .strip_prefix(from)
            .map_err(|_| Exception::InvalidInput(format!("路径计算失败: {}", path.display())))?;

        // Windows 路径分隔符统一为 '/'（ZIP 规范要求）
        let zip_path = rel_path
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");

        // 写入 ZIP
        zip.start_file(&zip_path, options)?;

        let mut file = File::open(path)?;
        let bytes_written = std::io::copy(&mut file, &mut zip)?;

        total_bytes += bytes_written;
        file_count += 1;
    }

    // 6. 完成 ZIP 文件
    let _writer = zip.finish()?;

    Ok(())
}
