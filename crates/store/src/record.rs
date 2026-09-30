//! 一条指令在库里的形状，以及导入的设置与结果。

use corex_engine::Directive;
use std::path::PathBuf;

/// 指令库文件名，放在数据目录下（`<数据目录>/directives.db`）。
pub const DIRECTIVES_DB_FILE: &str = "directives.db";

/// 列目录用的元信息：不含模型，卡片只需要这些。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectiveMeta {
    pub name: String,
    /// 分组（自由文本；导入时取相对子目录）。`None` = 未分组。
    pub folder: Option<String>,
    /// 导入来源（YAML 路径）；库里新建的没有。
    pub source: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    /// 模型读得出来才有。读不出来的条目**照样列出来**——与 v12 列 YAML 目录时的行为一致：
    /// 用户正是要靠这份列表把坏条目打开来修，而不是让它从界面上消失。
    pub summary: Option<DirectiveSummary>,
}

/// 卡片要用的几个数：宿主列目录时不该为了显示它们把每条模型再解析一遍。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectiveSummary {
    pub description: String,
    /// 动作分类（`system` / `network` / `data` / `ui` / `logic` / `plugin`）。
    pub bucket: Option<String>,
    pub step_count: usize,
    pub input_count: usize,
    pub trigger_count: usize,
}

/// 一条指令的完整记录：模型 + 由模型序列化出的规范化 YAML。
#[derive(Debug, Clone)]
pub struct DirectiveRecord {
    pub name: String,
    pub folder: Option<String>,
    pub source: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub definition: Directive,
    /// 规范化 YAML：只给展示与导出用。宿主不要把它当输入再拼一遍——写盘格式只有引擎一份。
    pub yaml: String,
}

/// 导入开关。
#[derive(Debug, Clone, Default)]
pub struct ImportOptions {
    /// 同名时覆盖；默认拒绝并记 [`ImportStatus::Skipped`]。
    pub is_overwrite: bool,
    /// 只解析校验、不写库（`--dry-run`）。
    pub is_dry_run: bool,
    /// 分组：单个文件导入时用它；目录导入时相对子目录优先，没进子目录的才用它。
    pub folder: Option<String>,
}

/// 一个条目的导入结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportStatus {
    /// 新条目入库。
    Created,
    /// 覆盖了同名条目。
    Updated,
    /// 同名且没开覆盖。
    Skipped,
    /// 解析或校验没过，附带原因。
    Failed(String),
}

/// 一个文件（或一段 YAML）的导入结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportEntry {
    /// 指令名；解析失败时退化成文件名主干，好让报告里仍有东西可指。
    pub name: String,
    pub path: PathBuf,
    pub status: ImportStatus,
}

/// 一次导入的全部结果。
///
/// 逐条报告而不是「成功几条、失败几条」：导入是用户拿自己的文件来换库里的内容，
/// 「哪个文件为什么没进来」必须能一眼看到。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub entries: Vec<ImportEntry>,
}

impl ImportReport {
    pub fn created(&self) -> usize {
        self.count(|status| matches!(status, ImportStatus::Created))
    }

    pub fn updated(&self) -> usize {
        self.count(|status| matches!(status, ImportStatus::Updated))
    }

    pub fn skipped(&self) -> usize {
        self.count(|status| matches!(status, ImportStatus::Skipped))
    }

    pub fn failed(&self) -> usize {
        self.count(|status| matches!(status, ImportStatus::Failed(_)))
    }

    /// 全部条目都进了库（没有跳过、也没有失败）。
    pub fn is_clean(&self) -> bool {
        self.skipped() == 0 && self.failed() == 0
    }

    fn count(&self, wanted: impl Fn(&ImportStatus) -> bool) -> usize {
        self.entries
            .iter()
            .filter(|entry| wanted(&entry.status))
            .count()
    }
}
