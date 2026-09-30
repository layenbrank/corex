//! `corex directive …`：指令库的增删改查，以及 YAML 的进出。
//!
//! 库是真相，所以这一族里没有「目录」参数：`list` / `show` / `edit` / `rm` / `rename` 都按名字
//! 说话。YAML 只出现在两个方向——`import` 把它收进来，`export` 把它放出去；`edit` 的临时文件
//! 往返则是第三条（读出来改、校验完写回去）。

use crate::editor;
use crate::library::Library;
use crate::output::outln;
use crate::{ask, create, usage};
use anyhow::{Context, Result, bail};
use clap::Subcommand;
use corex_engine::Directive;
use corex_store::{ImportOptions, ImportStatus};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// `corex directive` 的子命令。
#[derive(Subcommand, Debug)]
pub(crate) enum DirectiveCmd {
    /// 列出指令库里的指令
    List {
        /// 输出机器可读的 JSON
        #[arg(long)]
        json: bool,
    },
    /// 打印一条指令的规范化 YAML
    Show {
        /// 指令名
        name: String,
        /// 输出模型 JSON，而不是 YAML
        #[arg(long)]
        json: bool,
    },
    /// 新建一条指令（交互向导，或 -t 选模板）
    New {
        /// 指令名；省略则在交互里问
        name: Option<String>,
        /// 模板：内置名（blank / hello / http / file / cron / watch / ui）、目录或 YAML 路径
        #[arg(short, long)]
        template: Option<String>,
        /// 库里已有同名指令时覆盖
        #[arg(short, long)]
        force: bool,
        /// 只写文件到这个目录，不入库（脚手架 / 要一份能提交的 YAML）
        #[arg(long, value_name = "DIR")]
        file: Option<PathBuf>,
    },
    /// 用 $COREX_EDITOR / $VISUAL / $EDITOR 打开一条指令，退出时校验并写回
    Edit {
        /// 指令名
        name: String,
    },
    /// 删掉一条指令
    Rm {
        /// 指令名
        name: String,
        /// 不再确认
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// 改名
    Rename {
        /// 现在的名字
        from: String,
        /// 换成的名字
        to: String,
    },
    /// 从 YAML 文件或目录导入（子目录成为分组）
    Import {
        /// 文件或目录
        path: PathBuf,
        /// 目录导入时的兜底分组（子目录优先）
        #[arg(long)]
        folder: Option<String>,
        /// 同名时覆盖
        #[arg(long)]
        overwrite: bool,
        /// 只解析校验，不写库
        #[arg(long)]
        dry_run: bool,
    },
    /// 把指令导出成 YAML 文件
    Export {
        /// 要导出的指令名；省略则导出全部
        names: Vec<String>,
        /// 输出目录
        #[arg(short, long, value_name = "DIR", default_value = ".")]
        out: PathBuf,
        /// 覆盖已存在的文件
        #[arg(long)]
        overwrite: bool,
    },
}

pub(crate) fn run(command: DirectiveCmd) -> Result<()> {
    match command {
        DirectiveCmd::List { json } => list(json),
        DirectiveCmd::Show { name, json } => show(&name, json),
        DirectiveCmd::New {
            name,
            template,
            force,
            file,
        } => create::run(name.as_deref(), template.as_deref(), force, file.as_deref()),
        DirectiveCmd::Edit { name } => edit(&name),
        DirectiveCmd::Rm { name, yes } => remove(&name, yes),
        DirectiveCmd::Rename { from, to } => rename(&from, &to),
        DirectiveCmd::Import {
            path,
            folder,
            overwrite,
            dry_run,
        } => import(&path, folder.as_deref(), overwrite, dry_run),
        DirectiveCmd::Export {
            names,
            out,
            overwrite,
        } => export(&names, &out, overwrite),
    }
}

/// `list` 里的一行：库里的元信息 + 卡片要的摘要 + 上次执行。
///
/// `--json` 直接序列化这一份（`meta` 平铺），与 daemon 给宿主的条目**不是**同一个契约：
/// 那个是 IPC 的形状，改它要同时改宿主；这个是给人看的稳定字段。
#[derive(Serialize)]
struct Row<'a> {
    #[serde(flatten)]
    meta: &'a corex_store::DirectiveMeta,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_run: Option<&'a corex_engine::DirectiveHistory>,
}

fn list(json: bool) -> Result<()> {
    let library = Library::open()?;
    let ran = library
        .history()
        .map(|history| history.by_directive())
        .unwrap_or_default();
    let metas = library.records()?;
    if json {
        let rows: Vec<Row<'_>> = metas
            .iter()
            .map(|meta| Row {
                meta,
                last_run: ran.get(&meta.name),
            })
            .collect();
        outln!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }
    if metas.is_empty() {
        outln!("（指令库是空的；`corex directive new <名称>` 建一条）");
        return Ok(());
    }
    for meta in &metas {
        outln!("{}", row(meta, ran.get(&meta.name)));
    }
    Ok(())
}

/// 一行：`build            分组 3 步   2026-09-11 21:30  ✓ 13.25s  打包`。
fn row(meta: &corex_store::DirectiveMeta, last: Option<&corex_engine::DirectiveHistory>) -> String {
    let folder = meta.folder.as_deref().unwrap_or("-");
    let steps = meta
        .summary
        .as_ref()
        .map(|summary| format!("{} 步", summary.step_count))
        .unwrap_or_else(|| "解析失败".to_string());
    let spent = match last {
        Some(run) if run.ok => format!("✓ {}", elapsed(run.duration_ms)),
        Some(_) => "✗".to_string(),
        None => "未运行".to_string(),
    };
    let description = meta
        .summary
        .as_ref()
        .map(|summary| summary.description.as_str())
        .unwrap_or("");
    format!(
        "{:<20} {:<10} {:<8} {:<10} {}",
        meta.name, folder, steps, spent, description
    )
    .trim_end()
    .to_string()
}

fn elapsed(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else {
        format!("{:.2}s", ms as f64 / 1000.0)
    }
}

fn show(name: &str, json: bool) -> Result<()> {
    let library = Library::open()?;
    let record = library.fetch(name)?;
    if json {
        outln!("{}", serde_json::to_string_pretty(&record.definition)?);
        return Ok(());
    }
    outln!("{}", record.yaml);
    Ok(())
}

/// `edit`：库 → 临时文件 → `$EDITOR` → 校验 → 回库。
///
/// 走临时文件而不是内置一个 TUI 编辑器：指令是 YAML，用户的编辑器里已经有补全、折叠、
/// diff 与自己的键位，再实现一遍只会得到更差的一套。
fn edit(name: &str) -> Result<()> {
    let library = Library::open()?;
    let record = library.fetch(name)?;
    let path = edit_path(&record.name)?;
    std::fs::write(&path, &record.yaml)
        .with_context(|| format!("无法写入临时文件 {}", path.display()))?;

    editor::edit_and_wait(&path)?;

    let text = std::fs::read_to_string(&path)?;
    if text == record.yaml {
        let _ = std::fs::remove_file(&path);
        outln!("未修改 {}", record.name);
        return Ok(());
    }
    let directive = match checked(&library, &text) {
        Ok(directive) => directive,
        Err(error) => {
            // 改坏了别丢掉用户刚写的东西：留下文件并告诉他路径，改完再 `import` 回来。
            bail!(
                "{error}\n改动留在 {}（改好后再保存，或 `corex directive import {}` 重新收进来）",
                path.display(),
                path.display()
            );
        }
    };

    // 编辑器里改了 `name:` 就是改名；没写名字（或没动它）就用原来那个键。
    let target = match directive.name.trim() {
        "" => record.name.clone(),
        other => other.to_string(),
    };
    let saved = library.save(Some(&record.name), &target, &directive)?;
    let _ = std::fs::remove_file(&path);
    if saved.name == record.name {
        outln!("已保存 {}", saved.name);
    } else {
        outln!("已保存 {}（原名 {}）", saved.name, record.name);
    }
    Ok(())
}

/// 编辑用的临时文件落点：数据目录下，同名指令复用它（覆盖写），不堆积垃圾。
fn edit_path(name: &str) -> Result<PathBuf> {
    let dir = corex_ipc::data_dir()?.join("edit");
    std::fs::create_dir_all(&dir).with_context(|| format!("无法创建 {}", dir.display()))?;
    Ok(dir.join(format!("{name}.yaml")))
}

/// 解析 + 过 `run` 走的那两道门；给 `edit` 用，所以错误只要一句能读的原因。
fn checked(library: &Library, text: &str) -> Result<Directive, String> {
    let directive = Directive::from_yaml_str(text).map_err(|error| error.to_string())?;
    library.admission()(&directive)?;
    Ok(directive)
}

fn remove(name: &str, yes: bool) -> Result<()> {
    let library = Library::open()?;
    // 先取一次：删不存在的指令与「用户确认前先看清删的是哪条」都要这份内容。
    let record = library.fetch(name)?;
    if !yes {
        if !ask::is_interactive() {
            return Err(usage(format!("删除 {} 需要 --yes", record.name)));
        }
        let description = if record.definition.description.is_empty() {
            String::new()
        } else {
            format!("（{}）", record.definition.description)
        };
        if !ask::confirm(&format!("删掉 {}{description}？", record.name), false)? {
            outln!("已取消，未改动 {}", record.name);
            return Ok(());
        }
    }
    library.delete(&record.name)?;
    outln!("已删除 {}", record.name);
    Ok(())
}

fn rename(from: &str, to: &str) -> Result<()> {
    let library = Library::open()?;
    let saved = library.rename(from, to)?;
    outln!("已改名 {} → {}", from, saved.name);
    Ok(())
}

fn import(path: &Path, folder: Option<&str>, overwrite: bool, dry_run: bool) -> Result<()> {
    let library = Library::open()?;
    let opts = ImportOptions {
        is_overwrite: overwrite,
        is_dry_run: dry_run,
        folder: folder.map(str::to_owned),
    };
    let report = library.import(path, &opts)?;

    for entry in &report.entries {
        match &entry.status {
            ImportStatus::Failed(reason) => outln!("✗ {} — {reason}", entry.path.display()),
            ImportStatus::Skipped => outln!(
                "· {} — 已存在同名指令 `{}`（要覆盖加 --overwrite）",
                entry.path.display(),
                entry.name
            ),
            ImportStatus::Created => outln!("✓ {} → {}", entry.path.display(), entry.name),
            ImportStatus::Updated => outln!("↻ {} → {}", entry.path.display(), entry.name),
        }
    }
    if dry_run {
        outln!(
            "（--dry-run）新增 {} / 覆盖 {} / 跳过 {} / 失败 {}，未写库",
            report.created(),
            report.updated(),
            report.skipped(),
            report.failed()
        );
        return Ok(());
    }
    outln!(
        "新增 {} / 覆盖 {} / 跳过 {} / 失败 {}",
        report.created(),
        report.updated(),
        report.skipped(),
        report.failed()
    );
    if report.failed() > 0 {
        // 有失败就是「这次导入没做到你要的事」：给调用方一个非零退出码。
        return Err(usage(format!("{} 条导入失败", report.failed())));
    }
    Ok(())
}

fn export(names: &[String], out: &Path, overwrite: bool) -> Result<()> {
    let library = Library::open()?;
    if names.is_empty() {
        let written = library.export_dir(out, overwrite)?;
        outln!("已导出 {} 条 → {}", written.len(), out.display());
        return Ok(());
    }
    // 指名道姓时写到 `<out>/<名字>.yaml`，与整体导出的布局一致。
    std::fs::create_dir_all(out).with_context(|| format!("无法创建 {}", out.display()))?;
    for name in names {
        let path = out.join(format!("{name}.yaml"));
        if path.exists() && !overwrite {
            bail!("已存在: {}（要覆盖加 --overwrite）", path.display());
        }
        let yaml = library.export_yaml(name)?;
        std::fs::write(&path, yaml).with_context(|| format!("无法写入 {}", path.display()))?;
        outln!("✓ {}", path.display());
    }
    Ok(())
}
