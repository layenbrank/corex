use serde_json::Value;

use super::artifact::Artifact;

/// 统一模块调用结果
#[derive(Debug, Clone, Default)]
pub struct InvokeResult {
    pub artifact: Option<Artifact>,
    pub data: Option<Value>,
}

impl InvokeResult {
    /// 从 Artifact 构造结果，并将 artifact.data 同步到顶层 data。
    pub fn from_artifact(artifact: Artifact) -> Self {
        let data = if artifact.data.is_empty() {
            None
        } else {
            Some(Value::Object(
                artifact
                    .data
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            ))
        };
        Self {
            artifact: Some(artifact),
            data,
        }
    }

    /// 附加 IPC 侧车数据（Monitors、scan 等）。
    pub fn with_ipc_data(mut self, data: Option<Value>) -> Self {
        let Some(data) = data else {
            return self;
        };
        if let Some(ref mut art) = self.artifact {
            art.data.insert("data".to_string(), data.clone());
        }
        self.data = Some(data);
        self
    }

    pub fn path_string(&self) -> Option<String> {
        self.artifact
            .as_ref()
            .and_then(|a| a.path.as_ref())
            .map(|p| p.to_string_lossy().into_owned())
    }
}
