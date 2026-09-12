//! 生成类动作：路径列表、uuid、cvid。

use crate::ActionRegistry;
use crate::builtin::filter::Filter;
use crate::builtin::util::{
    MAX_RANGE, Slice, confine_path, ensure_parent, file_size, hex, opt_bool, opt_i64, opt_str,
    opt_strs, range_len, range_params, require_map, require_path, require_str,
};
use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::STANDARD};
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
    hex(&array).to_uppercase()
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
        // 一个文件一行，未缓冲的话每行都是一次系统调用；几千个文件就看得出来。
        let mut file = std::io::BufWriter::new(std::fs::File::create(&to)?);
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
        file.flush()?;

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
    fn parse(raw: &str) -> Result<Self, ActionError> {
        match raw.trim().to_ascii_lowercase().replace('-', "").as_str() {
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

/// 摘要写成什么形式。
///
/// 同一个摘要两种说法都有人要：对象存储报十六进制，而不少分片接口要 base64。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Encoding {
    Hex,
    Base64,
}

impl Encoding {
    fn parse(raw: Option<&str>) -> Result<Self, ActionError> {
        match raw.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
            None | Some("") | Some("hex") => Ok(Self::Hex),
            Some("base64") | Some("b64") => Ok(Self::Base64),
            Some(other) => Err(ActionError::InvalidParams(format!(
                "不支持的 encoding: {other}（hex | base64）"
            ))),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Hex => "hex",
            Self::Base64 => "base64",
        }
    }

    fn encode(self, digest: &[u8]) -> String {
        match self {
            Self::Hex => hex(digest),
            Self::Base64 => STANDARD.encode(digest),
        }
    }
}

/// 解析 `algorithm`：一个名字，或一串名字。
///
/// 分片上传常要两种：整文件按 `sha256` 报，每片按服务端要求的 `md5` 算。
/// 传数组就是「同一遍读取全算出来」，不必为此再读一遍盘。
fn algo_params(map: &BTreeMap<String, Value>) -> Result<Vec<Algo>, ActionError> {
    let raw: Vec<String> = match map.get("algorithm") {
        None | Some(Value::Null) => vec!["sha256".to_string()],
        Some(Value::Str(s)) => vec![s.clone()],
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| {
                v.as_str().map(str::to_string).ok_or_else(|| {
                    ActionError::InvalidParams("algorithm 数组的元素须是字符串".into())
                })
            })
            .collect::<Result<_, _>>()?,
        Some(other) => {
            return Err(ActionError::InvalidParams(format!(
                "algorithm 须是字符串或字符串数组，收到 {}",
                other.to_json()
            )));
        }
    };
    let mut algos: Vec<Algo> = Vec::new();
    for name in &raw {
        let algo = Algo::parse(name)?;
        if !algos.contains(&algo) {
            algos.push(algo);
        }
    }
    if algos.is_empty() {
        return Err(ActionError::InvalidParams("algorithm 至少要有一个".into()));
    }
    Ok(algos)
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

    /// 摘要的原始字节；写成 hex 还是 base64 由 [`Encoding`] 决定。
    fn digest(self) -> Vec<u8> {
        match self {
            Self::Sha256(h) => h.finalize().to_vec(),
            Self::Sha512(h) => h.finalize().to_vec(),
            Self::Md5(h) => h.finalize().to_vec(),
        }
    }
}

/// 一组摘要器：一次读取同时喂多个算法。
struct Digests {
    algos: Vec<Algo>,
    hashers: Vec<Hasher>,
}

impl Digests {
    fn new(algos: &[Algo]) -> Self {
        Self {
            algos: algos.to_vec(),
            hashers: algos.iter().map(|a| a.hasher()).collect(),
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        for hasher in &mut self.hashers {
            hasher.update(bytes);
        }
    }

    fn finish(self) -> Digested {
        let Self { algos, hashers } = self;
        Digested {
            algos,
            digests: hashers.into_iter().map(Hasher::digest).collect(),
        }
    }
}

/// 算完的一组摘要。
///
/// 原始字节先留着，因为同一个摘要要出两种形式：`hex` / `hash` 这两个兼容字段
/// 永远是十六进制，而 `hashes` 按调用方要的 `encoding` 写。
struct Digested {
    algos: Vec<Algo>,
    digests: Vec<Vec<u8>>,
}

impl Digested {
    /// 第一个算法的摘要，十六进制。
    fn first_hex(&self) -> String {
        hex(&self.digests[0])
    }

    /// `hashes` 只在比 `hex` 多说一点时才出现：单算法 + hex 就是纯重复。
    ///
    /// 整段与每片共用这一条判据，形状不会两处各说一套。
    fn has_detail(&self, encoding: Encoding) -> bool {
        self.algos.len() > 1 || encoding != Encoding::Hex
    }

    /// `{ "<algo>": "<按 encoding 编码的摘要>" }`。
    fn map(&self, encoding: Encoding) -> BTreeMap<String, Value> {
        self.algos
            .iter()
            .zip(&self.digests)
            .map(|(algo, digest)| (algo.name().to_string(), Value::Str(encoding.encode(digest))))
            .collect()
    }
}

/// 把整段流式吃完，边读边报进度。
async fn digest_slice(
    slice: &mut Slice,
    algos: &[Algo],
    ctx: &ExecutionContext,
) -> Result<Digested, ActionError> {
    let total = slice.len();
    let mut digests = Digests::new(algos);
    let mut buf = vec![0u8; READ_CHUNK];
    let mut done = 0u64;
    loop {
        let n = slice.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        digests.update(&buf[..n]);
        done += n as u64;
        ctx.chunk(done, Some(total), Unit::Bytes);
    }
    Ok(digests.finish())
}

/// `generate.chunks` 的一次调用。
struct ChunkPlan {
    offset: u64,
    length: Option<u64>,
    /// 每片字节数。
    chunk: u64,
    /// 第一片的序号：断点续传时接着上次的编号往下走，服务端才不会错位。
    index_start: u64,
}

/// 只切块：把 `[offset, offset + length)` 按 `chunk` 划成一段段，给出每段的
/// `index` / `offset` / `length`。
///
/// **不碰内容，也不算摘要**。每片的摘要由调用方按需向 [`GenerateHash`] 要
/// （带上这里给出的 `offset` / `length`）——切块和算摘要是两件事，各是一遍读盘，
/// 只要其中一件的人不必付另一件的代价。这里只 `stat` 一次拿文件大小。
fn chunk_plan(plan: ChunkPlan, size: u64) -> Result<Value, ActionError> {
    let ChunkPlan {
        offset,
        length,
        chunk,
        index_start,
    } = plan;
    // 上限来自「一段」的公共天花板：再大的片下游也发不出去，早点说比建完计划再失败好。
    if chunk > MAX_RANGE {
        return Err(ActionError::InvalidParams(format!(
            "chunk {chunk} 超过 {MAX_RANGE} 字节上限"
        )));
    }
    let size = range_len(size, offset, length)?;
    let total = size.div_ceil(chunk);
    let mut chunks = Vec::with_capacity(total as usize);
    for i in 0..total {
        let start = i * chunk;
        chunks.push(Value::Map(BTreeMap::from([
            ("index".into(), Value::Int((index_start + i) as i64)),
            ("offset".into(), Value::Int((offset + start) as i64)),
            (
                "length".into(),
                Value::Int(size.saturating_sub(start).min(chunk) as i64),
            ),
        ])));
    }
    Ok(Value::Map(BTreeMap::from([
        ("chunk".into(), Value::Int(chunk as i64)),
        ("size".into(), Value::Int(size as i64)),
        ("total".into(), Value::Int(total as i64)),
        ("chunks".into(), Value::Array(chunks)),
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
                .with_description("sha256 | sha512 | md5；也可以是数组，一遍读完一起算"),
            ParamSchema::new("encoding", SchemaType::Str, false)
                .with_default("hex")
                .with_description(
                    "`hashes` 里的写法：hex | base64（单算法 + hex 时不输出 `hashes`）",
                ),
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
        let algos = algo_params(map)?;
        let encoding = Encoding::parse(opt_str(map, "encoding").as_deref())?;
        let (offset, length) = range_params(map)?;

        let (digested, size) = match (opt_str(map, "path"), opt_str(map, "text")) {
            (Some(_), Some(_)) => {
                return Err(ActionError::InvalidParams(
                    "path 与 text 只能指定其一".into(),
                ));
            }
            (None, None) => return Err(ActionError::MissingParam("path|text".into())),
            (Some(raw), None) => {
                let path = confine_path(ctx, Path::new(&raw))?;
                let mut slice = Slice::open(&path, offset, length).await?;
                // 先记下长度：`digest_slice` 会把这段读完，`left` 就归零了。
                let size = slice.len();
                let digested = digest_slice(&mut slice, &algos, ctx).await?;
                (digested, size)
            }
            (None, Some(text)) => {
                // 文本没有「区间」可言：给了就报错，别悄悄忽略。
                if offset != 0 || length.is_some() {
                    return Err(ActionError::InvalidParams(
                        "text 输入不支持 offset / length".into(),
                    ));
                }
                let mut digests = Digests::new(&algos);
                digests.update(text.as_bytes());
                (digests.finish(), text.len() as u64)
            }
        };

        let algorithms = algos.iter().map(|a| Value::Str(a.name().into())).collect();
        let mut out = BTreeMap::from([
            ("algorithm".into(), Value::Str(algos[0].name().into())),
            ("algorithms".into(), Value::Array(algorithms)),
            ("encoding".into(), Value::Str(encoding.name().into())),
            ("hex".into(), Value::Str(digested.first_hex())),
            ("size".into(), Value::Int(size as i64)),
        ]);
        if digested.has_detail(encoding) {
            out.insert("hashes".into(), Value::Map(digested.map(encoding)));
        }
        Ok(Value::Map(out))
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
            "只切块：给出每片的 index / offset / length（不算摘要，摘要请用 generate.hash）",
            Bucket::Data,
        )
        .with_params(vec![
            ParamSchema::new("path", SchemaType::File, true),
            ParamSchema::new("chunk", SchemaType::Int, true).with_description("每片字节数"),
            ParamSchema::new("index", SchemaType::Int, false)
                .with_default(0)
                .with_description("第一片的序号（断点续传时接着上次的编号）"),
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
        let path = confine_path(ctx, &require_path(map, "path")?)?;
        let chunk = opt_i64(map, "chunk", 0);
        if chunk <= 0 {
            return Err(ActionError::InvalidParams(
                "chunk 必填且需大于 0（每片字节数）".into(),
            ));
        }
        let index_start = opt_i64(map, "index", 0);
        if index_start < 0 {
            return Err(ActionError::InvalidParams("index 不能为负".into()));
        }
        let (offset, length) = range_params(map)?;
        chunk_plan(
            ChunkPlan {
                offset,
                length,
                chunk: chunk as u64,
                index_start: index_start as u64,
            },
            file_size(&path).await?,
        )
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
    async fn chunks_cover_the_file() {
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

        let chunks = field(&out, "chunks").as_array().unwrap();
        assert_eq!(chunks.len(), 3);
        let expected = [(0i64, 0i64, 10usize), (1, 10, 10), (2, 20, 5)];
        for (item, (index, offset, len)) in chunks.iter().zip(expected) {
            assert_eq!(field(item, "index"), &Value::Int(index));
            assert_eq!(field(item, "offset"), &Value::Int(offset));
            assert_eq!(field(item, "length"), &Value::Int(len as i64));
        }
    }

    /// 单一职责：分片计划里**一点摘要都没有**。
    ///
    /// 摘要归 `generate.hash`（可按 `offset`/`length` 只算一片），两块拼在一起会让人
    /// 以为「文件摘要 = 各片摘要的某种聚合」，也逼着只想切块的人多读一遍盘。
    #[tokio::test]
    async fn chunks_never_carry_a_digest() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("blob.bin");
        std::fs::write(&path, [7u8; 32]).unwrap();
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

        let names: Vec<&str> = out.as_map().unwrap().keys().map(String::as_str).collect();
        assert_eq!(names, vec!["chunk", "chunks", "size", "total"], "{names:?}");
        let item = &field(&out, "chunks").as_array().unwrap()[0];
        let names: Vec<&str> = item.as_map().unwrap().keys().map(String::as_str).collect();
        assert_eq!(names, vec!["index", "length", "offset"], "{names:?}");
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

    /// 一次读取算多个摘要：整文件报 `sha256`、每片按服务端要求算 `md5` 是常见组合。
    #[tokio::test]
    async fn hash_accepts_a_list_of_algorithms() {
        let mut ctx = ExecutionContext::default();
        let out = GenerateHash
            .execute(
                map(&[
                    ("text", Value::Str("abc".into())),
                    (
                        "algorithm",
                        Value::Array(vec![Value::Str("sha256".into()), Value::Str("md5".into())]),
                    ),
                ]),
                &mut ctx,
            )
            .await
            .unwrap();
        assert_eq!(field(&out, "algorithm"), &Value::Str("sha256".into()));
        assert_eq!(
            field(&out, "hashes").as_map().unwrap().get("md5"),
            Some(&Value::Str("900150983cd24fb0d6963f7d28e17f72".into()))
        );
        // `hex` 仍是第一个算法的十六进制，老调用方不受影响。
        assert_eq!(
            field(&out, "hex").as_str().unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    /// `encoding: base64` 换个写法，`hex` 不动。
    #[tokio::test]
    async fn hash_supports_base64_encoding() {
        let mut ctx = ExecutionContext::default();
        let out = GenerateHash
            .execute(
                map(&[
                    ("text", Value::Str("abc".into())),
                    ("encoding", Value::Str("base64".into())),
                ]),
                &mut ctx,
            )
            .await
            .unwrap();
        assert_eq!(field(&out, "encoding"), &Value::Str("base64".into()));
        assert_eq!(
            field(&out, "hashes").as_map().unwrap().get("sha256"),
            Some(&Value::Str(
                "ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0=".into()
            ))
        );
        assert!(field(&out, "hex").as_str().unwrap().starts_with("ba7816bf"));
    }

    /// 断点续传：接着上次的编号往下走，`offset` 也从中途开始。
    #[tokio::test]
    async fn chunks_can_start_at_an_offset_index() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("blob.bin");
        std::fs::write(&path, [0u8; 25]).unwrap();

        let mut ctx = ExecutionContext::default();
        let out = GenerateChunks
            .execute(
                map(&[
                    ("path", Value::Str(path.to_string_lossy().into())),
                    ("chunk", Value::Int(10)),
                    ("offset", Value::Int(20)),
                    ("index", Value::Int(5)),
                ]),
                &mut ctx,
            )
            .await
            .unwrap();

        assert_eq!(field(&out, "total"), &Value::Int(1));
        let chunks = field(&out, "chunks").as_array().unwrap();
        assert_eq!(field(&chunks[0], "index"), &Value::Int(5));
        assert_eq!(field(&chunks[0], "offset"), &Value::Int(20));
        assert_eq!(field(&chunks[0], "length"), &Value::Int(5));
    }
}
