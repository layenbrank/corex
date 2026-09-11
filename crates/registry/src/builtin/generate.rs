//! 生成类动作：路径列表、uuid、cvid。

use crate::ActionRegistry;
use crate::builtin::filter::Filter;
use crate::builtin::util::{
    MAX_RANGE, Slice, confine_path, ensure_parent, opt_bool, opt_i64, opt_str, opt_strs,
    range_params, require_map, require_path, require_str,
};
use async_trait::async_trait;
use corex_core::{
    Action, ActionError, ActionMeta, Bucket, ExecutionContext, ParamSchema, PermissionSet,
    SchemaType, Unit, Value,
};
use rand::RngExt;
// `sha2` 与 `md-5` 共用同一个 `digest` 版本，所以这一个 trait 导入对三种算法都生效。
use sha2::Digest;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;
use walkdir::WalkDir;

pub fn generate_secure_cvid() -> String {
    let mut array = [0u8; 16];
    rand::rng().fill(&mut array);
    array[6] = (array[6] & 0x0f) | 0x40;
    array[8] = (array[8] & 0x3f) | 0x80;
    array.iter().map(|b| format!("{b:02X}")).collect()
}

pub struct GenerateUuid;
pub struct GenerateCvid;
pub struct GeneratePath;
pub struct GenerateTimestamp;

#[async_trait]
impl Action for GenerateUuid {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::NONE
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new("generate.uuid", "生成 UUID", "生成 UUID v4", Bucket::Data).with_params(
            vec![
                ParamSchema::new("count", SchemaType::Int, false).with_default(1),
                ParamSchema::new("uppercase", SchemaType::Bool, false).with_default(false),
            ],
        )
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let empty = BTreeMap::new();
        let map = params.as_map().unwrap_or(&empty);
        let count = opt_i64(map, "count", 1).max(1) as usize;
        let uppercase = opt_bool(map, "uppercase", false);
        let mut items = Vec::with_capacity(count);
        for _ in 0..count {
            let id = Uuid::new_v4().to_string();
            items.push(Value::Str(if uppercase { id.to_uppercase() } else { id }));
        }
        let mut out = BTreeMap::new();
        out.insert("items".into(), Value::Array(items.clone()));
        out.insert(
            "value".into(),
            items.first().cloned().unwrap_or(Value::Null),
        );
        Ok(Value::Map(out))
    }
}

#[async_trait]
impl Action for GenerateCvid {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::NONE
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "generate.cvid",
            "生成 CVID",
            "生成 GUID v4 大写 hex（CVID）",
            Bucket::Data,
        )
    }

    async fn execute(
        &self,
        _params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        Ok(Value::Str(generate_secure_cvid()))
    }
}

#[async_trait]
impl Action for GeneratePath {
    fn permissions(&self) -> PermissionSet {
        // 会枚举源目录（`from` + `includes`），因此要读文件系统。
        PermissionSet::FILESYSTEM
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "generate.path",
            "生成路径列表",
            "遍历目录并按模板写出路径列表",
            Bucket::Data,
        )
        .with_params(vec![
            ParamSchema::new("from", SchemaType::File, true),
            ParamSchema::new("to", SchemaType::File, true),
            ParamSchema::new("transform", SchemaType::Str, true),
            ParamSchema::new("index", SchemaType::Int, false).with_default(0),
            ParamSchema::new("separator", SchemaType::Str, false).with_default(""),
            ParamSchema::new("includes", SchemaType::Array, false),
            ParamSchema::new("excludes", SchemaType::Array, false),
            ParamSchema::new("uppercase", SchemaType::Array, false),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let from = confine_path(ctx, &require_path(map, "from")?)?;
        let to = confine_path(ctx, &require_path(map, "to")?)?;
        let transform = require_str(map, "transform")?;
        let index_start = opt_i64(map, "index", 0) as usize;
        let separator = opt_str(map, "separator").unwrap_or_default();
        let includes = opt_strs(map, "includes");
        let excludes = opt_strs(map, "excludes");
        let uppercase = opt_strs(map, "uppercase");

        if to.is_dir() {
            return Err(ActionError::InvalidParams(
                "目标路径应是一个文件路径".into(),
            ));
        }
        ensure_parent(&to)?;

        let filter = Filter::new(&includes, &excludes);
        let mut entries: Vec<_> = WalkDir::new(&from)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|entry| {
                let raw = entry.path().strip_prefix(&from).unwrap_or(entry.path());
                !filter.is_filtered(raw) && entry.path().is_file()
            })
            .collect();

        entries.sort_by(|a, b| {
            let ext_a = a
                .path()
                .extension()
                .map(|e| e.to_string_lossy())
                .unwrap_or_default();
            let ext_b = b
                .path()
                .extension()
                .map(|e| e.to_string_lossy())
                .unwrap_or_default();
            match ext_a.cmp(&ext_b) {
                std::cmp::Ordering::Equal => a
                    .file_name()
                    .to_string_lossy()
                    .cmp(&b.file_name().to_string_lossy()),
                other => other,
            }
        });

        let pad_width = entries.len().to_string().len().max(1);
        let spec = NameSpec {
            transform: &transform,
            pad_width,
            uppercase: &uppercase,
            separator: &separator,
        };
        let mut file = std::fs::File::create(&to)?;
        let mut items = 0u64;
        for (key, entry) in entries.iter().enumerate() {
            let line = spec.render(
                entry.path(),
                entry.file_name().to_string_lossy().as_ref(),
                key + index_start,
                &from,
            );
            if key + 1 == entries.len() {
                write!(file, "{line}")?;
            } else {
                writeln!(file, "{line}")?;
            }
            items += 1;
        }

        let mut out = BTreeMap::new();
        out.insert("path".into(), Value::File(to));
        out.insert("items".into(), Value::Int(items as i64));
        Ok(Value::Map(out))
    }
}

/// `generate.path` 如何渲染每一行输出。
///
/// 用一个值代替四个并列参数：命名策略每次运行只构建一次，
/// 之后每个条目复用。
struct NameSpec<'a> {
    /// 带 `{{name}}` / `{name}` 占位符的模板。
    transform: &'a str,
    /// `index` 占位符的补零宽度。
    pad_width: usize,
    /// 需要转大写的占位符名。
    uppercase: &'a [String],
    /// 路径分隔符的替换字符；为空则原样保留。
    separator: &'a str,
}

impl NameSpec<'_> {
    fn render(&self, entry_path: &Path, filename: &str, index: usize, from: &Path) -> String {
        let extension = entry_path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let relative = entry_path.strip_prefix(from).unwrap_or(entry_path);
        let dirpart = relative
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let fullpath = if dirpart.is_empty() {
            filename.to_string()
        } else {
            let sep = if self.separator.is_empty() {
                std::path::MAIN_SEPARATOR_STR
            } else {
                self.separator
            };
            format!("{dirpart}{sep}{filename}")
        };
        let index_str = format!("{:0pad_width$}", index, pad_width = self.pad_width);
        let filename_v = up(self.uppercase, "filename", filename);
        let extension_v = up(self.uppercase, "extension", &extension);
        let path_v = up(self.uppercase, "path", &dirpart);
        let fullpath_v = up(self.uppercase, "fullpath", &fullpath);
        let mut out = self.transform.to_string();
        // 优先 `{{name}}`，再试 `{name}`（单层大括号可避开指令 `{{ }}` 解析器的冲突）。
        for (key, val) in [
            ("index", index_str.as_str()),
            ("filename", filename_v.as_str()),
            ("extension", extension_v.as_str()),
            ("path", path_v.as_str()),
            ("fullpath", fullpath_v.as_str()),
        ] {
            out = out.replace(&format!("{{{{{key}}}}}"), val);
            out = out.replace(&format!("{{{key}}}"), val);
        }
        if !self.separator.is_empty() {
            out = out.replace(['\\', '/'], self.separator);
        }
        out
    }
}

fn up(uppercase: &[String], field: &str, value: &str) -> String {
    if uppercase.iter().any(|s| s == field) {
        value.to_uppercase()
    } else {
        value.to_string()
    }
}

#[async_trait]
impl Action for GenerateTimestamp {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::NONE
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "generate.timestamp",
            "生成时间戳",
            "生成当前时间戳字符串",
            Bucket::Data,
        )
        .with_params(vec![
            ParamSchema::new("format", SchemaType::Str, false).with_default("%Y-%m-%d %H:%M:%S"),
            ParamSchema::new("utc", SchemaType::Bool, false).with_default(false),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        use chrono::{Local, Utc};
        let empty = BTreeMap::new();
        let map = params.as_map().unwrap_or(&empty);
        let format = opt_str(map, "format").unwrap_or_else(|| "%Y-%m-%d %H:%M:%S".into());
        let utc = opt_bool(map, "utc", false);
        let (formatted, unix, iso8601) = if utc {
            let dt = Utc::now();
            (
                dt.format(&format).to_string(),
                dt.timestamp(),
                dt.to_rfc3339(),
            )
        } else {
            let dt = Local::now();
            (
                dt.format(&format).to_string(),
                dt.timestamp(),
                dt.to_rfc3339(),
            )
        };
        let mut out = BTreeMap::new();
        out.insert("value".into(), Value::Str(formatted));
        out.insert("unix".into(), Value::Int(unix));
        out.insert("iso8601".into(), Value::Str(iso8601));
        Ok(Value::Map(out))
    }
}

pub struct GenerateHash;
pub struct GenerateChunks;

/// 读取文件的缓冲区：1 MiB。
const READ_CHUNK: usize = 1024 * 1024;

/// `generate.hash` / `generate.chunks` 支持的摘要算法。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Algo {
    Sha256,
    Sha512,
    Md5,
}

impl Algo {
    fn parse(raw: Option<&str>) -> Result<Self, ActionError> {
        match raw
            .unwrap_or("sha256")
            .trim()
            .to_ascii_lowercase()
            .replace('-', "")
            .as_str()
        {
            "sha256" => Ok(Self::Sha256),
            "sha512" => Ok(Self::Sha512),
            "md5" => Ok(Self::Md5),
            other => Err(ActionError::InvalidParams(format!(
                "不支持的 algorithm: {other}（sha256 | sha512 | md5）"
            ))),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Sha256 => "sha256",
            Self::Sha512 => "sha512",
            Self::Md5 => "md5",
        }
    }

    fn hasher(self) -> Hasher {
        match self {
            Self::Sha256 => Hasher::Sha256(sha2::Sha256::new()),
            Self::Sha512 => Hasher::Sha512(sha2::Sha512::new()),
            Self::Md5 => Hasher::Md5(md5::Md5::new()),
        }
    }
}

/// 可增量喂入的摘要器：三种算法共用同一条读取路径，于是「流式」只需要写一遍。
enum Hasher {
    Sha256(sha2::Sha256),
    Sha512(sha2::Sha512),
    Md5(md5::Md5),
}

impl Hasher {
    fn update(&mut self, bytes: &[u8]) {
        match self {
            Self::Sha256(h) => h.update(bytes),
            Self::Sha512(h) => h.update(bytes),
            Self::Md5(h) => h.update(bytes),
        }
    }

    /// 小写十六进制摘要。
    fn hex(self) -> String {
        let digest: Vec<u8> = match self {
            Self::Sha256(h) => h.finalize().to_vec(),
            Self::Sha512(h) => h.finalize().to_vec(),
            Self::Md5(h) => h.finalize().to_vec(),
        };
        hex(&digest)
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// 把整段流式吃完，边读边报进度。
async fn hash_slice(
    slice: &mut Slice,
    algo: Algo,
    ctx: &ExecutionContext,
) -> Result<String, ActionError> {
    let total = slice.len();
    let mut hasher = algo.hasher();
    let mut buf = vec![0u8; READ_CHUNK];
    let mut done = 0u64;
    loop {
        let n = slice.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        done += n as u64;
        ctx.chunk(done, Some(total), Unit::Bytes);
    }
    Ok(hasher.hex())
}

/// 一遍读完文件，同时给出整段摘要与每一片的摘要。
///
/// 分片上传需要的正是这一张表：`index` / `offset` / `length` / `hex` 直接进 multipart，
/// 而整段摘要顺手就算出来了（同一批字节喂两个摘要器，不用再读第二遍）。
async fn chunk_plan(
    path: &Path,
    offset: u64,
    length: Option<u64>,
    chunk: u64,
    algo: Algo,
    ctx: &ExecutionContext,
) -> Result<Value, ActionError> {
    if chunk > MAX_RANGE {
        return Err(ActionError::InvalidParams(format!(
            "chunk {chunk} 超过 {MAX_RANGE} 字节上限"
        )));
    }
    let mut slice = Slice::open(path, offset, length).await?;
    let total = slice.len();
    let mut whole = algo.hasher();
    let mut buf = vec![0u8; chunk as usize];
    let mut done = 0u64;
    let mut items = Vec::new();

    loop {
        let mut filled = 0usize;
        while filled < buf.len() {
            let n = slice.read(&mut buf[filled..]).await?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        if filled == 0 {
            break;
        }
        let part = &buf[..filled];
        whole.update(part);
        let mut one = algo.hasher();
        one.update(part);

        items.push(Value::Map(BTreeMap::from([
            ("index".into(), Value::Int(items.len() as i64)),
            ("offset".into(), Value::Int((offset + done) as i64)),
            ("length".into(), Value::Int(filled as i64)),
            ("hex".into(), Value::Str(one.hex())),
        ])));
        done += filled as u64;
        ctx.chunk(done, Some(total), Unit::Bytes);
    }

    Ok(Value::Map(BTreeMap::from([
        ("algorithm".into(), Value::Str(algo.name().into())),
        ("size".into(), Value::Int(total as i64)),
        ("chunk".into(), Value::Int(chunk as i64)),
        ("total".into(), Value::Int(items.len() as i64)),
        ("hash".into(), Value::Str(whole.hex())),
        ("chunks".into(), Value::Array(items)),
    ])))
}

#[async_trait]
impl Action for GenerateHash {
    fn permissions(&self) -> PermissionSet {
        // 可以读文件（`path`）；纯文本输入不碰磁盘，但权限按最大的算。
        PermissionSet::FILESYSTEM
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "generate.hash",
            "生成摘要",
            "计算 sha256 / sha512 / md5：文件（可只取一段）或文本，流式读取",
            Bucket::Data,
        )
        .with_params(vec![
            ParamSchema::new("algorithm", SchemaType::Str, false)
                .with_default("sha256")
                .with_description("sha256 | sha512 | md5"),
            ParamSchema::new("path", SchemaType::File, false)
                .with_description("要摘要的文件；与 text 二选一"),
            ParamSchema::new("text", SchemaType::Str, false).with_description("直接摘要这段文本"),
            ParamSchema::new("offset", SchemaType::Int, false).with_default(0),
            ParamSchema::new("length", SchemaType::Int, false)
                .with_description("只取这么多字节；不填 = 读到结尾"),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let algo = Algo::parse(opt_str(map, "algorithm").as_deref())?;
        let (offset, length) = range_params(map)?;

        let (hex, size) = match (opt_str(map, "path"), opt_str(map, "text")) {
            (Some(_), Some(_)) => {
                return Err(ActionError::InvalidParams(
                    "path 与 text 只能指定其一".into(),
                ));
            }
            (None, None) => return Err(ActionError::MissingParam("path|text".into())),
            (Some(raw), None) => {
                let path = confine_path(ctx, Path::new(&raw))?;
                let mut slice = Slice::open(&path, offset, length).await?;
                // 先记下长度：`hash_slice` 会把这段读完，`left` 就归零了。
                let size = slice.len();
                let hex = hash_slice(&mut slice, algo, ctx).await?;
                (hex, size)
            }
            (None, Some(text)) => {
                // 文本没有「区间」可言：给了就报错，别悄悄忽略。
                if offset != 0 || length.is_some() {
                    return Err(ActionError::InvalidParams(
                        "text 输入不支持 offset / length".into(),
                    ));
                }
                let mut hasher = algo.hasher();
                hasher.update(text.as_bytes());
                (hasher.hex(), text.len() as u64)
            }
        };

        Ok(Value::Map(BTreeMap::from([
            ("algorithm".into(), Value::Str(algo.name().into())),
            ("hex".into(), Value::Str(hex)),
            ("size".into(), Value::Int(size as i64)),
        ])))
    }
}

#[async_trait]
impl Action for GenerateChunks {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::FILESYSTEM
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "generate.chunks",
            "生成分片计划",
            "一遍读完文件：给出整段摘要与每片的 index / offset / length / 摘要",
            Bucket::Data,
        )
        .with_params(vec![
            ParamSchema::new("path", SchemaType::File, true),
            ParamSchema::new("chunk", SchemaType::Int, true).with_description("每片字节数"),
            ParamSchema::new("algorithm", SchemaType::Str, false)
                .with_default("sha256")
                .with_description("sha256 | sha512 | md5"),
            ParamSchema::new("offset", SchemaType::Int, false).with_default(0),
            ParamSchema::new("length", SchemaType::Int, false)
                .with_description("只规划这么多字节；不填 = 到文件结尾"),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let algo = Algo::parse(opt_str(map, "algorithm").as_deref())?;
        let path = confine_path(ctx, &require_path(map, "path")?)?;
        let chunk = opt_i64(map, "chunk", 0);
        if chunk <= 0 {
            return Err(ActionError::InvalidParams(
                "chunk 必填且需大于 0（每片字节数）".into(),
            ));
        }
        let (offset, length) = range_params(map)?;
        chunk_plan(&path, offset, length, chunk as u64, algo, ctx).await
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(GenerateUuid));
    registry.register(Arc::new(GenerateCvid));
    registry.register(Arc::new(GeneratePath));
    registry.register(Arc::new(GenerateTimestamp));
    registry.register(Arc::new(GenerateHash));
    registry.register(Arc::new(GenerateChunks));
}

#[cfg(test)]
mod tests {
    use super::*;
    use corex_core::ExecutionContext;
    use tempfile::tempdir;

    #[tokio::test]
    async fn uuid_and_cvid() {
        let mut ctx = ExecutionContext::default();
        let out = GenerateUuid
            .execute(Value::Map(BTreeMap::new()), &mut ctx)
            .await
            .unwrap();
        let items = out
            .as_map()
            .unwrap()
            .get("items")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].as_str().unwrap().len(), 36);

        let cvid = GenerateCvid.execute(Value::Null, &mut ctx).await.unwrap();
        let s = cvid.as_str().unwrap();
        assert_eq!(s.len(), 32);
        assert!(
            s.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase())
        );
    }

    fn map(pairs: &[(&str, Value)]) -> Value {
        Value::Map(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), v.clone()))
                .collect(),
        )
    }

    fn field<'a>(out: &'a Value, key: &str) -> &'a Value {
        out.as_map().unwrap().get(key).unwrap()
    }

    #[tokio::test]
    async fn hash_matches_known_digests() {
        let mut ctx = ExecutionContext::default();
        // 标准测试向量："abc"
        let out = GenerateHash
            .execute(map(&[("text", Value::Str("abc".into()))]), &mut ctx)
            .await
            .unwrap();
        assert_eq!(field(&out, "algorithm"), &Value::Str("sha256".into()));
        assert_eq!(
            field(&out, "hex").as_str().unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(field(&out, "size"), &Value::Int(3));

        for (algo, expected) in [
            ("md5", "900150983cd24fb0d6963f7d28e17f72"),
            (
                "sha512",
                "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a\
                 2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f",
            ),
        ] {
            let out = GenerateHash
                .execute(
                    map(&[
                        ("text", Value::Str("abc".into())),
                        ("algorithm", Value::Str(algo.into())),
                    ]),
                    &mut ctx,
                )
                .await
                .unwrap();
            assert_eq!(field(&out, "hex").as_str().unwrap(), expected, "{algo}");
        }
    }

    #[tokio::test]
    async fn hash_reads_only_the_requested_range() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("blob.bin");
        let bytes: Vec<u8> = (0..3000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&path, &bytes).unwrap();

        let mut ctx = ExecutionContext::default();
        let out = GenerateHash
            .execute(
                map(&[
                    ("path", Value::Str(path.to_string_lossy().into())),
                    ("offset", Value::Int(1000)),
                    ("length", Value::Int(1000)),
                ]),
                &mut ctx,
            )
            .await
            .unwrap();
        assert_eq!(field(&out, "size"), &Value::Int(1000));
        assert_eq!(
            field(&out, "hex").as_str().unwrap(),
            hex(&sha2::Sha256::digest(&bytes[1000..2000]))
        );
    }

    #[tokio::test]
    async fn chunks_cover_the_file_once() {
        const SIZE: usize = 25;
        let dir = tempdir().unwrap();
        let path = dir.path().join("blob.bin");
        let bytes: Vec<u8> = (0..SIZE).map(|i| i as u8).collect();
        std::fs::write(&path, &bytes).unwrap();

        let mut ctx = ExecutionContext::default();
        let out = GenerateChunks
            .execute(
                map(&[
                    ("path", Value::Str(path.to_string_lossy().into())),
                    ("chunk", Value::Int(10)),
                ]),
                &mut ctx,
            )
            .await
            .unwrap();

        assert_eq!(field(&out, "total"), &Value::Int(3));
        assert_eq!(field(&out, "size"), &Value::Int(SIZE as i64));
        // 整段摘要顺手就算出来了，不用再读第二遍。
        assert_eq!(
            field(&out, "hash").as_str().unwrap(),
            hex(&sha2::Sha256::digest(&bytes))
        );

        let chunks = field(&out, "chunks").as_array().unwrap();
        assert_eq!(chunks.len(), 3);
        let expected = [(0i64, 0i64, 10usize), (1, 10, 10), (2, 20, 5)];
        for (item, (index, offset, len)) in chunks.iter().zip(expected) {
            assert_eq!(field(item, "index"), &Value::Int(index));
            assert_eq!(field(item, "offset"), &Value::Int(offset));
            assert_eq!(field(item, "length"), &Value::Int(len as i64));
            assert_eq!(
                field(item, "hex").as_str().unwrap(),
                hex(&sha2::Sha256::digest(&bytes[offset as usize..][..len]))
            );
        }
    }

    #[tokio::test]
    async fn chunks_reject_a_zero_sized_chunk() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("blob.bin");
        std::fs::write(&path, b"x").unwrap();
        let mut ctx = ExecutionContext::default();
        let err = GenerateChunks
            .execute(
                map(&[
                    ("path", Value::Str(path.to_string_lossy().into())),
                    ("chunk", Value::Int(0)),
                ]),
                &mut ctx,
            )
            .await
            .expect_err("chunk 0");
        assert!(err.to_string().contains("chunk"), "{err}");
    }
}
