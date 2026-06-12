use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::Path,
};

use anyhow::{Context, Result};
use uuid::Uuid;
use walkdir::{DirEntry, WalkDir};

use crate::generate::schema::{Args, FileArgs, PathArgs, UuidArgs};
use crate::generate::template::create_handlebars;
use crate::utils::{ignore::Filter, notify, verifier};

pub fn run(args: &Args) -> Result<()> {
    match args {
        Args::Path(path_args) => match path_task(path_args) {
            Ok(_) => {
                let _ = notify::success("路径生成成功", "路径生成操作已成功完成");
            }
            Err(e) => {
                let _ = notify::error("文件生成失败", &format!("生成过程中发生错误: {}", e));
                return Err(e);
            }
        },
        Args::Uuid(uuid_args) => uuid_task(uuid_args),
        Args::File(file_args) => match file_task(file_args) {
            Ok(_) => {
                let _ = notify::success("文件生成成功", "文件生成操作已成功完成");
            }
            Err(e) => {
                let _ = notify::error("文件生成失败", &format!("生成过程中发生错误: {}", e));
                return Err(e);
            }
        },
    }
    Ok(())
}

pub fn file_task(args: &FileArgs) -> Result<()> {
    let hb = create_handlebars()?;

    // 构建模板数据
    let mut data = serde_json::Map::new();
    for (k, v) in &args.variable {
        data.insert(k.clone(), serde_json::Value::String(v.clone()));
    }

    let rendered = if let Some(tpl_path) = &args.template {
        let template_content = fs::read_to_string(tpl_path)?;
        hb.render_template(&template_content, &data)?
    } else if let Some(fragment) = &args.fragment {
        hb.render_template(fragment, &data)?
    } else {
        anyhow::bail!("必须指定 --template 或 --fragment");
    };

    if let Some(parent) = std::path::Path::new(&args.to).parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&args.to, rendered)?;

    let _ = crate::utils::notify::success("文件生成成功", &format!("已生成: {}", args.to));
    Ok(())
}

pub fn uuid_task(args: &UuidArgs) {
    for _ in 0..args.count {
        let id = Uuid::new_v4();
        if args.uppercase {
            println!("{}", id.to_string().to_uppercase());
        } else {
            println!("{}", id);
        }
    }
}

pub fn path_task(args: &PathArgs) -> Result<()> {
    // 这里是具体的实现逻辑
    let from = Path::new(&args.from);
    let to = Path::new(&args.to);
    let transform = args.transform.clone();
    let excludes = args.excludes.clone();
    let includes = args.includes.clone();
    let separator = args.separator.clone();
    let index = args.index;
    let uppercase = args.uppercase.clone();

    if to.is_dir() {
        return Err(anyhow::anyhow!("目标路径应是一个文件路径!"));
    }

    // 创建或清空文件，然后创建缓冲写入器
    let file = if let Some(to_str) = to.to_str() {
        if verifier::file(to_str).is_err() {
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

    // 创建过滤器（白名单 + 黑名单）
    let filter = Filter::new(&includes, &excludes);

    let mut entries: Vec<_> = WalkDir::new(from)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|entry| {
            let raw_path = entry.path().strip_prefix(from).unwrap_or(entry.path());
            !filter.is_filtered(raw_path) && entry.path().is_file()
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
            count,
            uppercase: uppercase.clone(),
            separator: separator.clone(),
            from: from.to_path_buf(),
        };
        let transformed = replacement.run()?;

        println!("转换结果: {}", transformed);

        // 流式写入到缓冲区，除了最后一行不添加换行符
        if key == entries.len() - 1 {
            write!(writer, "{}", transformed).context("写入文件失败")?;
        } else {
            writeln!(writer, "{}", transformed).context("写入文件失败")?;
        }
    }

    // 确保所有数据都被写入到文件
    writer.flush().context("刷新缓冲区失败")?;

    anyhow::Ok(())
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
