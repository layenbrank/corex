use crate::generate::controller::PathArgs;
use crate::utils::verifier::Verifier;
use anyhow::{Context, Result};
use glob::Pattern;
use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::Path,
};
use walkdir::WalkDir;

pub fn run_path(args: &PathArgs) -> Result<()> {
    // 这里是具体的实现逻辑
    let from = Path::new(&args.from);
    let to = Path::new(&args.to);
    let transform = args.transform.clone();
    let ignores = args.ignores.clone();

    if to.is_dir() {
        return Err(anyhow::anyhow!("目标路径应是一个文件路径!"));
    }

    // 创建或清空文件，然后创建缓冲写入器
    let file = if let Some(to_str) = to.to_str() {
        if Verifier::file(to_str).is_err() {
            // 文件不存在则创建文件
            File::create(to)?
        } else {
            // 文件存在则以写入模式打开（会清空内容）
            OpenOptions::new().write(true).truncate(true).open(to)?
        }
    } else {
        File::create(to)?
    };

    // 创建缓冲写入器，类似 Node.js 的流
    let mut writer = BufWriter::new(file);

    // 预编译 glob 模式以提高性能
    let patterns: Vec<Pattern> = ignores
        .iter()
        .filter_map(|pattern| Pattern::new(pattern).ok())
        .collect();

    let entries: Vec<_> = WalkDir::new(from)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|entry| {
            let raw_path = entry.path().strip_prefix(from).unwrap_or(entry.path());
            !ignored(&raw_path, &patterns) && entry.path().is_file()
        })
        .collect();

    for (index, entry) in entries.iter().enumerate() {
        let source = entry.path();

        // 将 transform 字符串中的 "index" 替换为当前索引
        let transformed = transform
            // .replace("{{index}}", &index.to_string())
            // 将 transform 字符串中的 "index" 替换为当前索引（补0占位）
            .replace("{{index}}", &format!("{:02}", index))
            .replace("{{name}}", entry.file_name().to_string_lossy().as_ref())
            .replace("{{path}}", source.to_str().unwrap_or(""));

        println!("转换结果: {}", transformed);

        // 流式写入到缓冲区，除了最后一行不添加换行符
        if index == entries.len() - 1 {
            write!(writer, "{}", transformed).context("写入文件失败")?;
        } else {
            writeln!(writer, "{}", transformed).context("写入文件失败")?;
        }
    }

    // 确保所有数据都被写入到文件
    writer.flush().context("刷新缓冲区失败")?;

    Ok(())
}

fn ignored(path: &Path, patterns: &[Pattern]) -> bool {
    let path_str = path.to_string_lossy();

    for pattern in patterns {
        if pattern.matches(&path_str) {
            return true;
        }
        if let Some(filename) = path.file_name() {
            if pattern.matches(&filename.to_string_lossy()) {
                return true;
            }
        }

        for component in path.components() {
            if pattern.matches(&component.as_os_str().to_string_lossy()) {
                return true;
            }
        }
    }

    false
}
