use std::{fs, io, os, path};

fn run() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        println!("Usage: {} <path>", args[0]);
        return;
    }
    let path = path::Path::new(&args[1]);
    if !path.exists() {
        println!("Error: {} does not exist", args[1]);
        return;
    }
    if !path.is_dir() {
        println!("Error: {} is not a directory", args[1]);
        return;
    }
    for entry in fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        let file_type = entry.file_type().unwrap();
        if file_type.is_file() {
            println!("{}", entry.path().display());
        }
    }

    /**
     * 追加写入
     */
    let file = OpenOptions::new()
        .append(true)
        .create(true)
        .open("./cache/process_copy.txt")?;

    let mut writer = BufWriter::new(file);

    // 写入三个路径并换行
    writer.write_all(source_path.as_os_str().as_encoded_bytes())?;
    writer.write_all(b"\n")?; // 换行符
    writer.write_all(real_path.as_os_str().as_encoded_bytes())?;
    writer.write_all(b"\n")?;
    writer.write_all(target_path.as_os_str().as_encoded_bytes())?;
    writer.write_all(b"\n\n")?; // 两个换行符作为分隔

    writer.flush()?; // 确保缓冲区数据写入磁盘
}

fn recursive(dir_path: &Path) {
    for entry in fs::read_dir(dir_path).unwrap() {
        let entry = entry.unwrap();
        let file_type = entry.file_type().unwrap();
        if file_type.is_file() {
            println!("entry {}", entry.path().display());
        } else if file_type.is_dir() {
            recursive(entry.path().as_path());
        }
    }
}
