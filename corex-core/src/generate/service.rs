use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::Path,
};

use anyhow::{Context, Result};
use rand::RngExt;
use uuid::Uuid;
use walkdir::{DirEntry, WalkDir};

use crate::generate::schema::{Args, PathArgs, UuidArgs};
use crate::utils::{notify, verifier, Filter};

#[derive(Debug, Clone)]
pub struct Output {
    pub path: Option<std::path::PathBuf>,
    pub items: u64,
    pub value: Option<String>,
}

/// 生成安全的 CVID (加密随机数，符合 GUID v4 标准)
///
/// - 生成 16 字节随机数组
/// - 设置版本位（第 6 字节）：(byte6 & 0x0f) | 0x40
/// - 设置变体位（第 8 字节）：(byte8 & 0x3f) | 0x80
/// - 转换为十六进制字符串并大写
pub fn generate_secure_cvid() -> String {
    let mut array = [0u8; 16];
    rand::rng().fill(&mut array);

    array[6] = (array[6] & 0x0f) | 0x40;
    array[8] = (array[8] & 0x3f) | 0x80;

    array
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<String>()
}

pub fn run(args: &Args) -> Result<()> {
    match args {
        Args::Uuid(uuid_args) => {
            uuid_task(uuid_args);
            Ok(())
        }
        Args::Cvid(_) => {
            let out = execute(args)?;
            if let Some(value) = out.value {
                println!("{value}");
            }
            Ok(())
        }
        Args::Path(_) => match execute(args) {
            Ok(_) => {
                let _ = notify::success("路径生成成功", "操作已成功完成");
                Ok(())
            }
            Err(e) => {
                let _ = notify::error("路径生成失败", &format!("生成过程中发生错误: {e}"));
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
                value: None,
            })
        }
        Args::Uuid(uuid_args) => {
            uuid_task(uuid_args);
            Ok(Output {
                path: None,
                items: uuid_args.count as u64,
                value: None,
            })
        }
        Args::Cvid(_) => Ok(Output {
            path: None,
            items: 1,
            value: Some(generate_secure_cvid()),
        }),
    }
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
            let _ = notify::error("路径生成失败", &format!("生成过程中发生错误: {e}"));
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

#[cfg(test)]
mod secrets_tests {
    use super::*;

    #[test]
    fn cvid_is_v4_uppercase_hex() {
        let cvid = generate_secure_cvid();
        assert_eq!(cvid.len(), 32);
        assert!(cvid.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(cvid.chars().all(|c| !c.is_ascii_lowercase()));
        let bytes = (0..16)
            .map(|i| u8::from_str_radix(&cvid[i * 2..i * 2 + 2], 16).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(bytes[6] & 0xf0, 0x40);
        assert_eq!(bytes[8] & 0xc0, 0x80);
    }
}
