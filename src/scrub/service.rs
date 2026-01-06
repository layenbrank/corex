use crate::{scrub::controller::Args, utils::scan::Scan};
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, thiserror::Error)]
pub enum ScrubError {
	#[error("路径不存在：{0}")]
	Exists(String),
}

pub fn run(args: &Args) {
	// let source = env::current_dir().expect("无法获取当前工作目录");
	let target: &Path = Path::new(&args.target);
	let file_name = target.file_name().expect("获取路径失败");
	let exists = target.exists();

	// 1. 验证目标路径存在性
	if !exists {
		eprintln!("路径不存在: {:?}", target);
		return;
	}

	// TODO: 开发环境-测试
	let dev = true;

	if dev {
		println!(
			"target file_name: {}",
			file_name.to_str().expect("路径提取失败")
		);
		let directory = Scan::directory(&target);
		println!("Scan directory path: {:?}", directory);

		let file = Scan::file(&target);
		println!("Scan file path: {:?}", file);

		// return;
	}

	// 2. 递归删除要求目标是目录
	if args.recursive && !target.is_dir() {
		eprintln!("递归删除仅支持目录: {:?}", target);
		return;
	}

	match args.recursive {
		true => depth(target, file_name),
		false => shallow(target),
	}
}

fn depth(target: &Path, file_name: &OsStr) {
	let entries = WalkDir::new(&target)
		.into_iter()
		.filter_map(|entry| entry.ok());
	// .filter(|entry| !is_ignored(entry.path()))

	for entry in entries {
		let filename = entry.path().file_name();

		match filename {
			Some(filename) => {
				println!("filename: {:?}", filename);
				if filename == file_name {
					// remove_path(entry.path(), entry.path().is_dir());
				}
			}
			None => {
				eprintln!("获取文件名失败");
			}
		}
	}

	// for entry in entries {
	// 	let path = entry;
	// 	remove_path(path, path.is_dir())
	// }
	//
	// remove_path(target, target.is_dir())
}

fn shallow(target: &Path) {
	remove_path(target, true)
}

// 优化忽略规则 (避免遍历常见目录)
fn is_ignored(path: &Path) -> bool {
	path.file_name()
		.and_then(|name| name.to_str())
		.map(|s| s.starts_with('.') || ["node_modules", ".git"].contains(&s))
		.unwrap_or(false)
}

fn remove_path(path: &Path, is_dir: bool) {
	let removed = match is_dir {
		true => fs::remove_dir(path),
		false => fs::remove_file(path),
	};

	match removed {
		Ok(removed) => (),
		Err(e) => eprintln!("删除失败: {:?} - {}", path, e),
	}
}
