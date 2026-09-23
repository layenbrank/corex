//! 端点发现文件：`<数据目录>/endpoint.json`。
//!
//! 连接方过去只能**猜**端点是哪一个：Windows 上写死 `\\.\pipe\corex`，Unix 上还得先
//! 复刻一遍 [`data_dir`] 的平台规则才拼得出 `<data>/corex.sock`；配置里改过
//! `socket_path`（或用 `--socket` 起过 daemon）时根本猜不到。
//!
//! 于是 daemon 在开始服务前把**它到底监听在哪**写下来，退出时删掉；连接方读到的
//! 是事实而不是约定。文件不在（没有 daemon 在跑）就退回平台默认端点，那是老行为。
//!
//! 这里只负责「这份记录长什么样、怎么读写」。谁该用哪种优先级去找端点见
//! [`crate::transport::find_endpoint`]（连接方）与
//! [`crate::transport::resolve_endpoint`]（监听方）。
//!
//! [`data_dir`]: crate::transport::data_dir

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 文件名，位于数据目录下。
pub const FILE: &str = "endpoint.json";

/// 本模块写出的格式版本。读到别的版本就当文件不存在——宁可退回平台默认，
/// 也不要按旧字段去猜。
pub const FORMAT: u32 = 1;

/// 端点种类。
///
/// 由平台决定而不由字符串猜：Windows 上端点只能是命名管道
/// （[`crate::transport::resolve_endpoint`] 会拒掉文件路径），Unix 上只能是 socket。
/// 写进记录是因为连接方需要据此挑选连接 API，而不想自己也判一遍平台。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Pipe,
    Socket,
}

#[cfg(windows)]
const fn kind() -> Kind {
    Kind::Pipe
}

#[cfg(not(windows))]
const fn kind() -> Kind {
    Kind::Socket
}

/// 连接方看到的端点形态：有记录就听记录的（那是 daemon 实际监听的形态），
/// 没有记录就是各平台默认的那一种。
pub fn kind_of(data: &Path) -> Kind {
    discover(data).map_or_else(kind, |record| record.kind)
}

impl Kind {
    /// 写进 JSON 与记录里的写法。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pipe => "pipe",
            Self::Socket => "socket",
        }
    }
}

/// 一份端点记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// 格式版本；读方不认识就忽略整份文件。
    pub version: u32,
    /// 监听方进程号。只用于排错（“是谁占着”），不承担存活性判断——pid 会被复用。
    pub pid: u32,
    /// 实际监听的端点。
    pub endpoint: PathBuf,
    pub kind: Kind,
    /// token **文件**的位置，仅当 token 本身来自文件时才有。
    ///
    /// 来自 `COREX_TOKEN` 或配置 `[daemon].token` 时是 `None`：那两处的值属于
    /// 调用方，不该被复制进一个默认权限的文件里。此时连接方得自己拿到 token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_file: Option<PathBuf>,
}

impl Record {
    /// 描述一个正在监听的端点。
    pub fn new(endpoint: impl Into<PathBuf>, token_file: Option<PathBuf>) -> Self {
        Self {
            version: FORMAT,
            pid: std::process::id(),
            endpoint: endpoint.into(),
            kind: kind(),
            token_file,
        }
    }
}

/// 写下记录（监听方开始服务前调）。
pub fn publish(data: &Path, record: &Record) -> std::io::Result<()> {
    let path = data.join(FILE);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(record).map_err(std::io::Error::other)?;
    std::fs::write(&path, text)
}

/// 删掉记录（监听方退出时调）。文件本来就不在不算失败。
pub fn retract(data: &Path) {
    let _ = std::fs::remove_file(data.join(FILE));
}

/// 读记录。文件缺失、JSON 损坏或版本不认识都是 `None`。
///
/// 三种情况都归到「没有」是有意的：这份文件只是便利设施，读不到就该退回平台默认端点，
/// 而不是让连接方起不来。真正连不上的错误会在连接时以它自己的面貌出现。
///
/// ⚠️ 记录可能是**残留**的（daemon 崩溃时来不及删）。这里不做存活性判断，也不该由
/// 连接方据此断定「daemon 在跑」——那件事的问法是去 ping 一次。
pub fn discover(data: &Path) -> Option<Record> {
    let text = std::fs::read_to_string(data.join(FILE)).ok()?;
    let record: Record = serde_json::from_str(&text).ok()?;
    (record.version == FORMAT).then_some(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> tempfile::TempDir {
        tempfile::tempdir().expect("临时目录")
    }

    #[test]
    fn publish_then_discover_round_trips() {
        let dir = temp();
        let record = Record::new(r"\\.\pipe\corex-test", Some(PathBuf::from("/tmp/token")));
        publish(dir.path(), &record).expect("写入记录");

        let back = discover(dir.path()).expect("读回记录");
        assert_eq!(back, record);
        assert_eq!(back.pid, std::process::id());
        assert_eq!(back.version, FORMAT);
    }

    /// `token_file` 没有来源时不该出现在文件里——省得读方以为「没有就是默认路径」。
    #[test]
    fn token_file_is_omitted_when_absent() {
        let dir = temp();
        publish(dir.path(), &Record::new(r"\\.\pipe\corex-test", None)).expect("写入记录");

        let text = std::fs::read_to_string(dir.path().join(FILE)).expect("读文件");
        assert!(!text.contains("token_file"), "{text}");
        assert_eq!(discover(dir.path()).expect("读回记录").token_file, None);
    }

    #[test]
    fn missing_file_is_not_an_error() {
        let dir = temp();
        assert_eq!(discover(dir.path()), None);

        // 也不该顺手建出一个空文件来。
        retract(dir.path());
        assert!(!dir.path().join(FILE).exists());
    }

    #[test]
    fn broken_or_future_files_are_ignored() {
        let dir = temp();
        let path = dir.path().join(FILE);

        std::fs::write(&path, "{ 这不是 JSON").expect("写坏文件");
        assert_eq!(discover(dir.path()), None);

        std::fs::write(
            &path,
            r#"{"version":99,"pid":1,"endpoint":"x","kind":"pipe"}"#,
        )
        .expect("写未来版本");
        assert_eq!(discover(dir.path()), None);
    }

    #[test]
    fn retract_removes_the_file() {
        let dir = temp();
        publish(dir.path(), &Record::new("x", None)).expect("写入记录");
        assert!(dir.path().join(FILE).exists());

        retract(dir.path());
        assert!(!dir.path().join(FILE).exists());
    }

    /// 没有记录时退回平台默认——`corex paths` 在 daemon 没跑时也得给得出答案。
    #[test]
    fn kind_without_a_record_is_the_platform_default() {
        let dir = temp();
        assert_eq!(kind_of(dir.path()), kind());
    }

    /// 有记录时以记录为准：报的是 daemon 实际监听的形态，而不是平台默认。
    #[test]
    fn kind_follows_the_record() {
        let dir = temp();
        let recorded = match kind() {
            Kind::Pipe => Kind::Socket,
            Kind::Socket => Kind::Pipe,
        };
        std::fs::write(
            dir.path().join(FILE),
            format!(
                r#"{{"version":{FORMAT},"pid":1,"endpoint":"x","kind":"{}"}}"#,
                recorded.as_str()
            ),
        )
        .expect("写记录");

        assert_eq!(kind_of(dir.path()), recorded);
    }

    /// 记录里的名字就是 `as_str` 那两个字面量，读方不必再翻译一遍。
    #[test]
    fn kind_names_match_the_record() {
        for kind in [Kind::Pipe, Kind::Socket] {
            let json = serde_json::to_string(&kind).expect("序列化");
            assert_eq!(json.trim_matches('"'), kind.as_str());
        }
    }
}
