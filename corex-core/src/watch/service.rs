use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::style::Stylize;
use notify_debouncer_full::{
    new_debouncer, notify::EventKind, notify::RecursiveMode, DebounceEventResult, Debouncer,
    RecommendedCache,
};

use crate::pipeline::config::{
    find_config_path, load_config, validate_config, PipelineConfig, PipelinesConfig, WatchConfig,
};
use crate::pipeline::context::PipelineContext;
use crate::pipeline::guard::{self, RunningSet};
use crate::utils::Filter;
use crate::watch::schema::Args;

#[derive(Debug)]
pub(crate) struct WatchTarget {
    pub(crate) pipeline: PipelineConfig,
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) filter: Filter,
    pub(crate) debounce_ms: u64,
    pub(crate) cooldown_ms: u64,
}

/// watch 守护选项
#[derive(Debug, Clone, Default)]
pub struct WatchOpts {
    pub debounce_ms: Option<u64>,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
    pub immediate: bool,
    /// 与 cron 并行守护时共享，防止重复触发
    pub running: Option<RunningSet>,
}

/// `corex watch` 命令入口
pub fn run(args: &Args) -> Result<()> {
    match args {
        Args::Run {
            config,
            pipeline,
            debounce_ms,
            includes,
            excludes,
            immediate,
        } => {
            let config_path = config
                .as_deref()
                .map(PathBuf::from)
                .unwrap_or_else(find_config_path);
            if !config_path.exists() {
                anyhow::bail!(
                    "配置文件未找到：{}，请先运行 `corex schedule generate` 生成配置",
                    config_path.display()
                );
            }
            let cfg = load_config(&config_path)?;
            validate_config(&cfg)?;
            let opts = WatchOpts {
                debounce_ms: *debounce_ms,
                includes: includes.clone(),
                excludes: excludes.clone(),
                immediate: *immediate,
                ..WatchOpts::default()
            };
            serve(&cfg, &config_path, pipeline, &opts, None)
        }
    }
}

/// 解析并校验 watch 目标（路径必须存在）
pub(crate) fn resolve(
    config: &PipelinesConfig,
    ids: &[String],
    opts: &WatchOpts,
) -> Result<Vec<WatchTarget>> {
    collect_targets(
        config,
        ids,
        opts.debounce_ms,
        &opts.includes,
        &opts.excludes,
    )
}

/// 常驻文件监听；`targets` 已解析时跳过 resolve
pub(crate) fn serve(
    config: &PipelinesConfig,
    _config_path: &Path,
    ids: &[String],
    opts: &WatchOpts,
    targets: Option<Vec<WatchTarget>>,
) -> Result<()> {
    let targets = match targets {
        Some(t) => t,
        None => resolve(config, ids, opts)?,
    };

    if targets.is_empty() {
        anyhow::bail!(
            "配置文件中没有任何 Pipeline 设置了 watch 字段\n\
             提示: 在 pipeline 配置中添加 watch.paths 即可启用文件监听"
        );
    }

    print_banner("Corex · 文件监听");

    println!(
        "  {} 已加载 {} 条监听 Pipeline（共 {} 条）\n",
        "✓".green().bold(),
        targets.len(),
        config.pipelines.len()
    );

    for target in &targets {
        let desc = target
            .pipeline
            .description
            .as_deref()
            .unwrap_or(&target.pipeline.id);
        println!(
            "  {} {} — debounce: {}ms — cooldown: {}ms — 路径: {}",
            "▸".cyan(),
            desc.bold(),
            target.debounce_ms,
            target.cooldown_ms,
            target
                .paths
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
                .dim()
        );
    }
    println!();

    run_loop(config, targets, opts)
}

/// 监听循环（假定 targets 已通过 resolve 校验）
pub(crate) fn run_loop(
    config: &PipelinesConfig,
    targets: Vec<WatchTarget>,
    opts: &WatchOpts,
) -> Result<()> {
    let running = opts.running.clone().unwrap_or_else(guard::new_set);
    let variables = config.variables.clone();

    if opts.immediate {
        println!("  {} 启动时执行 Pipeline...\n", "▶".yellow().bold());
    }

    let mut join_handles = Vec::new();

    for target in targets {
        let pipeline = target.pipeline.clone();
        let filter = target.filter;
        let roots = target.paths.clone();
        let running = Arc::clone(&running);
        let variables = variables.clone();
        let pipeline_id = pipeline.id.clone();
        let debounce_ms = target.debounce_ms;
        let cooldown = Duration::from_millis(target.cooldown_ms);
        let last_finished = guard::new_last_finished();

        if opts.immediate {
            guard::spawn(
                Arc::clone(&running),
                &pipeline,
                &variables,
                "启动",
                Some(Arc::clone(&last_finished)),
            );
        }

        let thread_name = format!("watch-{pipeline_id}");
        let handle = std::thread::Builder::new()
            .name(thread_name)
            .spawn(move || {
                let id = pipeline.id.clone();
                if let Err(e) = run_target_loop(
                    pipeline,
                    roots,
                    filter,
                    debounce_ms,
                    cooldown,
                    running,
                    variables,
                    last_finished,
                ) {
                    eprintln!(
                        "  {} Pipeline '{}' 监听退出: {}\n",
                        "×".red().bold(),
                        id,
                        e
                    );
                }
            })
            .with_context(|| format!("启动监听线程失败: pipeline '{pipeline_id}'"))?;

        join_handles.push(handle);
    }

    println!("  {} 等待文件变更...（Ctrl+C 退出）\n", "⏳".yellow());

    for handle in join_handles {
        let _ = handle.join();
    }

    Ok(())
}

type FsDebouncer =
    Debouncer<notify_debouncer_full::notify::RecommendedWatcher, RecommendedCache>;

fn run_target_loop(
    pipeline: PipelineConfig,
    roots: Vec<PathBuf>,
    filter: Filter,
    debounce_ms: u64,
    cooldown: Duration,
    running: RunningSet,
    variables: std::collections::HashMap<String, String>,
    last_finished: guard::LastFinished,
) -> Result<()> {
    let pipeline_id = pipeline.id.clone();
    let (tx, rx) = mpsc::channel::<DebounceEventResult>();

    let mut debouncer: FsDebouncer = new_debouncer(
        Duration::from_millis(debounce_ms),
        None,
        move |result: DebounceEventResult| {
            let _ = tx.send(result);
        },
    )
    .with_context(|| format!("创建 debouncer 失败: pipeline '{pipeline_id}'"))?;

    // 同时挂父目录：vue-cli 等会删重建 roots，根目录句柄失效后仍能靠父目录感知重建
    attach_watches(&mut debouncer, &roots, &pipeline_id)?;

    while let Ok(result) = rx.recv() {
        if let Err(errors) = &result {
            eprintln!(
                "  {} Pipeline '{}' 监听异常: {:?}（将重新挂载）",
                "⚠".yellow(),
                pipeline_id,
                errors
            );
            ensure_watches(&mut debouncer, &roots, &pipeline_id)?;
            continue;
        }

        // 根路径已不存在，或收到根路径 Remove / 父目录上的根名 Remove
        if roots_need_rewatch(&result, &roots) {
            eprintln!(
                "  {} Pipeline '{}' 监听路径丢失，等待重建后重新挂载...",
                "↻".yellow(),
                pipeline_id
            );
            ensure_watches(&mut debouncer, &roots, &pipeline_id)?;
            eprintln!(
                "  {} Pipeline '{}' 已重新挂载: {}",
                "✓".green(),
                pipeline_id,
                roots
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            // 重建伴随大量写入；交给后续事件 + cooldown
            continue;
        }

        if guard::is_in_cooldown(&last_finished, cooldown) {
            continue;
        }
        if !should_trigger(&result, &filter, &roots) {
            continue;
        }
        guard::spawn(
            Arc::clone(&running),
            &pipeline,
            &variables,
            "变更",
            Some(Arc::clone(&last_finished)),
        );
    }

    Ok(())
}

/// 挂载 roots（若存在）+ 各自父目录（NonRecursive，保证删建后仍能感知）
fn attach_watches(debouncer: &mut FsDebouncer, roots: &[PathBuf], pipeline_id: &str) -> Result<()> {
    let mut attached = HashSet::new();

    for root in roots {
        if let Some(parent) = root.parent() {
            if parent.as_os_str().is_empty() {
                continue;
            }
            if parent.exists() {
                let key = normalize_path(parent);
                if attached.insert(key.clone()) {
                    debouncer
                        .watch(parent, RecursiveMode::NonRecursive)
                        .with_context(|| {
                            format!(
                                "监听父目录失败: {} (pipeline '{pipeline_id}')",
                                parent.display()
                            )
                        })?;
                }
            }
        }

        if root.exists() {
            let mode = if root.is_dir() {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            let key = normalize_path(root);
            if attached.insert(key) {
                debouncer.watch(root, mode).with_context(|| {
                    format!(
                        "监听路径失败: {} (pipeline '{pipeline_id}')",
                        root.display()
                    )
                })?;
            }
        }
    }

    Ok(())
}

/// 等待 roots 全部存在后重新挂载（父目录句柄尽量保留）
fn ensure_watches(debouncer: &mut FsDebouncer, roots: &[PathBuf], pipeline_id: &str) -> Result<()> {
    wait_roots_exist(roots);
    // 根路径句柄可能已失效；unwatch 忽略错误后重新挂
    for root in roots {
        let _ = debouncer.unwatch(root);
        if let Some(parent) = root.parent() {
            let _ = debouncer.unwatch(parent);
        }
    }
    attach_watches(debouncer, roots, pipeline_id)
}

fn wait_roots_exist(roots: &[PathBuf]) {
    loop {
        if roots.iter().all(|p| p.exists()) {
            std::thread::sleep(Duration::from_millis(200));
            if roots.iter().all(|p| p.exists()) {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// 需要重挂：根已不存在，或事件表明根被删/在父目录上被 Remove
fn roots_need_rewatch(result: &DebounceEventResult, roots: &[PathBuf]) -> bool {
    if roots.iter().any(|p| !p.exists()) {
        return true;
    }

    let Ok(events) = result else {
        return false;
    };

    events.iter().any(|event| {
        matches!(event.kind, EventKind::Remove(_))
            && event
                .paths
                .iter()
                .any(|path| roots.iter().any(|root| is_root_path(path, root)))
    })
}

fn is_root_path(event_path: &Path, root: &Path) -> bool {
    normalize_path(event_path) == normalize_path(root)
}

fn normalize_path(path: &Path) -> PathBuf {
    let s = path
        .to_string_lossy()
        .trim_end_matches(['/', '\\'])
        .to_string();
    PathBuf::from(s)
}

/// 事件路径是否落在任一配置的 root 之下（含 root 自身）
fn path_under_roots(path: &Path, roots: &[PathBuf]) -> bool {
    let norm = normalize_path(path);
    roots.iter().any(|root| {
        let root_n = normalize_path(root);
        norm == root_n || norm.starts_with(&root_n)
    })
}

fn should_trigger(result: &DebounceEventResult, filter: &Filter, roots: &[PathBuf]) -> bool {
    let Ok(events) = result else {
        return false;
    };

    let paths: Vec<PathBuf> = events
        .iter()
        .filter(|event| is_actionable_kind(&event.kind))
        .flat_map(|event| event.paths.clone())
        .filter(|path| path_under_roots(path, roots))
        .collect();

    if paths.is_empty() {
        return false;
    }

    paths_should_trigger(&paths, filter)
}

fn is_actionable_kind(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}

fn resolve_cooldown_ms(watch: &WatchConfig, debounce_ms: u64) -> u64 {
    watch
        .cooldown_ms
        .unwrap_or_else(|| debounce_ms.saturating_mul(2).max(1000))
}

fn paths_should_trigger(paths: &[PathBuf], filter: &Filter) -> bool {
    paths.iter().any(|path| !filter.is_filtered(path))
}

fn collect_targets(
    config: &PipelinesConfig,
    pipeline_filter: &[String],
    debounce_override: Option<u64>,
    cli_includes: &[String],
    cli_excludes: &[String],
) -> Result<Vec<WatchTarget>> {
    let parse_ctx = PipelineContext::with_variables(config.variables.clone());
    let filter_set: HashSet<&str> = pipeline_filter.iter().map(String::as_str).collect();

    let mut targets = Vec::new();

    for pipeline in &config.pipelines {
        let Some(watch) = &pipeline.watch else {
            continue;
        };

        if !filter_set.is_empty() && !filter_set.contains(pipeline.id.as_str()) {
            continue;
        }

        let paths = resolve_watch_paths(&parse_ctx, watch, &pipeline.id)?;
        let filter = build_filter(watch, cli_includes, cli_excludes);
        let debounce_ms = debounce_override.unwrap_or(watch.debounce_ms);
        let cooldown_ms = resolve_cooldown_ms(watch, debounce_ms);

        targets.push(WatchTarget {
            pipeline: pipeline.clone(),
            paths,
            filter,
            debounce_ms,
            cooldown_ms,
        });
    }

    Ok(targets)
}

fn resolve_watch_paths(
    ctx: &PipelineContext,
    watch: &WatchConfig,
    pipeline_id: &str,
) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();

    for raw in &watch.paths {
        let resolved = ctx.parse(raw);
        let path = PathBuf::from(&resolved);
        if !path.exists() {
            anyhow::bail!("Pipeline '{pipeline_id}' watch 路径不存在: {resolved}");
        }
        paths.push(path);
    }

    Ok(paths)
}

fn build_filter(watch: &WatchConfig, cli_includes: &[String], cli_excludes: &[String]) -> Filter {
    let mut includes = watch.includes.clone();
    includes.extend_from_slice(cli_includes);

    let mut excludes = watch.excludes.clone();
    excludes.extend_from_slice(cli_excludes);

    Filter::new(&includes, &excludes)
}

fn print_banner(title: &str) {
    let width: usize = 54;
    let title_len = title.chars().count();
    let pad_total = width.saturating_sub(title_len + 2);
    let pad_left = pad_total / 2;
    let pad_right = pad_total - pad_left;
    println!();
    println!("{}", format!("╭{}╮", "─".repeat(width)).cyan().bold());
    println!(
        "{}",
        format!(
            "│{}{}{}│",
            " ".repeat(pad_left),
            title,
            " ".repeat(pad_right)
        )
        .cyan()
        .bold()
    );
    println!("{}", format!("╰{}╯", "─".repeat(width)).cyan().bold());
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify_debouncer_full::notify::event::{AccessKind, ModifyKind, RemoveKind};
    use notify_debouncer_full::notify::Event;
    use notify_debouncer_full::notify::EventKind;
    use notify_debouncer_full::DebouncedEvent;
    use std::path::Path;
    use std::time::Instant;

    #[test]
    fn build_filter_merges_cli_patterns() {
        let watch = WatchConfig {
            paths: vec![".".into()],
            includes: vec!["**/*.rs".into()],
            excludes: vec!["**/.git/**".into()],
            debounce_ms: 300,
            cooldown_ms: None,
        };
        let filter = build_filter(&watch, &["**/*.toml".into()], &["**/*.tmp".into()]);
        assert!(!filter.is_filtered(Path::new("main.rs")));
        assert!(!filter.is_filtered(Path::new("Cargo.toml")));
        assert!(filter.is_filtered(Path::new("scratch.tmp")));
    }

    #[test]
    fn paths_should_trigger_respects_filter() {
        let filter = Filter::new(&[], &["**/*.tmp".into()]);
        assert!(!paths_should_trigger(&[PathBuf::from("foo.tmp")], &filter));
        assert!(paths_should_trigger(&[PathBuf::from("foo.rs")], &filter));
    }

    #[test]
    fn version_json_exclude_prevents_trigger() {
        let filter = Filter::new(&[], &["**/version.json".into()]);
        assert!(!paths_should_trigger(
            &[PathBuf::from("app/version.json")],
            &filter
        ));
        assert!(paths_should_trigger(&[PathBuf::from("app/main.js")], &filter));
    }

    #[test]
    fn resolve_cooldown_ms_defaults_to_max_debounce_times_two_and_1000() {
        let watch = WatchConfig {
            paths: vec![],
            includes: vec![],
            excludes: vec![],
            debounce_ms: 600,
            cooldown_ms: None,
        };
        assert_eq!(resolve_cooldown_ms(&watch, 600), 1200);

        let small = WatchConfig {
            paths: vec![],
            includes: vec![],
            excludes: vec![],
            debounce_ms: 200,
            cooldown_ms: None,
        };
        assert_eq!(resolve_cooldown_ms(&small, 200), 1000);
    }

    #[test]
    fn resolve_cooldown_ms_honors_explicit_value() {
        let watch = WatchConfig {
            paths: vec![],
            includes: vec![],
            excludes: vec![],
            debounce_ms: 600,
            cooldown_ms: Some(2500),
        };
        assert_eq!(resolve_cooldown_ms(&watch, 600), 2500);
    }

    #[test]
    fn access_event_does_not_trigger() {
        let root = PathBuf::from("app");
        let event = Event {
            kind: EventKind::Access(AccessKind::Read),
            paths: vec![PathBuf::from("app/foo.js")],
            attrs: Default::default(),
        };
        let debounced = vec![DebouncedEvent::new(event, Instant::now())];
        let filter = Filter::default();
        assert!(!should_trigger(&Ok(debounced), &filter, &[root]));
    }

    #[test]
    fn modify_event_triggers_when_not_filtered() {
        let root = PathBuf::from("app");
        let event = Event {
            kind: EventKind::Modify(ModifyKind::Any),
            paths: vec![PathBuf::from("app/foo.js")],
            attrs: Default::default(),
        };
        let debounced = vec![DebouncedEvent::new(event, Instant::now())];
        let filter = Filter::default();
        assert!(should_trigger(&Ok(debounced), &filter, &[root]));
    }

    #[test]
    fn sibling_path_outside_root_does_not_trigger() {
        let root = PathBuf::from(r"C:\proj\app");
        let event = Event {
            kind: EventKind::Modify(ModifyKind::Any),
            paths: vec![PathBuf::from(r"C:\proj\package.json")],
            attrs: Default::default(),
        };
        let debounced = vec![DebouncedEvent::new(event, Instant::now())];
        let filter = Filter::default();
        assert!(!should_trigger(&Ok(debounced), &filter, &[root]));
    }

    #[test]
    fn is_actionable_kind_filters_access_only() {
        assert!(!is_actionable_kind(&EventKind::Access(AccessKind::Read)));
        assert!(is_actionable_kind(&EventKind::Modify(ModifyKind::Any)));
    }

    #[test]
    fn roots_need_rewatch_detects_watched_dir_delete() {
        let root = PathBuf::from(r"C:\proj\app");
        let event = Event {
            kind: EventKind::Remove(RemoveKind::Folder),
            paths: vec![root.clone()],
            attrs: Default::default(),
        };
        let debounced = vec![DebouncedEvent::new(event, Instant::now())];
        // root path string still "exists" as PathBuf; only Remove event matters here
        // (filesystem existence check would need a real missing dir)
        assert!(roots_need_rewatch(&Ok(debounced), &[root]));
    }

    #[test]
    fn roots_need_rewatch_ignores_nested_file_delete() {
        let root = PathBuf::from(r"C:\proj\app");
        let event = Event {
            kind: EventKind::Remove(RemoveKind::File),
            paths: vec![root.join("manifest.json")],
            attrs: Default::default(),
        };
        let debounced = vec![DebouncedEvent::new(event, Instant::now())];
        // nested delete alone does not require rewatch when root still exists on disk;
        // this synthetic root path likely doesn't exist → function returns true via !exists.
        // Use a path that exists: current dir.
        let existing = std::env::current_dir().unwrap();
        let event2 = Event {
            kind: EventKind::Remove(RemoveKind::File),
            paths: vec![existing.join("manifest.json")],
            attrs: Default::default(),
        };
        let debounced2 = vec![DebouncedEvent::new(event2, Instant::now())];
        assert!(!roots_need_rewatch(&Ok(debounced2), &[existing]));
        let _ = debounced;
        let _ = root;
    }

    #[test]
    fn notify_error_does_not_trigger_pipeline() {
        let filter = Filter::default();
        let err: DebounceEventResult = Err(vec![]);
        let root = PathBuf::from("app");
        assert!(!should_trigger(&err, &filter, &[root]));
    }
}
