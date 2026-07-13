use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::style::Stylize;
use notify_debouncer_full::{
    DebounceEventResult, new_debouncer, notify::EventKind, notify::RecursiveMode,
};

use crate::pipeline::config::{
    PipelineConfig, PipelinesConfig, WatchConfig, find_config_path, load_config, validate_config,
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

    let mut debouncers = Vec::new();

    for target in targets {
        let pipeline = target.pipeline.clone();
        let filter = target.filter;
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

        let last_finished_for_callback = Arc::clone(&last_finished);
        let mut debouncer = new_debouncer(
            Duration::from_millis(debounce_ms),
            None,
            move |result: DebounceEventResult| {
                if guard::is_in_cooldown(&last_finished_for_callback, cooldown) {
                    return;
                }
                if !should_trigger(&result, &filter) {
                    return;
                }
                guard::spawn(
                    Arc::clone(&running),
                    &pipeline,
                    &variables,
                    "变更",
                    Some(Arc::clone(&last_finished_for_callback)),
                );
            },
        )
        .with_context(|| format!("创建 debouncer 失败: pipeline '{pipeline_id}'"))?;

        for path in &target.paths {
            let mode = if path.is_dir() {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            debouncer.watch(path, mode).with_context(|| {
                format!(
                    "监听路径失败: {} (pipeline '{pipeline_id}')",
                    path.display()
                )
            })?;
        }

        debouncers.push(debouncer);
    }

    println!("  {} 等待文件变更...（Ctrl+C 退出）\n", "⏳".yellow());

    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}

fn should_trigger(result: &DebounceEventResult, filter: &Filter) -> bool {
    let Ok(events) = result else {
        return false;
    };

    let paths: Vec<PathBuf> = events
        .iter()
        .filter(|event| is_actionable_kind(&event.kind))
        .flat_map(|event| event.paths.clone())
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
    use notify_debouncer_full::DebouncedEvent;
    use notify_debouncer_full::notify::Event;
    use notify_debouncer_full::notify::EventKind;
    use notify_debouncer_full::notify::event::{AccessKind, ModifyKind};
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
        let event = Event {
            kind: EventKind::Access(AccessKind::Read),
            paths: vec![PathBuf::from("foo.js")],
            attrs: Default::default(),
        };
        let debounced = vec![DebouncedEvent::new(event, Instant::now())];
        let filter = Filter::default();
        assert!(!should_trigger(&Ok(debounced), &filter));
    }

    #[test]
    fn modify_event_triggers_when_not_filtered() {
        let event = Event {
            kind: EventKind::Modify(ModifyKind::Any),
            paths: vec![PathBuf::from("foo.js")],
            attrs: Default::default(),
        };
        let debounced = vec![DebouncedEvent::new(event, Instant::now())];
        let filter = Filter::default();
        assert!(should_trigger(&Ok(debounced), &filter));
    }

    #[test]
    fn is_actionable_kind_filters_access_only() {
        assert!(!is_actionable_kind(&EventKind::Access(AccessKind::Read)));
        assert!(is_actionable_kind(&EventKind::Modify(ModifyKind::Any)));
    }
}
