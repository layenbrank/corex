use crate::generate::controller::{Args, PathArgs};
use crate::utils::verifier::Verifier;
use anyhow::{Context, Result};
use glob::Pattern;
use notify_rust::Notification;
use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::Path,
};
use walkdir::{DirEntry, WalkDir};

pub fn run(args: &Args) {
    match args {
        Args::Path(path_args) => match path_task(&path_args) {
            Ok(_) => {
                Notification::new()
                    .summary("路径生成成功")
                    .body("路径生成操作已成功完成")
                    .icon("dialog-information")
                    .show()
                    .expect("显示成功通知失败");
            }
            Err(e) => {
                Notification::new()
                    .summary("文件复制失败")
                    .body(&format!("复制过程中发生错误: {}", e))
                    .icon("dialog-error")
                    .show()
                    .expect("显示错误通知失败");
            }
        },
    }
}

pub fn path_task(args: &PathArgs) -> Result<()> {
    // 这里是具体的实现逻辑
    let from = Path::new(&args.from);
    let to = Path::new(&args.to);
    let transform = args.transform.clone();
    let ignores = args.ignores.clone();
    let separator = args.separator.clone();
    let index = args.index.clone();
    let uppercase = args.uppercase.clone();

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

    let mut entries: Vec<_> = WalkDir::new(from)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|entry| {
            let raw_path = entry.path().strip_prefix(from).unwrap_or(entry.path());
            !ignored(&raw_path, &patterns) && entry.path().is_file()
        })
        .collect();

    // 按文件扩展名进行字母排序
    entries.sort_by(|a, b| {
        let ext_a = a
            .path()
            .extension()
            .map(|ext| ext.to_string_lossy())
            .unwrap_or_default();
        let ext_b = b
            .path()
            .extension()
            .map(|ext| ext.to_string_lossy())
            .unwrap_or_default();

        // 先按扩展名排序，如果扩展名相同再按文件名排序
        match ext_a.cmp(&ext_b) {
            std::cmp::Ordering::Equal => {
                let name_a = a.file_name().to_string_lossy();
                let name_b = b.file_name().to_string_lossy();
                name_a.cmp(&name_b)
            }
            other => other,
        }
    });

    // 根据 entries 的长度动态计算补零位数 计算需要的位数：文件总数的位数
    let count = entries.len().to_string().len();

    for (key, value) in entries.iter().enumerate() {
        let replacement = Replacement {
            transform: transform.clone(),
            entry: value.clone(),
            index: key + index,
            count: count,
            uppercase: uppercase.clone(),
            separator: separator.clone(),
            from: from.to_path_buf(),
        };
        let transformed = replacement.run()?;

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

    anyhow::Ok(())
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

struct Replacement {
    transform: String,
    entry: DirEntry,
    index: usize,
    count: usize,
    uppercase: Vec<String>,
    separator: String,
    from: std::path::PathBuf,
}

impl Replacement {
    fn run(&self) -> Result<String> {
        let mut transform = self.transform.to_string();
        let extension = self
            .entry
            .path()
            .extension()
            .unwrap_or_default()
            .to_string_lossy();

        let filename = self.entry.file_name().to_string_lossy();
        let relative = self
            .entry
            .path()
            .strip_prefix(&self.from)
            .unwrap_or(self.entry.path());
        let dirpart = relative
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let fullpath = if dirpart.is_empty() {
            filename.to_string()
        } else {
            let sep = if !self.separator.is_empty() {
                &self.separator
            } else {
                std::path::MAIN_SEPARATOR_STR
            };
            format!("{}{}{}", dirpart, sep, filename)
        };

        let index = format!("{:0count$}", self.index, count = self.count);

        let replacements = vec![
            ("{{index}}", index),
            (
                "{{filename}}",
                if self.uppercase.contains(&"filename".to_string()) {
                    filename.to_uppercase()
                } else {
                    filename.to_string()
                },
            ),
            (
                "{{extension}}",
                if self.uppercase.contains(&"extension".to_string()) {
                    extension.to_uppercase()
                } else {
                    extension.to_string()
                },
            ),
            (
                "{{path}}",
                if self.uppercase.contains(&"path".to_string()) {
                    dirpart.to_uppercase()
                } else {
                    dirpart.to_string()
                },
            ),
            (
                "{{fullpath}}",
                if self.uppercase.contains(&"fullpath".to_string()) {
                    fullpath.to_uppercase()
                } else {
                    fullpath
                },
            ),
        ];

        for (key, value) in replacements {
            transform = transform.replace(key, &value);
        }

        if !self.separator.is_empty() {
            transform = transform
                .replace("\\", &self.separator) // Windows 路径分隔符
                .replace("/", &self.separator); // Unix 路径分隔符
        }

        anyhow::Ok(transform)
    }
}
