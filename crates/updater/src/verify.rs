//! SHA-256 哈希与 `SHA256SUMS` 清单处理。

use crate::error::{Error, Result};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

/// `bytes` 的小写十六进制 SHA-256。
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(Sha256::digest(bytes).iter())
}

/// 文件的小写十六进制 SHA-256，按固定大小分块流式读取。
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(hex(hasher.finalize().iter()))
}

/// 把字节渲染成小写十六进制。
fn hex<'a>(bytes: impl Iterator<Item = &'a u8>) -> String {
    bytes.map(|b| format!("{b:02x}")).collect()
}

/// `s` 看起来像不像一个 64 字符的十六进制 SHA-256 摘要。
pub fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// 去掉 GitHub 在资源 `digest` 字段里用的 `sha256:` 前缀。
pub fn strip_algo(digest: &str) -> &str {
    digest.split_once(':').map_or(digest, |(_, value)| value)
}

/// `actual` 与 `from` 给出的摘要不一致就报错。
pub fn require_digest(from: &str, expected: &str, actual: &str) -> Result<()> {
    if expected.eq_ignore_ascii_case(actual) {
        return Ok(());
    }
    Err(Error::ChecksumMismatch {
        from: from.to_string(),
        expected: expected.to_ascii_lowercase(),
        actual: actual.to_ascii_lowercase(),
    })
}

/// 读取清单行的一种写法。
type LineParser = fn(&str) -> Option<(String, String)>;

/// sums 清单可能采用的行格式，依次尝试。
///
/// 该清单由我们自己的 CI 生成，但也会从字节不受我们控制的 release 上读回来，
/// 所以 `self_update` 接受过的每一种形式都要认得，而不是假定只有一种排版。
const LINE_PARSERS: [LineParser; 2] = [parse_bsd_line, parse_coreutils_line];

/// 把 `SHA256SUMS` 清单解析成 `文件名 -> 小写十六进制摘要`。
///
/// 接受 coreutils 的文本与二进制（`*name`）两种模式、BSD 的
/// `SHA256 (name) = hex` 标签形式、`./` 与 `\` 转义、`#` 注释和空行。
/// 条目以文件名为键，这正是 `corex update` 需要的：
/// 清单列的是包内文件（`corex.exe`、`corex-daemon.exe` 等）。
///
/// 没有解析器认得的行会被跳过，所以未知格式退化为
/// “该来源没有这条记录”，而不是让更新失败。
pub fn parse_sums(text: &str) -> HashMap<String, String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| LINE_PARSERS.iter().find_map(|parse| parse(line)))
        .collect()
}

/// 从 `.sha256` 旁车文件里取摘要，它只有一行 `hex  name`。
pub fn parse_sidecar(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .and_then(|line| {
            let (digest, _) = split_first_token(line)?;
            Some(digest.trim_start_matches('\\').to_ascii_lowercase())
        })
        .filter(|digest| is_sha256(digest))
}

/// 清单条目的最后一段路径（条目可能带目录）。
pub(crate) fn basename(name: &str) -> String {
    name.trim_start_matches('\\')
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .to_string()
}

/// 把一行拆成第一个空白分隔的 token 和剩余部分。
fn split_first_token(line: &str) -> Option<(&str, &str)> {
    let line = line.trim_start();
    let index = line.find(char::is_whitespace)?;
    let (head, tail) = line.split_at(index);
    Some((head, tail.trim_start()))
}

/// `SHA256 (file) = <hex>`，BSD `shasum` 的标签形式。
fn parse_bsd_line(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("SHA256")?.trim_start();
    let rest = rest.strip_prefix('(')?;
    let (name, rest) = rest.split_once(')')?;
    let digest = rest.trim_start().strip_prefix('=')?.trim();
    if !is_sha256(digest) {
        return None;
    }
    Some((basename(name), digest.to_ascii_lowercase()))
}

/// `<hash>  <file>` 与 `<hash> *<file>`，即 coreutils 的文本与二进制模式。
///
/// 前导 `\` 表示文件名里有换行符，`*` 表示二进制
/// 模式；两者都会被剥掉，而不是当作名字的一部分。
fn parse_coreutils_line(line: &str) -> Option<(String, String)> {
    let (digest, rest) = split_first_token(line)?;
    let digest = digest.trim_start_matches('\\');
    if !is_sha256(digest) {
        return None;
    }
    let name = rest.trim_start_matches('*');
    if name.is_empty() {
        return None;
    }
    Some((basename(name), digest.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_known_vector() {
        // 空输入的 SHA-256，标准测试向量。
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn reads_the_coreutils_manifest_the_workflow_writes() {
        // publish-release.yml 产出的形态：`<hash>  <name>`。
        let text = "\
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  corex.exe
ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  corex-daemon.exe
";
        let sums = parse_sums(text);
        assert_eq!(sums.len(), 2);
        assert_eq!(
            sums["corex.exe"],
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert!(sums.contains_key("corex-daemon.exe"));
    }

    #[test]
    fn tolerates_the_other_manifest_shapes() {
        let text = "\
# generated by CI

\\e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 *./corex.exe
SHA256 (corex-daemon.exe) = BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD
not-a-digest  README.md
";
        let sums = parse_sums(text);
        assert_eq!(sums.len(), 2);
        assert!(sums.contains_key("corex.exe"));
        assert_eq!(
            sums["corex-daemon.exe"],
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(!sums.contains_key("README.md"));
    }

    #[test]
    fn parses_the_zip_sidecar() {
        let text = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  corex-v6.0.1-windows-x64.zip\n";
        assert_eq!(
            parse_sidecar(text).as_deref(),
            Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        );
        assert!(parse_sidecar("").is_none());
        assert!(parse_sidecar("garbage\n").is_none());
    }

    #[test]
    fn strips_the_github_digest_prefix() {
        assert_eq!(strip_algo("sha256:abcd"), "abcd");
        assert_eq!(strip_algo("abcd"), "abcd");
    }

    #[test]
    fn mismatch_reports_both_sides() {
        let err = require_digest("SHA256SUMS.txt", "aa", "bb").unwrap_err();
        let text = err.to_string();
        assert!(text.contains("SHA256SUMS.txt"), "{text}");
        assert!(text.contains("aa") && text.contains("bb"), "{text}");
        assert!(require_digest("s", "AB", "ab").is_ok());
    }

    #[test]
    fn identifies_sha256_shaped_strings() {
        assert!(is_sha256(&"a".repeat(64)));
        assert!(!is_sha256(&"a".repeat(63)));
        assert!(!is_sha256(&"z".repeat(64)));
    }
}
