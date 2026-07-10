use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::Path,
};

use anyhow::{Context, Result};
use uuid::Uuid;
use walkdir::{DirEntry, WalkDir};

use crate::generate::schema::{Args, FileArgs, PathArgs, UuidArgs};
use crate::generate::template::engine;
use crate::utils::{notify, verifier, Filter};

#[derive(Debug, Clone)]
pub struct Output {
    pub path: Option<std::path::PathBuf>,
    pub items: u64,
}

pub fn run(args: &Args) -> Result<()> {
    match args {
        Args::Uuid(uuid_args) => {
            uuid_task(uuid_args);
            Ok(())
        }
        _ => match execute(args) {
            Ok(_) => {
                let msg = match args {
                    Args::Path(_) => "路径生成成功",
                    Args::File(_) => "文件生成成功",
                    Args::Uuid(_) => unreachable!(),
                };
                let _ = notify::success(msg, "操作已成功完成");
                Ok(())
            }
            Err(e) => {
                let _ = notify::error("文件生成失败", &format!("生成过程中发生错误: {}", e));
                Err(e)
            }
        },
    }
}

pub fn execute(args: &Args) -> Result<Output> {
    match args {
        Args::Path(path_args) => {
            let (path, items) = path_task_streaming(path_args)?;
            Ok(Output {
                path: Some(path),
                items,
            })
        }
        Args::Uuid(uuid_args) => {
            uuid_task(uuid_args);
            Ok(Output {
                path: None,
                items: uuid_args.count as u64,
            })
        }
        Args::File(file_args) => {
            file_task(file_args)?;
            Ok(Output {
                path: Some(std::path::PathBuf::from(&file_args.to)),
                items: 1,
            })
        }
    }
}

pub fn file_task(args: &FileArgs) -> Result<()> {
    let hb = engine()?;

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
    match path_task_streaming(args) {
        Ok(_) => {
            let _ = notify::success("路径生成成功", "路径生成操作已成功完成");
        }
        Err(e) => {
            let _ = notify::error("文件生成失败", &format!("生成过程中发生错误: {e}"));
            return Err(e);
        }
    }
    Ok(())
}

/// 路径列表生成（流式写入），返回输出路径与条目数
pub fn path_task_streaming(args: &PathArgs) -> Result<(std::path::PathBuf, u64)> {
    let from = Path::new(&args.from);
    let to = Path::new(&args.to);

    if to.is_dir() {
        return Err(anyhow::anyhow!("目标路径应是一个文件路径!"));
    }

    let file = if let Some(to_str) = to.to_str() {
        if verifier::file(to_str).is_err() {
            File::create(to)?
        } else {
            OpenOptions::new().write(true).truncate(true).open(to)?
        }
    } else {
        File::create(to)?
    };

    let mut writer = BufWriter::new(file);
    let filter = Filter::new(&args.includes, &args.excludes);

    let mut entries: Vec<_> = WalkDir::new(from)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|entry| {
            let raw_path = entry.path().strip_prefix(from).unwrap_or(entry.path());
            !filter.is_filtered(raw_path) && entry.path().is_file()
        })
        .collect();

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
        match ext_a.cmp(&ext_b) {
            std::cmp::Ordering::Equal => {
                let name_a = a.file_name().to_string_lossy();
                let name_b = b.file_name().to_string_lossy();
                name_a.cmp(&name_b)
            }
            other => other,
        }
    });

    let pad_width = entries.len().to_string().len();
    let mut items = 0u64;

    for (key, entry) in entries.iter().enumerate() {
        let transformed = path_transform_line(
            &args.transform,
            entry,
            key + args.index,
            pad_width,
            &args.uppercase,
            &args.separator,
            from,
        )?;

        if !crate::runtime::is_quiet() && !crate::runtime::is_json_output() {
            println!("转换结果: {transformed}");
        }

        if key == entries.len().saturating_sub(1) {
            write!(writer, "{transformed}").context("写入文件失败")?;
        } else {
            writeln!(writer, "{transformed}").context("写入文件失败")?;
        }
        items += 1;
    }

    writer.flush().context("刷新缓冲区失败")?;
    Ok((to.to_path_buf(), items))
}

/// 单行路径模板转换（供流式 Pipeline 复用）
pub fn path_transform_line(
    transform: &str,
    entry: &DirEntry,
    index: usize,
    pad_width: usize,
    uppercase: &[String],
    separator: &str,
    from: &Path,
) -> Result<String> {
    let mut out = transform.to_string();
    let extension = entry
        .path()
        .extension()
        .unwrap_or_default()
        .to_string_lossy();
    let filename = entry.file_name().to_string_lossy();
    let relative = entry.path().strip_prefix(from).unwrap_or(entry.path());
    let dirpart = relative
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let fullpath = if dirpart.is_empty() {
        filename.to_string()
    } else {
        let sep = if !separator.is_empty() {
            separator
        } else {
            std::path::MAIN_SEPARATOR_STR
        };
        format!("{dirpart}{sep}{filename}")
    };

    let index_str = format!("{:0pad_width$}", index, pad_width = pad_width);

    let replacements = [
        ("{{index}}", index_str),
        (
            "{{filename}}",
            if uppercase.contains(&"filename".to_string()) {
                filename.to_uppercase()
            } else {
                filename.to_string()
            },
        ),
        (
            "{{extension}}",
            if uppercase.contains(&"extension".to_string()) {
                extension.to_uppercase()
            } else {
                extension.to_string()
            },
        ),
        (
            "{{path}}",
            if uppercase.contains(&"path".to_string()) {
                dirpart.to_uppercase()
            } else {
                dirpart.to_string()
            },
        ),
        (
            "{{fullpath}}",
            if uppercase.contains(&"fullpath".to_string()) {
                fullpath.to_uppercase()
            } else {
                fullpath
            },
        ),
    ];

    for (key, value) in replacements {
        out = out.replace(key, &value);
    }

    if !separator.is_empty() {
        out = out.replace('\\', separator).replace('/', separator);
    }

    Ok(out)
}

// legacy path_task body removed — see path_task_streaming
