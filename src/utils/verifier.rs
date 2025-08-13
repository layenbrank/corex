use std::path::Path;

#[derive(Debug, Clone)]
pub struct Verifier;

impl Verifier {
    /// 验证路径是否存在
    pub fn path(path: &str) -> Result<String, &'static str> {
        if path == "." || Path::new(path).exists() {
            Ok(path.into())
        } else {
            Err("未找到指定路径，请检查路径是否正确！")
        }
    }

    /// 验证目录是否存在
    pub fn dir(path: &str) -> Result<String, &'static str> {
        if Path::new(path).is_dir() {
            Ok(path.into())
        } else {
            Err("未找到指定目录，请检查路径是否正确！")
        }
    }

    /// 验证文件是否存在
    pub fn file(path: &str) -> Result<String, &'static str> {
        if Path::new(path).is_file() {
            Ok(path.into())
        } else {
            Err("未找到指定文件，请检查路径是否正确！")
        }
    }
}
