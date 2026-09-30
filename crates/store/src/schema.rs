//! 建表与版本迁移。库的结构只在这里定义。
//!
//! 版本号落在 `meta.schema_version`，而不是每次启动把 `CREATE TABLE IF NOT EXISTS` 跑一遍
//! 当作迁移：后者表达不了「列改名 / 列类型变化」这类变更，等真需要时才发现迁移根本写不出来。
//! 现在只有 v1，但按号补迁移的位置已经留好。

use super::error::StoreError;
use rusqlite::{Connection, OptionalExtension};

/// 当前结构版本。改表就 +1，并在 [`migrate`] 里补一步。
pub(crate) const SCHEMA_VERSION: i64 = 1;

/// 结构版本记录的键。
const VERSION_KEY: &str = "schema_version";

/// v1 的结构。
///
/// 模型整体存 JSON，字段不拆成列：模型的形状只有引擎一份（`corex_engine::Directive`），
/// 拆列就等于在这里再维护一份 schema —— v12 正是为了「不出现第二份真相」才把 YAML 解析
/// 收进 corex 一侧。**真的需要按字段查**（例如按分类筛选几十万条）时再拆，那时也有迁移路径。
const SQL_V1: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS directives (
  name            TEXT PRIMARY KEY,
  folder          TEXT,
  source          TEXT,
  definition_json TEXT NOT NULL,
  created_at_ms   INTEGER NOT NULL,
  updated_at_ms   INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_directives_folder ON directives(folder);
CREATE INDEX IF NOT EXISTS idx_directives_updated ON directives(updated_at_ms);

-- 执行日志：与指令同库，「上次执行时间 / 上次成功没 / 跑了多少次」不再另找一处账本。
-- `(directive, started_at_ms, ended_at_ms)` 唯一：旧 JSONL 账本可能被导入多次，重复计数
-- 比丢一次记录更难查，所以按「同一条运行」去重，导入也就天然幂等。
CREATE TABLE IF NOT EXISTS runs (
  id             INTEGER PRIMARY KEY AUTOINCREMENT,
  directive      TEXT NOT NULL,
  started_at_ms  INTEGER NOT NULL,
  ended_at_ms    INTEGER NOT NULL,
  ok             INTEGER NOT NULL,
  error          TEXT,
  duration_ms    INTEGER NOT NULL,
  recorded_at_ms INTEGER NOT NULL,
  UNIQUE (directive, started_at_ms, ended_at_ms)
);

CREATE INDEX IF NOT EXISTS idx_runs_directive ON runs(directive, id DESC);
CREATE INDEX IF NOT EXISTS idx_runs_recorded ON runs(recorded_at_ms DESC);
"#;

/// 把库补到 [`SCHEMA_VERSION`]。
pub(crate) fn migrate(conn: &Connection) -> Result<(), StoreError> {
    let version = read_version(conn)?;

    if let Some(found) = version
        && found > SCHEMA_VERSION
    {
        // 库比程序新：多半是用户把 corex 降级了。硬读下去只会写出半懂的数据，不如说清楚。
        return Err(StoreError::Invalid(format!(
            "指令库由更新的 corex 建过（结构版本 {found} > {SCHEMA_VERSION}），请升级 corex"
        )));
    }

    if version.is_none() {
        conn.execute_batch(SQL_V1)?;
    }

    // 下一次改表的写法（保持按号递进，不要在这里写 `if version < 2` 之外的判断）：
    //
    //   if version.is_some_and(|v| v < 2) {
    //       conn.execute_batch(SQL_V2)?;
    //   }

    write_version(conn, SCHEMA_VERSION)?;
    Ok(())
}

fn read_version(conn: &Connection) -> Result<Option<i64>, StoreError> {
    let has_meta: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'meta')",
        [],
        |row| row.get(0),
    )?;
    if !has_meta {
        return Ok(None);
    }

    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = ?1",
            [VERSION_KEY],
            |row| row.get(0),
        )
        .optional()?;
    Ok(raw.and_then(|value| value.trim().parse::<i64>().ok()))
}

fn write_version(conn: &Connection, version: i64) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![VERSION_KEY, version.to_string()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_schema_and_records_version() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        assert_eq!(read_version(&conn).unwrap(), Some(SCHEMA_VERSION));

        // 再跑一次是幂等的（每次启动都会经过这里）。
        migrate(&conn).unwrap();
        assert_eq!(read_version(&conn).unwrap(), Some(SCHEMA_VERSION));
    }

    /// 库比程序新时必须拒绝打开：降级后继续写，写出来的东西新版读不懂。
    #[test]
    fn refuses_newer_schema() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        write_version(&conn, SCHEMA_VERSION + 1).unwrap();

        let error = migrate(&conn).unwrap_err();
        assert_eq!(error.kind(), "parse");
    }
}
