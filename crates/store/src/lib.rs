//! 指令库：SQLite 是指令与执行日志的**唯一真相源**。
//!
//! v12 及以前，指令的真相是磁盘上的 YAML 文件，于是每个入口各有一套「指令根」：
//! CLI 还看当前目录与仓库的 `examples/directives`，daemon 只看 `<数据目录>/directives`
//! 且不递归，MCP 用 `--directives` 另指一处，Studio 找不到用户装的 corex 时还会退回
//! 自带的私有目录。四套规则下，「同一条指令在哪」本身就说不清。执行日志也一样：它在
//! 另一个文件（`history.jsonl`）里，宿主想显示「上次执行时间」就得自己再存一份。
//!
//! v13 把读写收成一处：[`DirectiveStore`] 打开 `<数据目录>/corex.db`，指令、执行日志、
//! 上次执行时间都在里面，谁读谁写都只经过它。YAML 退到三个位置——
//! [`DirectiveStore::import_file`] / [`DirectiveStore::export_yaml`] / `corex edit` 的编辑
//! 往返——它不再是真相，只是一份可读可写的表示。
//!
//! 这一层刻意**不认识动作注册表**：写进库的指令是否引用了已注册的动作、声明的权限够不够，
//! 由调用方（CLI / daemon）用 `corex_engine::validate_registered` / `validate_allowed` 判，
//! 判定结果经 [`Validator`] 传进下面这些导入函数。库只负责「存得住、取得回、改得动」。

mod error;
mod history;
mod record;
mod schema;
mod store;

pub use error::StoreError;
pub use history::{SqliteHistory, StoreDirectiveSource, history_sink};
pub use record::{
    DATABASE_FILE, DirectiveMeta, DirectiveRecord, DirectiveSummary, ImportEntry,
    ImportOptions, ImportReport, ImportStatus,
};
pub use store::{
    BootstrapOptions, BootstrapReport, DirectiveStore, Validator, database_path, validate_name,
};
