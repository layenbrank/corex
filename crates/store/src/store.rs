//! 指令库本体：SQLite 读写 + YAML 导入导出 + 启动时的一次性迁移。

use super::error::StoreError;
use super::record::{
    DATABASE_FILE, LEGACY_DATABASE_FILE, DirectiveMeta, DirectiveRecord, DirectiveSummary, ImportEntry,
    ImportOptions, ImportReport, ImportStatus,
};
use super::schema;
use corex_engine::{Directive, DirectiveHistory, HistoryEntry};
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

/// 写库排队等的上限。
///
/// CLI 与 daemon 同时开着是常态（用户在终端跑 `corex run`，Studio 也在跑指令），两边都写
/// 指令库时后到的一方要等前者的事务结束。等 5 秒还拿不到锁说明有长事务，那时报错比无限等更好。
const BUSY_TIMEOUT_MS: u64 = 5_000;

/// 一次性迁移的记账键：写过就代表「旧数据目录已经看过一遍了」。
const LEGACY_IMPORT_KEY: &str = "legacy_import_from";

/// 旧 JSONL 账本的记账键。
const LEGACY_HISTORY_KEY: &str = "legacy_history_from";

/// 聚合「最近一次 + 次数」时最多回看多少条日志。
///
/// 与 JSONL 时代 `ExecutionHistory::SCAN` 同一个量级：够给最近跑过的指令名去重，也不至于
/// 为了画一张卡片把整份账本读出来。
pub(crate) const RUNS_SCAN: usize = 512;

/// 校验回调：把「这条指令能不能跑」的判断留给带注册表的一侧。
///
/// 库不认识注册表，但导入必须是「先校验再落库」——写进一条引用未注册动作的指令，
/// 用户要到运行时才发现。所以把判定从外面传进来。
pub type Validator<'a> = &'a dyn Fn(&Directive) -> Result<(), String>;

/// 指令库文件路径（`<数据目录>/corex.db`）。
///
/// 若仅存在早期的 `directives.db`，一次性改名为 `corex.db`（含 WAL / SHM）。
pub fn database_path(data_dir: &Path) -> PathBuf {
    migrate_legacy_database(data_dir);
    data_dir.join(DATABASE_FILE)
}

fn migrate_legacy_database(data_dir: &Path) {
    let dest = data_dir.join(DATABASE_FILE);
    let src = data_dir.join(LEGACY_DATABASE_FILE);
    if dest.exists() || !src.is_file() {
        return;
    }
    if let Err(error) = std::fs::rename(&src, &dest) {
        warn!(
            from = %src.display(),
            to = %dest.display(),
            error = %error,
            "指令库改名失败"
        );
        return;
    }
    for suffix in ["-wal", "-shm"] {
        let from = data_dir.join(format!("{LEGACY_DATABASE_FILE}{suffix}"));
        let to = data_dir.join(format!("{DATABASE_FILE}{suffix}"));
        if from.is_file() {
            let _ = std::fs::rename(&from, &to);
        }
    }
    info!(
        from = %LEGACY_DATABASE_FILE,
        to = %DATABASE_FILE,
        "指令库已改名为产品库文件名"
    );
}

/// 指令名必须是裸名：`..`、路径分隔符、盘符、绝对路径都不行。
///
/// 库里已经没有「路径」这个概念，但名字仍会被宿主拿去拼临时文件名、写进历史与 IPC 请求，
/// 规则与 v12 的 `check_directive_name` 保持一致——换一处实现，而不是换一套规则。
pub fn validate_name(name: &str) -> Result<(), StoreError> {
    let mut components = Path::new(name).components();
    let is_bare =
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none();
    // `\` 在 Unix 上不是分隔符，但同一个名字会跨平台进 YAML / IPC / 临时文件，不该只在
    // Windows 上被挡。
    if !is_bare || name.contains("..") || name.contains('\\') {
        return Err(StoreError::InvalidName(name.to_owned()));
    }
    Ok(())
}

/// 启动时要做的几件额外的事。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapOptions {
    /// 把旧数据目录里的 YAML 一次性导入（v12 → v13 的迁移）。
    pub is_auto_import: bool,
    /// 空库时写入起步指令（与 v12「空指令目录写起步指令」同一套内容）。
    pub is_seed: bool,
    /// 旧版 JSONL 执行账本的位置（`[history] file`）；`Some` 时一次性导入。
    ///
    /// 「上次执行时间」这类记录同样要跟着迁移 —— 指令搬进库、运行记录却留在 JSONL 的话，
    /// 卡片上的时间会突然全空，看起来像功能坏了。
    pub history_jsonl: Option<PathBuf>,
}

impl Default for BootstrapOptions {
    fn default() -> Self {
        Self {
            is_auto_import: true,
            is_seed: true,
            history_jsonl: None,
        }
    }
}

/// 启动时发生了什么：宿主/CLI 据此决定要不要提示用户。
#[derive(Debug, Clone, Default)]
pub struct BootstrapReport {
    /// 跑过一次旧目录导入才有；`None` = 之前已经记过账，或本次关闭了自动导入。
    pub legacy: Option<ImportReport>,
    /// 本次写入的起步指令名。
    pub seeded: Vec<String>,
    /// 本次从旧 JSONL 账本导入了多少条；`None` = 没做这一步。
    pub history_imported: Option<usize>,
}

impl BootstrapOptions {
    /// 按生效配置算出启动时要做的几件事。
    ///
    /// 三个入口（CLI / daemon / MCP）打开的都是同一个库，也就必须算出同一份选项：各自写一遍
    /// 「旧目录在哪、旧账本在哪、要不要播种」的话，同一台机器上换条命令启动就会迁移出两样结果。
    pub fn from_config(data_dir: &Path, config: &corex_core::RuntimeConfig) -> Self {
        Self {
            is_auto_import: config.directives.auto_import,
            is_seed: config.directives.seed,
            history_jsonl: legacy_ledger(data_dir, config),
        }
    }
}

/// 旧版 JSONL 账本的位置；只在它真的还在时才去导入。
///
/// `[history] file` 在 v13 里只剩这一个用途（一次性搬家）：账本搬进库以后，卡片上的
/// 「上次执行时间」才不会在升级当天集体变空。历史被关掉时也不看它——那是不记账。
fn legacy_ledger(data_dir: &Path, config: &corex_core::RuntimeConfig) -> Option<PathBuf> {
    if !config.history.enabled {
        return None;
    }
    let path = if config.history.file.is_absolute() {
        config.history.file.clone()
    } else {
        data_dir.join(&config.history.file)
    };
    path.is_file().then_some(path)
}

impl BootstrapReport {
    /// 值不值得跟用户说一句。
    pub fn is_quiet(&self) -> bool {
        let imported = self
            .legacy
            .as_ref()
            .map(|report| report.entries.len())
            .unwrap_or(0);
        imported == 0 && self.seeded.is_empty() && self.history_imported.unwrap_or(0) == 0
    }
}

/// 指令库。
///
/// 进程内共享一个连接，用 `Mutex` 串行化：写库都是短事务（单条 upsert / 改名），
/// 几十毫秒级的排队比每操作开一次连接便宜得多。跨进程靠 WAL + `busy_timeout`。
#[derive(Debug)]
pub struct DirectiveStore {
    conn: Mutex<Connection>,
}

impl DirectiveStore {
    /// 打开（必要时新建）库文件。
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Self::from_connection(Connection::open(path)?)
    }

    /// 内存库：测试与「只跑一条临时指令」的场景用。
    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self, StoreError> {
        // `journal_mode` 会回一行，所以用 query_row 而不是 pragma_update。
        let mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
        debug!(mode = %mode, "指令库日志模式");
        conn.execute_batch("PRAGMA synchronous = NORMAL;")?;
        conn.busy_timeout(Duration::from_millis(BUSY_TIMEOUT_MS))?;
        schema::migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// 取连接。
    ///
    /// 中毒（某个线程持锁时 panic）时把连接捞回来继续用：一条查询把整个 daemon 卡在
    /// 「打不开指令库」上，比继续服务糟得多。
    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// 用户可见指令元信息，按名字排序（与 `corex schedule` 的历史顺序一致）。
    ///
    /// 隐藏 `visible = 0` 的预配置指令；按名 `fetch` / `run` 仍可执行它们。
    pub fn metas(&self) -> Result<Vec<DirectiveMeta>, StoreError> {
        self.query_metas(&format!(
            "SELECT {SELECT_COLUMNS} FROM directives WHERE visible = 1 ORDER BY name ASC"
        ))
    }

    /// 全部指令元信息（含隐藏），调试 / 管理用。
    pub fn all_metas(&self) -> Result<Vec<DirectiveMeta>, StoreError> {
        self.query_metas(&format!(
            "SELECT {SELECT_COLUMNS} FROM directives ORDER BY name ASC"
        ))
    }

    fn query_metas(&self, sql: &str) -> Result<Vec<DirectiveMeta>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], RawRow::from_row)?;

        let mut metas = Vec::new();
        for row in rows {
            metas.push(row?.into_meta());
        }
        Ok(metas)
    }

    /// 库里有多少条指令。
    pub fn count(&self) -> Result<usize, StoreError> {
        let count: i64 = self
            .conn()
            .query_row("SELECT COUNT(*) FROM directives", [], |row| row.get(0))?;
        Ok(usize::try_from(count).unwrap_or(0))
    }

    /// 读一条指令，不存在时给 [`StoreError::NotFound`]。
    pub fn fetch(&self, name: &str) -> Result<DirectiveRecord, StoreError> {
        validate_name(name)?;
        self.find(name)?
            .ok_or_else(|| StoreError::NotFound(name.to_owned()))
    }

    /// 读一条指令，不存在时给 `None`。
    pub fn find(&self, name: &str) -> Result<Option<DirectiveRecord>, StoreError> {
        validate_name(name)?;
        let conn = self.conn();
        let row = conn
            .query_row(
                &format!("SELECT {SELECT_COLUMNS} FROM directives WHERE name = ?1"),
                [name],
                RawRow::from_row,
            )
            .optional()?;
        row.map(RawRow::into_record).transpose()
    }

    /// 新建或整体覆盖：`folder` / `source` / `visible` 由调用方给定（新建、导入走这条）。
    ///
    /// 模型里的 `name` 会被改写成这里的 `name`——库的键与模型只有一处真相，v12 里
    /// 「文件主干与 YAML 的 `name` 不一致，卡片就一直显示未运行」那类毛病不会再出现。
    pub fn put(
        &self,
        name: &str,
        definition: &Directive,
        folder: Option<&str>,
        source: Option<&str>,
        visible: bool,
    ) -> Result<DirectiveRecord, StoreError> {
        validate_name(name)?;
        let folder = normalize_folder(folder);
        let json = encode(name, definition)?;
        let now = now_ms();
        let visible_flag = if visible { 1 } else { 0 };
        self.conn().execute(
            "INSERT INTO directives (name, folder, source, definitionJson, visible, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6) \
             ON CONFLICT(name) DO UPDATE SET \
               folder = excluded.folder, \
               source = excluded.source, \
               definitionJson = excluded.definitionJson, \
               visible = excluded.visible, \
               updatedAt = excluded.updatedAt",
            params![name, folder, source, json, visible_flag, now],
        )?;
        self.fetch(name)
    }

    /// 保存模型（编辑器保存走这条）。
    ///
    /// 与 [`Self::put`] 的差别只在 `folder` / `source`：这里**不动**它们。编辑器保存一次
    /// 不该顺手把分组改掉，而新建的条目本来就没有分组。
    pub fn save(&self, name: &str, definition: &Directive) -> Result<DirectiveRecord, StoreError> {
        self.save_with_rename(None, name, definition)
    }

    /// 保存模型，并按需改名（编辑器里改完名字再保存走的就是这条）。
    ///
    /// 两件事必须在**一个事务**里：先确认新名字没被占，再删旧键、写新键并沿用旧行的分组与
    /// 来源。分开做的话，中途失败会留下「两个名字各有一半」的状态——v12 的「保存完多出一条
    /// 指令」正是这么来的。
    pub fn save_with_rename(
        &self,
        previous_name: Option<&str>,
        name: &str,
        definition: &Directive,
    ) -> Result<DirectiveRecord, StoreError> {
        validate_name(name)?;
        let json = encode(name, definition)?;
        let now = now_ms();
        let renaming_from = previous_name.filter(|previous| *previous != name);

        let mut conn = self.conn();
        let tx = conn.transaction()?;

        // 改名时先把旧行的分组 / 来源 / 可见性取下来：它们是库的元信息，不该因为改个名就丢掉。
        let carried = match renaming_from {
            Some(previous) => {
                validate_name(previous)?;
                let row: Option<(Option<String>, Option<String>, i64)> = tx
                    .query_row(
                        "SELECT folder, source, visible FROM directives WHERE name = ?1",
                        [previous],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()?;
                let taken: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM directives WHERE name = ?1)",
                    [name],
                    |row| row.get(0),
                )?;
                if taken {
                    return Err(StoreError::NameTaken(name.to_owned()));
                }
                if row.is_some() {
                    tx.execute("DELETE FROM directives WHERE name = ?1", [previous])?;
                }
                row
            }
            None => None,
        };

        // 不是改名（或旧行本来就不在库里）时分组与来源留空、可见；命中已有行时 `ON CONFLICT` 不碰元信息列。
        let (folder, source, visible_flag) = carried
            .map(|(folder, source, visible)| (folder, source, visible))
            .unwrap_or((None, None, 1));
        tx.execute(
            "INSERT INTO directives (name, folder, source, definitionJson, visible, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6) \
             ON CONFLICT(name) DO UPDATE SET \
               definitionJson = excluded.definitionJson, \
               updatedAt = excluded.updatedAt",
            params![name, folder, source, json, visible_flag, now],
        )?;
        tx.commit()?;
        drop(conn);
        self.fetch(name)
    }

    /// 改名：模型里的 `name` 跟着换，旧名字随之消失。
    ///
    /// v12 的「改名」是拿新名字再存一份文件，旧文件留在目录里成了僵尸指令；库里改名是一次
    /// `UPDATE`，不存在第二份。
    pub fn rename(&self, from: &str, to: &str) -> Result<DirectiveRecord, StoreError> {
        if from == to {
            return self.fetch(from);
        }
        let definition = self.fetch(from)?.definition;
        self.save_with_rename(Some(from), to, &definition)
    }

    /// 删掉一条指令。
    pub fn delete(&self, name: &str) -> Result<(), StoreError> {
        validate_name(name)?;
        let removed = self
            .conn()
            .execute("DELETE FROM directives WHERE name = ?1", [name])?;
        if removed == 0 {
            return Err(StoreError::NotFound(name.to_owned()));
        }
        Ok(())
    }

    /// 一条指令的规范化 YAML（导出用）。
    pub fn export_yaml(&self, name: &str) -> Result<String, StoreError> {
        Ok(self.fetch(name)?.yaml)
    }

    /// 把库里全部指令导出成一个 YAML 目录（`<out>/<名字>.yaml`），返回写下的文件路径。
    ///
    /// 默认不覆盖已有文件：旧版的指令目录往往还在原地，一次手滑的导出不该把用户原来的
    /// 文件盖掉。要覆盖得显式说（`is_overwrite`）。
    pub fn export_dir(&self, out: &Path, is_overwrite: bool) -> Result<Vec<PathBuf>, StoreError> {
        std::fs::create_dir_all(out)?;
        let mut written = Vec::new();
        for meta in self.metas()? {
            let path = out.join(format!("{}.yaml", meta.name));
            if path.exists() && !is_overwrite {
                warn!(path = %path.display(), "已存在，跳过导出");
                continue;
            }
            let yaml = self.export_yaml(&meta.name)?;
            std::fs::write(&path, yaml)?;
            written.push(path);
        }
        Ok(written)
    }

    /// 写一条执行日志。
    ///
    /// `recordedAt` 记的是「谁在什么时候把它写进来的」，与运行自己的起止时间分开：旧账本
    /// 导入进去的那些，落库时间与运行时间差着好几个月，排查时能分清。
    ///
    /// 同一条运行重复写会被唯一索引挡掉（`INSERT OR IGNORE`）——旧账本可能被导入多次，
    /// 重复计数比丢记录更难查。
    pub fn append_run(&self, entry: &HistoryEntry) -> Result<(), StoreError> {
        let recorded = now_ms();
        self.conn().execute(
            "INSERT OR IGNORE INTO runs \
             (directive, startedAt, endedAt, ok, error, duration, recordedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                entry.directive,
                entry.started_at_ms as i64,
                entry.ended_at_ms as i64,
                entry.ok,
                entry.error,
                entry.duration_ms as i64,
                recorded,
            ],
        )?;
        Ok(())
    }

    /// 最近的执行日志，新的在前；`name` 只看一条指令，`limit` 最多回几条。
    pub fn recent_runs(
        &self,
        name: Option<&str>,
        limit: usize,
    ) -> Result<Vec<HistoryEntry>, StoreError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT directive, startedAt, endedAt, ok, error, duration FROM runs \
             WHERE (?1 IS NULL OR directive = ?1) ORDER BY id DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![name, limit as i64], |row| {
            Ok(HistoryEntry {
                directive: row.get(0)?,
                started_at_ms: to_ms(row.get(1)?),
                ended_at_ms: to_ms(row.get(2)?),
                ok: row.get(3)?,
                error: row.get(4)?,
                duration_ms: to_ms(row.get(5)?),
            })
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// 最近跑过的指令名，新的在前、同名只留一次。
    pub fn recent_run_names(&self, limit: usize) -> Result<Vec<String>, StoreError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut names: Vec<String> = Vec::new();
        for entry in self.recent_runs(None, RUNS_SCAN.max(limit))? {
            if names.iter().any(|name| name == &entry.directive) {
                continue;
            }
            names.push(entry.directive);
            if names.len() == limit {
                break;
            }
        }
        Ok(names)
    }

    /// 按指令名聚合「最近一次 + 窗口内的次数」，语义与 JSONL 时代的 `by_directive` 一致。
    pub fn runs_by_directive(
        &self,
        window: usize,
    ) -> Result<BTreeMap<String, DirectiveHistory>, StoreError> {
        let mut out: BTreeMap<String, DirectiveHistory> = BTreeMap::new();
        for entry in self.recent_runs(None, window)? {
            let summary = out
                .entry(entry.directive.clone())
                .or_insert_with(|| DirectiveHistory::from_run(&entry));
            summary.run_count += 1;
            if !entry.ok {
                summary.failed_count += 1;
            }
        }
        Ok(out)
    }

    /// 执行日志一共有多少条。
    pub fn runs_count(&self) -> Result<usize, StoreError> {
        let count: i64 = self
            .conn()
            .query_row("SELECT COUNT(*) FROM runs", [], |row| row.get(0))?;
        Ok(usize::try_from(count).unwrap_or(0))
    }

    /// 把旧版的 JSONL 账本一次性导入（`[history] file`）。
    ///
    /// 只在 `meta.legacy_history_from` 没写过时动手；文件不存在也记账 —— 新装环境本来就没有
    /// 这份文件，不该每次启动都去 `stat` 一次。坏行跳过：一份账本里有一行写坏，不该让整份
    /// 导入放弃。
    pub fn import_legacy_history(&self, path: &Path) -> Result<Option<usize>, StoreError> {
        if self.meta(LEGACY_HISTORY_KEY)?.is_some() {
            return Ok(None);
        }

        let mut imported = 0usize;
        if path.is_file() {
            let text = std::fs::read_to_string(path)?;
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                match serde_json::from_str::<HistoryEntry>(line) {
                    Ok(entry) => {
                        if self.append_run(&entry).is_ok() {
                            imported += 1;
                        }
                    }
                    Err(error) => warn!(error = %error, "跳过损坏的历史行"),
                }
            }
        }
        self.put_meta(LEGACY_HISTORY_KEY, &path.display().to_string())?;
        Ok(Some(imported))
    }

    /// 库里的元信息。
    pub fn meta(&self, key: &str) -> Result<Option<String>, StoreError> {
        let value: Option<String> = self
            .conn()
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?;
        Ok(value)
    }

    /// 写一条元信息。
    pub fn put_meta(&self, key: &str, value: &str) -> Result<(), StoreError> {
        self.conn().execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// 导入单个 YAML 文件。
    pub fn import_file(
        &self,
        path: &Path,
        opts: &ImportOptions,
        validate: Validator<'_>,
    ) -> Result<ImportEntry, StoreError> {
        let folder = opts.folder.as_deref();
        self.import_file_at(path, folder, opts, validate)
    }

    /// 导入「用户给的那个路径」：文件就导入一条，目录就递归导入（子目录成为分组）。
    ///
    /// 三个入口（CLI / daemon / MCP）拿到的都是用户随手给的路径，所以判定「这是文件还是目录」
    /// 只该有一处——各写一份的话，`--dry-run`、分组回退这类语义迟早走偏。
    pub fn import_path(
        &self,
        path: &Path,
        opts: &ImportOptions,
        validate: Validator<'_>,
    ) -> Result<ImportReport, StoreError> {
        if path.is_dir() {
            return self.import_dir(path, opts, validate);
        }
        let entry = self.import_file(path, opts, validate)?;
        Ok(ImportReport {
            entries: vec![entry],
        })
    }

    /// 导入一段 YAML 文本（`corex edit` 的编辑往返也走它）。
    pub fn import_text(
        &self,
        text: &str,
        path: &Path,
        folder: Option<&str>,
        opts: &ImportOptions,
        validate: Validator<'_>,
    ) -> Result<ImportEntry, StoreError> {
        let fallback = file_stem(path);
        let definition = match Directive::from_yaml_str(text) {
            Ok(definition) => definition,
            Err(error) => {
                return Ok(ImportEntry {
                    name: fallback,
                    path: path.to_path_buf(),
                    status: ImportStatus::Failed(error.to_string()),
                });
            }
        };

        // 名字以模型里的 `name` 为准；留空就退回文件名主干，免得导入一份没写名字的 YAML 变成
        // 一条叫「」的指令。
        let name = match definition.name.trim() {
            "" => fallback,
            other => other.to_owned(),
        };
        if let Err(reason) = validate(&definition) {
            return Ok(ImportEntry {
                name,
                path: path.to_path_buf(),
                status: ImportStatus::Failed(reason),
            });
        }
        if let Err(error) = validate_name(&name) {
            return Ok(ImportEntry {
                name,
                path: path.to_path_buf(),
                status: ImportStatus::Failed(error.to_string()),
            });
        }

        let existing = self.find(&name)?.is_some();
        if existing && !opts.is_overwrite {
            return Ok(ImportEntry {
                name,
                path: path.to_path_buf(),
                status: ImportStatus::Skipped,
            });
        }
        let status = if existing {
            ImportStatus::Updated
        } else {
            ImportStatus::Created
        };
        if opts.is_dry_run {
            return Ok(ImportEntry {
                name,
                path: path.to_path_buf(),
                status,
            });
        }

        let source = path.display().to_string();
        self.put(&name, &definition, folder, Some(&source), true)?;
        Ok(ImportEntry {
            name,
            path: path.to_path_buf(),
            status,
        })
    }

    /// 导入一个目录（递归），子目录名成为分组。
    pub fn import_dir(
        &self,
        dir: &Path,
        opts: &ImportOptions,
        validate: Validator<'_>,
    ) -> Result<ImportReport, StoreError> {
        let mut files = Vec::new();
        collect_yaml_files(dir, &mut files)?;
        files.sort();

        let mut report = ImportReport::default();
        for path in files {
            // 子目录优先于 `--folder`：目录结构本身就是用户表达的分组。
            let folder = folder_of(dir, &path).or_else(|| opts.folder.clone());
            let entry = self.import_file_at(&path, folder.as_deref(), opts, validate)?;
            report.entries.push(entry);
        }
        Ok(report)
    }

    /// 空库时写入起步指令；库里已经有东西就一条都不写。
    ///
    /// 与 v12 的「空目录写入起步指令」同一套门槛（空库本身就是唯一门槛，删除不会长回来），
    /// 只是落点从目录换成了库。
    pub fn seed_starters(&self) -> Result<Vec<String>, StoreError> {
        if self.count()? > 0 {
            return Ok(Vec::new());
        }

        let mut written = Vec::new();
        for (name, yaml) in corex_engine::starter::entries() {
            match Directive::from_yaml_str(yaml) {
                Ok(definition) => {
                    self.put(name, &definition, None, None, true)?;
                    written.push((*name).to_owned());
                }
                // 起步指令是编译进二进制的资产，解析不了就是构建期的问题；
                // 这里告警而让启动继续，别为了它把 corex 卡死。
                Err(error) => warn!(name, error = %error, "起步指令解析失败"),
            }
        }
        if !written.is_empty() {
            debug!(count = written.len(), "已写入起步指令");
        }
        Ok(written)
    }

    /// 写入 / 刷新系统预配置指令（`visible = false`）。
    ///
    /// 与 [`Self::seed_starters`] 不同：每次启动都 upsert，保证产品依赖的隐藏指令始终在库里；
    /// 用户从列表看不到它们，但仍可按名 `run_directive`。
    pub fn seed_system(&self) -> Result<Vec<String>, StoreError> {
        let mut written = Vec::new();
        for (name, yaml) in corex_engine::system::directives() {
            match Directive::from_yaml_str(yaml) {
                Ok(definition) => {
                    self.put(name, &definition, None, None, false)?;
                    written.push((*name).to_owned());
                }
                Err(error) => warn!(name, error = %error, "系统预配置指令解析失败"),
            }
        }
        if !written.is_empty() {
            debug!(count = written.len(), "已写入系统预配置指令");
        }
        Ok(written)
    }

    /// 打开数据目录下的指令库，并做启动时该做的两件事：一次性迁移 + 空库播种。
    pub fn open_in_data_dir(
        data_dir: &Path,
        opts: BootstrapOptions,
        validate: Validator<'_>,
    ) -> Result<(Self, BootstrapReport), StoreError> {
        let store = Self::open(&database_path(data_dir))?;
        let mut report = BootstrapReport::default();

        if opts.is_auto_import {
            let legacy = data_dir.join("directives");
            report.legacy = store.import_legacy_dir(&legacy, validate)?;
            if let Some(imported) = &report.legacy
                && !imported.entries.is_empty()
            {
                info!(
                    dir = %legacy.display(),
                    created = imported.created(),
                    updated = imported.updated(),
                    skipped = imported.skipped(),
                    failed = imported.failed(),
                    "已把旧指令目录导入指令库"
                );
            }
        }

        if opts.is_seed {
            report.seeded = store.seed_starters()?;
            let _ = store.seed_system()?;
        }

        if let Some(history) = &opts.history_jsonl {
            report.history_imported = store.import_legacy_history(history)?;
            if let Some(count) = report.history_imported
                && count > 0
            {
                info!(
                    path = %history.display(),
                    count,
                    "已把旧执行账本导入指令库"
                );
            }
        }

        Ok((store, report))
    }

    /// 把旧版遗留的 YAML 目录一次性导入（v13 从文件切到库的迁移）。
    ///
    /// 只在 `meta.legacy_import_from` 没写过时动手，写完就记账——之后的每次启动不再扫目录。
    /// 目录不存在也记账：新装的环境里 `<数据目录>/directives` 根本不会出现，没必要时隔几天
    /// 就去 `stat` 一次。原文件一律不删，想回到文件形态用 `corex directive export`。
    pub fn import_legacy_dir(
        &self,
        dir: &Path,
        validate: Validator<'_>,
    ) -> Result<Option<ImportReport>, StoreError> {
        if self.meta(LEGACY_IMPORT_KEY)?.is_some() {
            return Ok(None);
        }

        let import = if dir.is_dir() {
            self.import_dir(dir, &ImportOptions::default(), validate)?
        } else {
            ImportReport::default()
        };
        self.put_meta(LEGACY_IMPORT_KEY, &dir.display().to_string())?;
        Ok(Some(import))
    }

    /// 单个文件的导入：读文本、带上分组、交给 [`Self::import_text`]。
    fn import_file_at(
        &self,
        path: &Path,
        folder: Option<&str>,
        opts: &ImportOptions,
        validate: Validator<'_>,
    ) -> Result<ImportEntry, StoreError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            // 目录里某个文件读不动（权限、被独占）不该让整次导入失败：逐条报出来。
            Err(error) => {
                return Ok(ImportEntry {
                    name: file_stem(path),
                    path: path.to_path_buf(),
                    status: ImportStatus::Failed(error.to_string()),
                });
            }
        };
        self.import_text(&text, path, folder, opts, validate)
    }
}

/// 一条记录要选的列，`list` / `find` 共用一份，免得两处列名各写一遍再慢慢走偏。
const SELECT_COLUMNS: &str =
    "name, folder, source, definitionJson, visible, createdAt, updatedAt";

/// 库里的一行原始数据：模型还是 JSON，读出来才解析。
struct RawRow {
    name: String,
    folder: Option<String>,
    source: Option<String>,
    json: String,
    visible: bool,
    created_at_ms: i64,
    updated_at_ms: i64,
}

impl RawRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        let visible_flag: i64 = row.get(4)?;
        Ok(Self {
            name: row.get(0)?,
            folder: row.get(1)?,
            source: row.get(2)?,
            json: row.get(3)?,
            visible: visible_flag != 0,
            created_at_ms: row.get(5)?,
            updated_at_ms: row.get(6)?,
        })
    }

    /// 列目录用：模型读不出来就只有元信息，条目照样在。
    fn into_meta(self) -> DirectiveMeta {
        let summary = serde_json::from_str::<Directive>(&self.json)
            .ok()
            .map(|definition| DirectiveSummary {
                description: definition.description.clone(),
                bucket: definition.bucket.map(|bucket| bucket.as_str().to_owned()),
                step_count: definition.steps.len(),
                input_count: definition.inputs.len(),
                trigger_count: definition.triggers.len(),
            });

        DirectiveMeta {
            name: self.name,
            folder: self.folder,
            source: self.source,
            visible: self.visible,
            created_at_ms: to_ms(self.created_at_ms),
            updated_at_ms: to_ms(self.updated_at_ms),
            summary,
        }
    }

    fn into_record(self) -> Result<DirectiveRecord, StoreError> {
        let definition = decode(&self.json)?;
        let yaml = definition.to_yaml_str().map_err(StoreError::from)?;
        Ok(DirectiveRecord {
            name: self.name,
            folder: self.folder,
            source: self.source,
            visible: self.visible,
            created_at_ms: to_ms(self.created_at_ms),
            updated_at_ms: to_ms(self.updated_at_ms),
            definition,
            yaml,
        })
    }
}

/// 模型 → JSON，并把 `name` 统一成库里的键。
fn encode(name: &str, definition: &Directive) -> Result<String, StoreError> {
    let mut definition = definition.clone();
    definition.name = name.to_owned();
    serde_json::to_string(&definition)
        .map_err(|error| StoreError::Invalid(format!("指令定义写不成 JSON: {error}")))
}

fn decode(json: &str) -> Result<Directive, StoreError> {
    serde_json::from_str(json)
        .map_err(|error| StoreError::Invalid(format!("库里的指令定义读不出来: {error}")))
}

/// 分组名归一：两边空白去掉、分隔符统一成 `/`、外侧斜杠去掉，空的算没有分组。
fn normalize_folder(folder: Option<&str>) -> Option<String> {
    let folder = folder?.trim().replace('\\', "/");
    let folder = folder.trim_matches('/').to_owned();
    if folder.is_empty() || folder.split('/').any(|part| part == ".." || part == ".") {
        return None;
    }
    Some(folder)
}

/// 文件在导入根之下的相对目录（不含文件名），空/根目录给 `None`。
fn folder_of(root: &Path, path: &Path) -> Option<String> {
    let parent = path.parent()?;
    let relative = parent.strip_prefix(root).ok()?;
    let parts: Vec<String> = relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

/// 文件主干；拿不到就给个占位，别让报告里出现空名字。
fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("corex-directive")
        .to_owned()
}

/// 递归收集 `*.yaml` / `*.yml`；目录不存在就是空列表（新装环境本来就没有）。
fn collect_yaml_files(dir: &Path, found: &mut Vec<PathBuf>) -> Result<(), StoreError> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_yaml_files(&path, found)?;
            continue;
        }
        if matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("yaml") | Some("yml")
        ) {
            found.push(path);
        }
    }
    Ok(())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

/// SQLite 的整数列是 i64，时间是毫秒（远小于 2^53）；负数只可能来自手改库，当 0 处理。
fn to_ms(value: i64) -> u64 {
    value.max(0) as u64
}
