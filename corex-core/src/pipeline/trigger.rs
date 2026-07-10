use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use crossterm::style::Stylize;

use crate::pipeline::config::{PipelineArgs, PipelineConfig, PipelinesConfig};
use crate::pipeline::context::PipelineContext;
use crate::pipeline::guard::{self, RunningSet};
use crate::pipeline::orchestrator::run_pipeline as orchestrate;
use crate::pipeline::report::{RunStatus, write_report};
use crate::runtime;
use crate::schedule;
use crate::watch::{self, WatchOpts, WatchTarget};

/// yaml 中声明的触发源
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Triggers {
    pub watch: bool,
    pub cron: bool,
}

impl Triggers {
    pub fn any(self) -> bool {
        self.watch || self.cron
    }
}

impl PipelineConfig {
    pub fn triggers(&self) -> Triggers {
        Triggers {
            watch: self.watch.is_some(),
            cron: self.schedule.is_some(),
        }
    }
}

/// 运行模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    Once,
    Watch,
    Cron,
    Dual,
}

pub fn run_mode(pipeline: &PipelineConfig, once: bool) -> RunMode {
    if once {
        return RunMode::Once;
    }
    match pipeline.triggers() {
        Triggers {
            watch: true,
            cron: true,
        } => RunMode::Dual,
        Triggers {
            watch: true,
            cron: false,
        } => RunMode::Watch,
        Triggers {
            watch: false,
            cron: true,
        } => RunMode::Cron,
        Triggers {
            watch: false,
            cron: false,
        } => RunMode::Once,
    }
}

/// 交互菜单触发器徽章
pub fn label(pipeline: &PipelineConfig) -> &'static str {
    match pipeline.triggers() {
        Triggers {
            watch: true,
            cron: true,
        } => "watch+cron",
        Triggers {
            watch: true,
            cron: false,
        } => "watch",
        Triggers {
            watch: false,
            cron: true,
        } => "cron",
        Triggers {
            watch: false,
            cron: false,
        } => "",
    }
}

/// 按 yaml 触发器分发 pipeline 执行
pub fn run(
    pipeline: &PipelineConfig,
    config: &PipelinesConfig,
    config_path: &Path,
    args: &PipelineArgs,
) -> Result<()> {
    let mode = run_mode(pipeline, args.once);
    let watch_targets = if mode != RunMode::Once {
        Some(check(pipeline, config, mode)?)
    } else {
        None
    };

    match mode {
        RunMode::Once => run_once(pipeline, config, args),
        RunMode::Watch => {
            watch::serve(
                config,
                config_path,
                &[pipeline.id.clone()],
                &WatchOpts {
                    immediate: true,
                    ..WatchOpts::default()
                },
                watch_targets,
            )
        }
        RunMode::Cron => {
            let ids = vec![pipeline.id.clone()];
            schedule::serve(config, Some(&ids))
        }
        RunMode::Dual => serve_dual(
            pipeline,
            config,
            watch_targets.expect("dual mode checked"),
        ),
    }
}

fn run_once(
    pipeline: &PipelineConfig,
    config: &PipelinesConfig,
    args: &PipelineArgs,
) -> Result<()> {
    let mut ctx = PipelineContext::with_variables(config.variables.clone());
    let report = orchestrate(pipeline, &mut ctx)?;

    if runtime::is_json_output() {
        runtime::state().emitter.json(&report)?;
    }

    if let Some(ref path) = args.report_file {
        write_report(path, &report)?;
    }

    if report.status == RunStatus::Failed {
        return Err(report.into_err());
    }

    Ok(())
}

/// 校验触发配置；Dual 模式返回已解析的 watch 目标
fn check(
    pipeline: &PipelineConfig,
    config: &PipelinesConfig,
    mode: RunMode,
) -> Result<Vec<WatchTarget>> {
    let ids = [pipeline.id.clone()];
    let watch_opts = WatchOpts {
        immediate: true,
        ..WatchOpts::default()
    };

    match mode {
        RunMode::Watch => {
            let targets = watch::resolve(config, &ids, &watch_opts)?;
            if targets.is_empty() {
                anyhow::bail!("Pipeline '{}' 未配置有效的 watch", pipeline.id);
            }
            Ok(targets)
        }
        RunMode::Cron => {
            schedule::check_cron(config, Some(&ids))?;
            Ok(Vec::new())
        }
        RunMode::Dual => {
            let targets = watch::resolve(config, &ids, &watch_opts)?;
            if targets.is_empty() {
                anyhow::bail!("Pipeline '{}' 未配置有效的 watch", pipeline.id);
            }
            schedule::check_cron(config, Some(&ids))?;
            Ok(targets)
        }
        RunMode::Once => Ok(Vec::new()),
    }
}

fn serve_dual(
    pipeline: &PipelineConfig,
    config: &PipelinesConfig,
    targets: Vec<WatchTarget>,
) -> Result<()> {
    let id = pipeline.id.clone();

    println!(
        "\n  {} 并行守护：{}（watch + cron）\n",
        "▶".green().bold(),
        pipeline
            .description
            .as_deref()
            .unwrap_or(&pipeline.id)
            .bold()
    );

    for target in &targets {
        let paths = target
            .paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "  {} watch — debounce: {}ms — {}",
            "▸".cyan(),
            target.debounce_ms,
            paths.dim()
        );
    }
    println!();

    let running: RunningSet = guard::new_set();
    let cfg = config.clone();
    let cron_id = id.clone();
    let running_cron = Arc::clone(&running);
    std::thread::spawn(move || {
        if let Err(e) = crate::schedule::service::loop_for(&cfg, &[cron_id], running_cron) {
            eprintln!(
                "  {} cron 守护失败: {}\n",
                "×".red().bold(),
                e
            );
        }
    });

    watch::run_loop(
        config,
        targets,
        &WatchOpts {
            immediate: true,
            running: Some(running),
            ..WatchOpts::default()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::config::WatchConfig;

    fn pipeline_with(watch: bool, cron: bool) -> PipelineConfig {
        PipelineConfig {
            id: "demo".into(),
            description: None,
            schedule: cron.then(|| "0 * * * * *".into()),
            watch: watch.then(|| WatchConfig {
                paths: vec![".".into()],
                includes: vec![],
                excludes: vec![],
                debounce_ms: 300,
            }),
            steps: vec![],
        }
    }

    #[test]
    fn run_mode_respects_once_flag() {
        let p = pipeline_with(true, true);
        assert_eq!(run_mode(&p, true), RunMode::Once);
        assert_eq!(run_mode(&p, false), RunMode::Dual);
    }

    #[test]
    fn label_formats_triggers() {
        assert_eq!(label(&pipeline_with(true, true)), "watch+cron");
        assert_eq!(label(&pipeline_with(false, true)), "cron");
        assert_eq!(label(&pipeline_with(false, false)), "");
    }

    #[test]
    fn check_cron_rejects_bad_expression() {
        let mut cfg = PipelinesConfig {
            version: 3,
            variables: Default::default(),
            pipelines: vec![PipelineConfig {
                id: "bad".into(),
                description: None,
                schedule: Some("not a cron".into()),
                watch: None,
                steps: vec![],
            }],
        };
        let err = schedule::check_cron(&cfg, Some(&["bad".into()]))
            .unwrap_err()
            .to_string();
        assert!(err.contains("cron 表达式无效"));

        cfg.pipelines[0].schedule = Some("0 * * * * *".into());
        schedule::check_cron(&cfg, Some(&["bad".into()])).unwrap();
    }

    #[test]
    fn check_rejects_missing_watch_path() {
        let cfg = PipelinesConfig {
            version: 3,
            variables: Default::default(),
            pipelines: vec![PipelineConfig {
                id: "missing".into(),
                description: None,
                schedule: None,
                watch: Some(WatchConfig {
                    paths: vec!["/nonexistent/corex-watch-test".into()],
                    includes: vec![],
                    excludes: vec![],
                    debounce_ms: 300,
                }),
                steps: vec![],
            }],
        };
        let p = &cfg.pipelines[0];
        let err = check(p, &cfg, RunMode::Watch)
            .unwrap_err()
            .to_string();
        assert!(err.contains("watch 路径不存在"));
    }
}
