use std::path::Path;
use walkdir::WalkDir;

pub struct Scan;

#[derive(Debug)]
pub struct ScanFile {
    pub path: String,
    pub size: u64,
}

#[derive(Debug)]
pub struct ScanCount {
    pub file: u64,
    pub dir: u64,
    pub count: u64,
}

#[derive(Debug)]
pub struct ScanDir {
    pub size: u64,
    pub collective: ScanCount,
}

impl Scan {
    pub fn file(path: &Path) -> Result<ScanFile, &'static str> {
        if !path.is_file() {
            return Err("Not a file");
        }
        let scan = ScanFile {
            path: path.to_string_lossy().into_owned(),
            size: path.metadata().map(|m| m.len()).unwrap_or(0),
        };

        Ok(scan)
    }

    pub fn directory(path: &Path) -> Result<ScanDir, &'static str> {
        if !path.is_dir() {
            return Err("Not a directory");
        }

        let mut scan = ScanDir {
            size: 0,
            collective: ScanCount {
                file: 0,
                dir: 0,
                count: 0,
            },
        };

        let entries = WalkDir::new(path);

        entries
            .into_iter()
            .filter(|entry| entry.is_ok())
            .for_each(|entry| match entry {
                Ok(entry) => {
                    let metadata = entry.metadata().unwrap();
                    if metadata.is_file() {
                        scan.collective.file += 1;
                        scan.size += metadata.len();
                    } else if metadata.is_dir() {
                        scan.collective.dir += 1;
                    }
                    scan.collective.count += 1;
                }
                Err(e) => eprintln!("Error reading entry: {:?}", e),
            });

        Ok(scan)
    }
}
