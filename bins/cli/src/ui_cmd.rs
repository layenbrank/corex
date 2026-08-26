//! `corex ui` — interactive element probe commands.

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use corex_core::{RuntimeConfig, Value, RUNTIME_CONFIG};
use corex_registry::ui_probe;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Subcommand, Debug)]
pub enum UiCommands {
    /// List visible top-level windows
    Windows,
    /// List UIA elements under a window
    List {
        #[arg(long)]
        hwnd: Option<i64>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long, default_value = "3")]
        depth: i64,
        #[arg(long, default_value = "50")]
        limit: i64,
    },
    /// Find element by selector flags
    Find {
        #[arg(long)]
        hwnd: Option<i64>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        name_contains: Option<String>,
        #[arg(long)]
        automation_id: Option<String>,
        #[arg(long)]
        control_type: Option<String>,
        #[arg(long, default_value = "3000")]
        timeout_ms: i64,
    },
    /// Hit-test at screen coordinates (no overlay)
    At {
        #[arg(long)]
        x: i64,
        #[arg(long)]
        y: i64,
    },
    /// Interactive pick: hover highlight + click to capture selectors
    Pick {
        #[arg(long, help = "Limit picking to this top-level HWND")]
        scope_hwnd: Option<i64>,
        #[arg(long, help = "Copy selectors_yaml to clipboard (Windows clip.exe)")]
        copy_yaml: bool,
    },
}

fn load_runtime_config(data_dir: &Path) -> RuntimeConfig {
    let candidates = [
        PathBuf::from(RUNTIME_CONFIG),
        data_dir.join("config.toml"),
    ];
    for path in candidates {
        if path.exists() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(cfg) = toml::from_str::<RuntimeSectionWrapper>(&text) {
                    return cfg.into_runtime();
                }
            }
        }
    }
    RuntimeConfig::default()
}

#[derive(Debug, serde::Deserialize, Default)]
struct RuntimeSectionWrapper {
    #[serde(default)]
    runtime: Option<RuntimeSectionFields>,
}

#[derive(Debug, serde::Deserialize, Default)]
struct RuntimeSectionFields {
    #[serde(default)]
    ui_profile: Option<String>,
    #[serde(default)]
    ui_max_selector_chain: Option<usize>,
    #[serde(default)]
    ui_max_settle_ms: Option<u64>,
}

impl RuntimeSectionWrapper {
    fn into_runtime(self) -> RuntimeConfig {
        let mut cfg = RuntimeConfig::default();
        if let Some(r) = self.runtime {
            let overrides = corex_core::UiProfileOverrides {
                max_selector_chain: r.ui_max_selector_chain,
                max_settle_ms: r.ui_max_settle_ms,
            };
            if let Some(profile) = r.ui_profile {
                cfg.apply_ui_profile(&profile, overrides);
            } else {
                if let Some(n) = r.ui_max_selector_chain {
                    cfg.ui_max_selector_chain = n;
                }
                if let Some(ms) = r.ui_max_settle_ms {
                    cfg.ui_max_settle_ms = ms;
                }
            }
        }
        cfg
    }
}

fn scope_params(hwnd: Option<i64>, title: Option<String>) -> BTreeMap<String, Value> {
    let mut m = BTreeMap::new();
    if let Some(h) = hwnd {
        m.insert("hwnd".into(), Value::Int(h));
    }
    if let Some(t) = title {
        m.insert("title_contains".into(), Value::Str(t));
    }
    m
}

fn print_value(v: &Value) -> Result<()> {
    let json = v.to_json();
    println!("{}", serde_json::to_string_pretty(&json)?);
    Ok(())
}

fn copy_yaml_to_clipboard(yaml: &str) -> Result<()> {
    let mut child = Command::new("clip")
        .stdin(Stdio::piped())
        .spawn()
        .context("无法启动 clip.exe")?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(yaml.as_bytes())?;
    }
    let status = child.wait()?;
    if !status.success() {
        bail!("clip.exe 失败");
    }
    eprintln!("已复制 selectors_yaml 到剪贴板");
    Ok(())
}

pub async fn run(command: UiCommands, data_dir: &Path) -> Result<()> {
    let config = load_runtime_config(data_dir);
    let ctx = ui_probe::probe_context(config);

    match command {
        UiCommands::Windows => {
            let v = ui_probe::probe_windows()
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            print_value(&v)?;
        }
        UiCommands::List {
            hwnd,
            title,
            depth,
            limit,
        } => {
            let mut params = scope_params(hwnd, title);
            params.insert("depth".into(), Value::Int(depth));
            params.insert("limit".into(), Value::Int(limit));
            let v = ui_probe::probe_list(&ctx, params)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            print_value(&v)?;
        }
        UiCommands::Find {
            hwnd,
            title,
            name,
            name_contains,
            automation_id,
            control_type,
            timeout_ms,
        } => {
            let mut params = scope_params(hwnd, title);
            if let Some(n) = name {
                params.insert("name".into(), Value::Str(n));
            }
            if let Some(n) = name_contains {
                params.insert("name_contains".into(), Value::Str(n));
            }
            if let Some(a) = automation_id {
                params.insert("automation_id".into(), Value::Str(a));
            }
            if let Some(c) = control_type {
                params.insert("control_type".into(), Value::Str(c));
            }
            params.insert("timeout_ms".into(), Value::Int(timeout_ms));
            let v = ui_probe::probe_find(&ctx, params)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            print_value(&v)?;
        }
        UiCommands::At { x, y } => {
            let v = ui_probe::probe_at(x, y)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            print_value(&v)?;
        }
        UiCommands::Pick {
            scope_hwnd,
            copy_yaml,
        } => {
            #[cfg(windows)]
            {
                let v = corex_registry::ui_pick::probe_pick(scope_hwnd)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                if copy_yaml {
                    if let Value::Map(m) = &v {
                        if let Some(yaml) = m.get("selectors_yaml").and_then(|v| v.as_str()) {
                            copy_yaml_to_clipboard(yaml)?;
                        }
                    }
                }
                print_value(&v)?;
            }
            #[cfg(not(windows))]
            {
                let _ = (scope_hwnd, copy_yaml);
                bail!("ui pick 需要 Windows");
            }
        }
    }
    Ok(())
}
