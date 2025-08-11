use std::path::Path;

#[derive(Debug, Clone)]
pub struct Verifier {}

impl Verifier {
    pub fn path(path: &str) -> Result<String, &'static str> {
        if path == "." || Path::new(path).exists() {
            println!("{}", path);
            Ok(path.into())
        } else {
            Err("未找到指定路径，请检查路径是否正确！")
        }
    }
}
