//! 指令步骤树的提纲。
//!
//! `if` / `repeat` / `parallel` 三种容器的展开方式只写在这里一份：`corex run --dry-run`
//! 要靠它把「将要执行的步骤」按执行顺序印出来。**判定**（动作是否注册、权限够不够）
//! 不在这里，它们由 [`corex_engine::validate_registered`] / [`corex_engine::validate_allowed`]
//! 与引擎共用同一条遍历。

use corex_engine::{ActionStep, Step};
use corex_registry::ActionRegistry;

/// `--dry-run` 的提纲：带缩进的行，顺序与执行顺序一致。
///
/// 动作行尾部附上它要求的权限类别：预览的一半价值就在「它会碰什么」。
pub(crate) fn outline(steps: &[Step], registry: &ActionRegistry) -> Vec<String> {
    rows(steps).iter().map(|row| row.render(registry)).collect()
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
    fn render(&self, registry: &ActionRegistry) -> String {
        let pad = "  ".repeat(match self {
            Self::Action { depth, .. } | Self::Marker { depth, .. } => *depth,
        });
        match self {
            Self::Action { step, .. } => format!(
                "{pad}{}  {}{}",
                step.id,
                step.action,
                requires(registry, &step.action)
            ),
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

/// 动作要求的权限类别，形如 `  [filesystem、secret]`；不需要权限时是空串。
///
/// 动作没注册时不添任何东西：那是 [`corex_engine::validate_registered`] 的活儿，预览只管展示。
fn requires(registry: &ActionRegistry, action: &str) -> String {
    let Some(action) = registry.get(action) else {
        return String::new();
    };
    let names: Vec<&str> = action
        .permissions()
        .iter()
        .map(|kind| kind.name())
        .collect();
    if names.is_empty() {
        String::new()
    } else {
        format!("  [{}]", names.join("、"))
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
                Step::Steps(steps) => walk(&steps.steps, depth, out),
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
        let rendered = outline(&directive.steps, &ActionRegistry::new()).join("\n");
        assert!(rendered.contains("pick  if"), "{rendered}");
        assert!(rendered.contains("  a  template.render"), "{rendered}");
        assert!(rendered.contains("  else"), "{rendered}");
        assert!(rendered.contains("  b  template.render"), "{rendered}");
        assert!(rendered.contains("\nlast  template.render"), "{rendered}");
    }

    /// 提纲里的动作行要带上它要求的权限类别——预览的一半价值就在「它会碰什么」。
    #[test]
    fn outline_shows_required_permissions() {
        let directive = directive(concat!(
            "  - id: write\n",
            "    action: file.write\n",
            "  - id: wait\n",
            "    action: template.render\n",
        ));
        let mut registry = ActionRegistry::new();
        registry.register_builtins();
        let rendered = outline(&directive.steps, &registry).join("\n");
        assert!(
            rendered.contains("write  file.write  [filesystem]"),
            "{rendered}"
        );
        // 不需要任何权限的动作不添方括号。
        assert!(rendered.ends_with("wait  template.render"), "{rendered}");
    }

    /// 两道门都要下到容器里去：`repeat` 里的步骤不该因为嵌了一层就被漏掉。
    /// （判定本身在 `corex_engine`，这里只确认 CLI 拿到的提纲也下到了那一层。）
    #[test]
    fn outline_reaches_nested_steps() {
        let directive = directive(concat!(
            "  - id: loop\n",
            "    repeat:\n",
            "      count: 2\n",
            "    steps:\n",
            "      - id: inner\n",
            "        action: does.not.exist\n",
        ));
        let rendered = outline(&directive.steps, &ActionRegistry::new()).join("\n");
        assert!(rendered.contains("  inner  does.not.exist"), "{rendered}");
    }
}
