//! `corex ui` —— 交互式元素探测命令。

use crate::output::{errln, outln};
#[cfg(windows)]
use anyhow::Context;
use anyhow::{Result, bail};
use clap::{Subcommand, ValueEnum};
use corex_core::{RuntimeConfig, Value};
use corex_engine::{AuditEntry, ExecutionAudit};
use corex_registry::ui_probe::{self, TreeFormat};
use std::collections::BTreeMap;
#[cfg(windows)]
use std::io::Write;
use std::path::Path;
#[cfg(windows)]
use std::process::{Command, Stdio};
use std::time::Instant;

#[derive(Subcommand, Debug)]
pub enum UiCommands {
    /// 窗口级探测
    Window {
        #[command(subcommand)]
        cmd: WindowCmd,
    },
    /// 窗口内 UIA 元素探测
    Element {
        #[command(subcommand)]
        cmd: ElementCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum WindowCmd {
    /// 列出可见的顶层窗口
    List,
    /// 列出桌面图标
    Desktop,
}

#[derive(Subcommand, Debug)]
pub enum ElementCmd {
    /// 列出窗口下的 UIA 元素（需要 --hwnd 或 --title）
    Tree {
        #[arg(long)]
        hwnd: Option<i64>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long, default_value = "3")]
        depth: i64,
        #[arg(long, default_value = "50")]
        limit: i64,
        #[arg(long, value_enum, default_value_t = OutputFormat::Flat)]
        format: OutputFormat,
        #[arg(long, help = "输出时隐去元素名称字段")]
        redact: bool,
    },
    /// 按 selector 参数查找元素（需要窗口范围）
    Get {
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
        #[arg(long, help = "Win32 / UIA 类名")]
        class: Option<String>,
        #[arg(long, default_value = "3000")]
        timeout_ms: i64,
        #[arg(long)]
        redact: bool,
    },
    /// 按屏幕坐标做命中测试（不显示浮层）
    Point {
        #[arg(long)]
        x: i64,
        #[arg(long)]
        y: i64,
        #[arg(long)]
        redact: bool,
    },
    /// 交互式拾取：悬停高亮 + 点击采集 selector
    Pick {
        #[arg(long, help = "把拾取范围限制在该顶层 HWND")]
        scope_hwnd: Option<i64>,
        #[arg(long, help = "把 selectors_yaml 复制到剪贴板（Windows clip.exe）")]
        copy_yaml: bool,
        #[arg(long)]
        redact: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Flat,
    Tree,
}

impl OutputFormat {
    fn tree_format(self) -> TreeFormat {
        match self {
            OutputFormat::Flat => TreeFormat::Flat,
            OutputFormat::Tree => TreeFormat::Tree,
        }
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

fn redact_value(v: &mut Value) {
    match v {
        Value::Map(m) => {
            if let Some(Value::Str(_)) = m.get_mut("name") {
                m.insert("name".into(), Value::Str("***".into()));
            }
            if let Some(Value::Str(_)) = m.get_mut("automation_id") {
                m.insert("automation_id".into(), Value::Str("***".into()));
            }
            for val in m.values_mut() {
                redact_value(val);
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_value(item);
            }
        }
        _ => {}
    }
}

fn print_value(v: &Value, redact: bool) -> Result<()> {
    let mut out = v.clone();
    if redact {
        redact_value(&mut out);
    }
    let json = out.to_json();
    outln!("{}", serde_json::to_string_pretty(&json)?);
    Ok(())
}

#[cfg(windows)]
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
    errln!("已复制 selectors_yaml 到剪贴板");
    Ok(())
}

fn record_probe_audit(
    data_dir: &Path,
    action_id: &str,
    duration_ms: u64,
    result: Result<(), &corex_core::ActionError>,
) {
    if let Ok(audit) = ExecutionAudit::under_data_dir(data_dir) {
        let entry = AuditEntry::from_action("ui.probe", "probe", action_id, duration_ms, result);
        let _ = audit.append(&entry);
    }
}

async fn run_probe<F>(
    data_dir: &Path,
    config: &RuntimeConfig,
    action_id: &str,
    f: F,
) -> Result<Value>
where
    F: std::future::Future<Output = Result<Value, corex_core::ActionError>>,
{
    let registry = crate::build_registry();
    // 保留错误类型：`ActionError::PermissionDenied` 必须仍可 downcast，
    // 退出码才能报“拒绝”（3）而不是“失败”（1）。
    ui_probe::check_probe_allowed(config, &registry, action_id).map_err(anyhow::Error::new)?;
    let t0 = Instant::now();
    let result = f.await;
    let duration_ms = t0.elapsed().as_millis() as u64;
    record_probe_audit(
        data_dir,
        action_id,
        duration_ms,
        result.as_ref().map(|_| ()),
    );
    result.map_err(anyhow::Error::new)
}

pub async fn run(command: UiCommands, data_dir: &Path) -> Result<()> {
    let config = crate::settings::effective().clone();
    let ctx = ui_probe::probe_context(config.clone());

    match command {
        UiCommands::Window { cmd } => match cmd {
            WindowCmd::List => {
                let v = run_probe(data_dir, &config, "ui.window.list", async {
                    ui_probe::probe_windows().await
                })
                .await?;
                print_value(&v, false)?;
            }
            WindowCmd::Desktop => {
                let v = run_probe(data_dir, &config, "ui.window.desktop", async {
                    ui_probe::probe_desktop_icons().await
                })
                .await?;
                print_value(&v, false)?;
            }
        },
        UiCommands::Element { cmd } => match cmd {
            ElementCmd::Tree {
                hwnd,
                title,
                depth,
                limit,
                format,
                redact,
            } => {
                let mut params = scope_params(hwnd, title);
                params.insert("depth".into(), Value::Int(depth));
                params.insert("limit".into(), Value::Int(limit));
                let fmt = format.tree_format();
                let v = run_probe(data_dir, &config, "ui.element.list", async {
                    ui_probe::probe_element_tree(&ctx, params, fmt).await
                })
                .await?;
                print_value(&v, redact)?;
            }
            ElementCmd::Get {
                hwnd,
                title,
                name,
                name_contains,
                automation_id,
                control_type,
                class,
                timeout_ms,
                redact,
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
                if let Some(c) = class {
                    params.insert("class".into(), Value::Str(c));
                }
                params.insert("timeout_ms".into(), Value::Int(timeout_ms));
                let v = run_probe(data_dir, &config, "ui.element.find", async {
                    ui_probe::probe_element_get(&ctx, params).await
                })
                .await?;
                print_value(&v, redact)?;
            }
            ElementCmd::Point { x, y, redact } => {
                let v = run_probe(data_dir, &config, "ui.element.point", async {
                    ui_probe::probe_element_point(x, y).await
                })
                .await?;
                print_value(&v, redact)?;
            }
            ElementCmd::Pick {
                scope_hwnd,
                copy_yaml,
                redact,
            } => {
                #[cfg(windows)]
                {
                    let v = run_probe(data_dir, &config, "ui.element.pick", async {
                        corex_registry::ui_pick::probe_pick(scope_hwnd).await
                    })
                    .await?;
                    if copy_yaml
                        && let Value::Map(m) = &v
                        && let Some(yaml) = m.get("selectors_yaml").and_then(|v| v.as_str())
                    {
                        copy_yaml_to_clipboard(yaml)?;
                    }
                    print_value(&v, redact)?;
                }
                #[cfg(not(windows))]
                {
                    let _ = (scope_hwnd, copy_yaml, redact);
                    bail!("ui element pick 需要 Windows");
                }
            }
        },
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_masks_names_and_automation_ids() {
        let mut m = BTreeMap::new();
        m.insert("name".into(), Value::Str("secret".into()));
        m.insert("automation_id".into(), Value::Str("btnSecret".into()));
        m.insert(
            "children".into(),
            Value::Array(vec![Value::Map(BTreeMap::from([(
                "name".into(),
                Value::Str("child".into()),
            )]))]),
        );
        let mut v = Value::Map(m);
        redact_value(&mut v);
        if let Value::Map(m) = v {
            assert_eq!(m.get("name").and_then(|v| v.as_str()), Some("***"));
            assert_eq!(m.get("automation_id").and_then(|v| v.as_str()), Some("***"));
        }
    }
}
