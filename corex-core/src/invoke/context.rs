use crate::pipeline::context::PipelineContext;

/// 模块调用上下文（变量解析 + 可选 Daemon 状态）
pub struct InvokeContext<'a> {
    pub pipeline: Option<&'a PipelineContext>,
    #[cfg(feature = "serve")]
    pub daemon: Option<&'a mut crate::serve::state::DaemonState>,
}

impl<'a> InvokeContext<'a> {
    pub fn empty() -> InvokeContext<'static> {
        InvokeContext {
            pipeline: None,
            #[cfg(feature = "serve")]
            daemon: None,
        }
    }

    pub fn pipeline(ctx: &'a PipelineContext) -> Self {
        Self {
            pipeline: Some(ctx),
            #[cfg(feature = "serve")]
            daemon: None,
        }
    }

    pub fn parse(&self, input: &str) -> String {
        match self.pipeline {
            Some(ctx) => ctx.parse(input),
            None => input.to_string(),
        }
    }

    /// Daemon 预热的显示器列表（IPC 截图加速）；无 daemon 时返回 None。
    #[cfg(feature = "screenshot")]
    pub fn cached_monitors(&self) -> Option<&[xcap::Monitor]> {
        #[cfg(feature = "serve")]
        {
            return self.daemon.as_ref().and_then(|d| d.monitors.as_deref());
        }
        None
    }
}

#[cfg(feature = "serve")]
impl<'a> InvokeContext<'a> {
    pub fn daemon(state: &'a mut crate::serve::state::DaemonState) -> Self {
        Self {
            pipeline: None,
            daemon: Some(state),
        }
    }

    pub fn pipeline_and_daemon(
        ctx: &'a PipelineContext,
        state: &'a mut crate::serve::state::DaemonState,
    ) -> Self {
        Self {
            pipeline: Some(ctx),
            daemon: Some(state),
        }
    }
}
