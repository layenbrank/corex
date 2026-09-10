//! 进程退出码。
//!
//! `anyhow` 会把所有失败都压成退出码 1，脚本分不出“指令跑失败”“命令写错了”“策略拒绝了”。
//! 引擎已经为每个错误发布了稳定的 `kind()` 字符串，因此用该字符串作为下表的键。

use corex_core::{ActionError, EngineError};

/// 本进程向调用方报告的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitStatus {
    /// 命令按要求完成。
    Success = 0,
    /// 命令执行了但失败。
    Failure = 1,
    /// 调用方式或其输入不对：参数写错、指令或动作未知、YAML 无法解析、参数非法、配置损坏。
    Usage = 2,
    /// 权限或策略门禁拒绝了该动作。
    Denied = 3,
}

impl ExitStatus {
    pub fn code(self) -> u8 {
        self as u8
    }

    /// 从 `err` 整条 `anyhow` 上下文中读出状态码。
    ///
    /// 第一个带类型的 action / engine 错误说了算：外层 `anyhow` 上下文只是加了些说明文字。
    pub fn read(err: &anyhow::Error) -> Self {
        for cause in err.chain() {
            if let Some(action) = cause.downcast_ref::<ActionError>() {
                return Self::action(&action.kind());
            }
            if let Some(engine) = cause.downcast_ref::<EngineError>() {
                // `EngineError::kind()` 对 `StepFailed` 和 `Action` 会委派给内层动作，但那个
                // 字符串无法定桶：`not_found` 在动作上意味着“文件或窗口不存在”，在引擎上
                // 意味着“指令不存在”。所以直接问内层错误，而不走 `kind()`。
                return match engine {
                    EngineError::StepFailed { source, .. } => Self::action(&source.kind()),
                    EngineError::Action(action) => Self::action(&action.kind()),
                    other => Self::engine(&other.kind()),
                };
            }
        }
        // CLI 自己的 `bail!` 不带类型化错误；就当运行失败，而不是去猜它属于另外三桶中的哪一桶。
        Self::Failure
    }

    /// 动作表。这里的 `not_found` 是*运行期*缺失——动作要找的文件或窗口没了；
    /// 引擎的 `not_found` 意思相反（见下）。
    fn action(kind: &str) -> Self {
        match kind {
            "permission_denied" | "disabled" => Self::Denied,
            "invalid_params" => Self::Usage,
            // `timeout`、`not_found`、`io`、`execution` 以及动态的 `ui_*` 码，都表示
            // 动作跑了但没成功。
            _ => Self::Failure,
        }
    }

    /// 引擎表。这里的 `not_found` 是用户点名的指令：属于用法错误。
    fn engine(kind: &str) -> Self {
        match kind {
            "not_registered" | "not_found" | "parse" | "config" | "usage" => Self::Usage,
            // `resolve` / `control_flow` 可能依赖运行期数据，所以算运行失败而非编写错误。
            _ => Self::Failure,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(err: anyhow::Error) -> u8 {
        ExitStatus::read(&err).code()
    }

    #[test]
    fn a_pipeline_denial_is_denied_not_failed() {
        // 流水线会包装步骤失败，所以即使含义属于动作，这里拿到的 kind 也是*引擎*的 kind。
        // 在去查内层错误之前，这种情况会退成退出码 1。
        let engine = EngineError::StepFailed {
            step: "copy".into(),
            source: ActionError::PermissionDenied("x".into()),
        };
        assert_eq!(code(engine.into()), ExitStatus::Denied.code());
    }

    #[test]
    fn not_found_keeps_its_two_meanings_apart() {
        // 在步骤内部，它是运行期缺失（文件 / 窗口没了）。
        let engine = EngineError::StepFailed {
            step: "read".into(),
            source: ActionError::NotFound("build.log".into()),
        };
        assert_eq!(code(engine.into()), ExitStatus::Failure.code());
        // 单独出现时，它就是用户点名的指令：用法错误。
        let missing: anyhow::Error = EngineError::DirectiveNotFound("nope".into()).into();
        assert_eq!(code(missing), ExitStatus::Usage.code());
    }

    #[test]
    fn cli_errors_use_the_same_two_tables() {
        // 门禁拒绝就是权限错误，不管它从哪个代码路径出来。
        assert_eq!(
            code(anyhow::Error::new(ActionError::PermissionDenied(
                "strict 拒绝".into()
            ))),
            ExitStatus::Denied.code()
        );
        // 调用上的失误有自己的引擎 kind，因此归入用法桶，而不是通用失败桶。
        assert_eq!(
            code(anyhow::Error::new(EngineError::Usage(
                "--strict 用错了".into()
            ))),
            ExitStatus::Usage.code()
        );
    }
}
