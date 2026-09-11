//! 指令步骤树的摊平与两道门。
//!
//! `if` / `repeat` / `parallel` 三种容器的展开方式**只写在这里一份**。`--dry-run` 的
//! 提纲、未注册动作与运行时权限门检查都要走同一棵树，各写一份遍历迟早会走偏——
//! 多加一种容器时只改这里。
//!
//! 「什么算未注册」「什么算被权限门拒」也只在这里定义：`corex run --dry-run` 与
//! `corex validate` 因此给出同一个判定，也只有一个退出码。

use corex_core::{ActionError, EngineError};
use corex_engine::definition::ActionStep;
use corex_engine::{Permissions, Step};
use corex_registry::ActionRegistry;

/// `--dry-run` 的提纲：带缩进的行，顺序与执行顺序一致。
pub(crate) fn outline(steps: &[Step]) -> Vec<String> {
    rows(steps).iter().map(Row::render).collect()
}

/// 步骤树里若有本进程注册表没有的动作就报错。
///
/// 用 [`EngineError::ActionNotRegistered`] 而不是裸 `bail!`：它对应退出码 2
/// （调用方给的东西不对），而通用失败是 1。
pub(crate) fn require_registered(
    steps: &[Step],
    registry: &ActionRegistry,
) -> Result<(), EngineError> {
    let missing: Vec<String> = rows(steps)
        .iter()
        .filter_map(Row::action)
        .filter(|(_, action)| !registry.contains(action))
        .map(|(id, action)| format!("{action} ({id})"))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(EngineError::ActionNotRegistered(missing.join(", ")))
}

/// 步骤树里若有运行时会因权限声明不足而被拒的动作就报错（退出码 3）。
///
/// 查的是**运行时那道门**（[`Permissions::allows_action`]），不是 `validate --strict`
/// 的企业门禁：后者额外要求“必须声明 permissions”，而真实运行并不要求。
pub(crate) fn require_allowed(
    steps: &[Step],
    permissions: &Permissions,
    registry: &ActionRegistry,
) -> Result<(), ActionError> {
    let denied: Vec<String> = rows(steps)
        .iter()
        .filter_map(Row::action)
        .filter_map(|(id, action)| {
            permissions
                .allows_action(registry, action)
                .err()
                .map(|err| format!("{id}: {err}"))
        })
        .collect();
    if denied.is_empty() {
        return Ok(());
    }
    Err(ActionError::PermissionDenied(denied.join("；")))
}

/// 步骤树摊平后的一行。
enum Row<'a> {
    /// 一个动作步骤。
    Action { step: &'a ActionStep, depth: usize },
    /// 容器节点的开头（`if` / `repeat` / `parallel`），或 `if` 的 `else` 分支。
    Marker {
        label: &'static str,
        /// 容器自己的 id；`else` 不是节点，没有 id。
        id: Option<&'a str>,
        depth: usize,
    },
}

impl Row<'_> {
    /// 动作步骤的 `(id, action)`；容器行返回 `None`。
    fn action(&self) -> Option<(&str, &str)> {
        match self {
            Self::Action { step, .. } => Some((step.id.as_str(), step.action.as_str())),
            Self::Marker { .. } => None,
        }
    }

    fn render(&self) -> String {
        let pad = "  ".repeat(match self {
            Self::Action { depth, .. } | Self::Marker { depth, .. } => *depth,
        });
        match self {
            Self::Action { step, .. } => format!("{pad}{}  {}", step.id, step.action),
            Self::Marker {
                label,
                id: Some(id),
                ..
            } => format!("{pad}{id}  {label}"),
            Self::Marker {
                label, id: None, ..
            } => format!("{pad}{label}"),
        }
    }
}

/// 按执行顺序把步骤树摊成行。
fn rows(steps: &[Step]) -> Vec<Row<'_>> {
    fn walk<'a>(steps: &'a [Step], depth: usize, out: &mut Vec<Row<'a>>) {
        for step in steps {
            match step {
                Step::Action(step) => out.push(Row::Action { step, depth }),
                Step::If(branch) => {
                    out.push(Row::Marker {
                        label: "if",
                        id: Some(&branch.id),
                        depth,
                    });
                    walk(&branch.then, depth + 1, out);
                    if !branch.else_steps.is_empty() {
                        // `else` 与它的子步骤同层，与 `then` 的子树平齐。
                        out.push(Row::Marker {
                            label: "else",
                            id: None,
                            depth: depth + 1,
                        });
                        walk(&branch.else_steps, depth + 1, out);
                    }
                }
                Step::Repeat(repeat) => {
                    out.push(Row::Marker {
                        label: "repeat",
                        id: Some(&repeat.id),
                        depth,
                    });
                    walk(&repeat.steps, depth + 1, out);
                }
                Step::Parallel(parallel) => {
                    out.push(Row::Marker {
                        label: "parallel",
                        id: Some(&parallel.id),
                        depth,
                    });
                    walk(&parallel.parallel, depth + 1, out);
                }
            }
        }
    }

    let mut out = Vec::new();
    walk(steps, 0, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directive(steps_yaml: &str) -> corex_engine::Directive {
        corex_engine::Directive::from_yaml_str(&format!("name: probe\nsteps:\n{steps_yaml}"))
            .expect("fixture parses")
    }

    #[test]
    fn outline_keeps_the_nesting() {
        let directive = directive(concat!(
            "  - id: pick\n",
            "    if:\n",
            "      eq: [\"{{input.mode}}\", prod]\n",
            "    then:\n",
            "      - id: a\n",
            "        action: template.render\n",
            "    else:\n",
            "      - id: b\n",
            "        action: template.render\n",
            "  - id: last\n",
            "    action: template.render\n",
        ));
        let rendered = outline(&directive.steps).join("\n");
        assert!(rendered.contains("pick  if"), "{rendered}");
        assert!(rendered.contains("  a  template.render"), "{rendered}");
        assert!(rendered.contains("  else"), "{rendered}");
        assert!(rendered.contains("  b  template.render"), "{rendered}");
        assert!(rendered.contains("\nlast  template.render"), "{rendered}");
    }

    /// 两道门都要下到容器里去：`repeat` 里的步骤不该因为嵌了一层就被漏掉。
    #[test]
    fn both_doors_reach_nested_steps() {
        let directive = directive(concat!(
            "  - id: loop\n",
            "    repeat:\n",
            "      count: 2\n",
            "    steps:\n",
            "      - id: inner\n",
            "        action: does.not.exist\n",
        ));
        let registry = ActionRegistry::new();
        let missing = require_registered(&directive.steps, &registry).expect_err("未注册");
        assert!(missing.to_string().contains("inner"), "{missing}");

        let allowed = require_allowed(&directive.steps, &Permissions::default(), &registry);
        assert!(allowed.is_ok(), "没声明 permissions 就是不受限");
    }
}
