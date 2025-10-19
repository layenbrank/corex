use crate::remove::controller::Args;
use std::{env, fs, path::Path};
use walkdir::WalkDir;

pub fn run(args: &Args) {
	let source = env::current_dir().expect("无法获取当前工作目录");
	let target: &Path = Path::new(&args.target);
	let recursive = args.recursive;
	let target_path = source.join(target);

	if !target_path.exists() {
		eprintln!("目标路径不存在: {:?}", target_path);
		return;
	}

	if recursive {
		for entry in WalkDir::new(&source) {
			match entry {
				Ok(entry) => {
					let path = entry.path();

					if path.file_name() == target_path.file_name() {
						if path.is_file() {
							if let Err(e) = fs::remove_file(path) {
								eprintln!("移除文件 {:?} 失败: {:?}", path, e);
							} else {
								println!("已移除文件 {:?}", path);
							}
						} else if path.is_dir() {
							if let Err(e) = fs::remove_dir_all(path) {
								eprintln!("移除目录 {:?} 失败: {:?}", path, e);
							} else {
								println!("已移除目录 {:?}", path);
							}
						}
					}
				}
				Err(e) => eprintln!("读取条目失败: {:?}", e),
			}
		}
	} else {
		if target_path.is_file() {
			if let Err(e) = fs::remove_file(&target_path) {
				eprintln!("移除文件 {:?} 失败: {:?}", target_path, e);
			} else {
				println!("已移除文件 {:?}", target_path);
			}
		} else if target_path.is_dir() {
			if let Err(e) = fs::remove_dir(&target_path) {
				eprintln!("移除目录 {:?} 失败: {:?}", target_path, e);
			} else {
				println!("已移除目录 {:?}", target_path);
			}
		} else {
			eprintln!("目标路径不存在: {:?}", target_path);
		}
	}
}
