//! 系统预配置指令：写入指令库且对用户列表隐藏（`visible = false`）。
//!
//! 与 [`crate::starter`] 不同：起步指令只在空库写一次；系统指令每次启动 upsert，
//! 保证产品（如截屏磁贴）依赖的指令始终可按名执行。

/// 系统预配置：`(指令名, YAML)`。
const SYSTEM: &[(&str, &str)] = &[(
    "capture-screenshot",
    include_str!("../assets/system/capture-screenshot.yaml"),
)];

/// 系统预配置指令名。
pub fn names() -> Vec<&'static str> {
    SYSTEM.iter().map(|(name, _)| *name).collect()
}

/// 系统预配置的 `(名字, YAML)` 表。
pub fn directives() -> &'static [(&'static str, &'static str)] {
    SYSTEM
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Directive;

    #[test]
    fn system_yaml_parses() {
        for (name, yaml) in SYSTEM {
            let definition = Directive::from_yaml_str(yaml).unwrap_or_else(|error| {
                panic!("系统指令 {name} 解析失败: {error}")
            });
            assert_eq!(definition.name, *name);
        }
    }
}
