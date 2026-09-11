//! Action trait 与其元数据。

use crate::context::ExecutionContext;
use crate::error::ActionError;
use crate::permission::PermissionSet;
use crate::schema::SchemaType;
use crate::value::Value;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

/// 供发现 / UI 使用的高层分组：一个动作只属于一个 bucket。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Bucket {
    System,
    Network,
    Data,
    Ui,
    Logic,
    Plugin,
}

impl Bucket {
    /// 全部 bucket，顺序即 `corex actions` 列表里的分组顺序。
    pub const ALL: [Self; 6] = [
        Self::System,
        Self::Network,
        Self::Data,
        Self::Ui,
        Self::Logic,
        Self::Plugin,
    ];

    /// 展示与 `--bucket` 用的小写标识。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Network => "network",
            Self::Data => "data",
            Self::Ui => "ui",
            Self::Logic => "logic",
            Self::Plugin => "plugin",
        }
    }

    /// 按 [`Self::as_str`] 的写法解析，大小写不敏感。
    ///
    /// 不认识的写法返回 `None`，由调用方决定怎么给候选——解析本身不该猜。
    pub fn parse(name: &str) -> Option<Self> {
        let key = name.trim().to_ascii_lowercase();
        Self::ALL.into_iter().find(|bucket| bucket.as_str() == key)
    }
}

impl std::fmt::Display for Bucket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 声明动作的一个参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamSchema {
    pub name: String,
    pub ty: SchemaType,
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
}

impl ParamSchema {
    pub fn new(name: impl Into<String>, ty: SchemaType, required: bool) -> Self {
        Self {
            name: name.into(),
            ty,
            required,
            description: None,
            default: None,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn with_default(mut self, default: impl Into<Value>) -> Self {
        self.default = Some(default.into());
        self
    }
}

/// 描述一个动作的静态元数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionMeta {
    pub id: String,
    pub name: String,
    pub description: String,
    pub bucket: Bucket,
    #[serde(default)]
    pub params: Vec<ParamSchema>,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl ActionMeta {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        bucket: Bucket,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            bucket,
            params: Vec::new(),
            tags: Vec::new(),
        }
    }

    pub fn with_params(mut self, params: Vec<ParamSchema>) -> Self {
        self.params = params;
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }
}

/// 注册进动作注册表的可执行单元。
#[async_trait]
pub trait Action: Send + Sync {
    fn meta(&self) -> ActionMeta;

    /// 该动作运行前需要的权限类别。
    ///
    /// 刻意**没有默认实现**：忘记声明权限要求的动作必须编译不过，
    /// 而不是悄悄不受限地跑起来。
    fn permissions(&self) -> PermissionSet;

    /// 执行前校验参数。默认实现：检查必填参数是否存在。
    async fn validate(&self, params: &Value) -> Result<(), ActionError> {
        let meta = self.meta();
        let map = match params {
            Value::Map(m) => m,
            Value::Null => &BTreeMap::new(),
            _ => {
                return Err(ActionError::InvalidParams("参数必须是对象（map）".into()));
            }
        };
        for p in &meta.params {
            if p.required && !map.contains_key(&p.name) {
                return Err(ActionError::MissingParam(p.name.clone()));
            }
        }
        Ok(())
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError>;
}

/// 查找门面，使引擎能在不依赖 registry crate 的前提下解析动作。
pub trait ActionStore: Send + Sync {
    fn find_action(&self, id: &str) -> Option<Arc<dyn Action>>;

    /// `action_id` 声明的权限要求。
    ///
    /// 未知 id 什么都不声明：它本来就无法执行，缺注册这件事由引擎自己的
    /// 查找来报，而不是由权限门禁报。
    fn permissions_of(&self, action_id: &str) -> PermissionSet {
        self.find_action(action_id)
            .map_or(PermissionSet::NONE, |action| action.permissions())
    }

    fn actions(&self) -> Vec<ActionMeta> {
        Vec::new()
    }
}

impl ActionStore for HashMapStore {
    fn find_action(&self, id: &str) -> Option<Arc<dyn Action>> {
        self.0.get(id).cloned()
    }

    fn actions(&self) -> Vec<ActionMeta> {
        self.0.values().map(|a| a.meta()).collect()
    }
}

/// 供测试与轻量运行器使用的内存 store。
#[derive(Default)]
pub struct HashMapStore(pub std::collections::HashMap<String, Arc<dyn Action>>);

impl HashMapStore {
    pub fn new() -> Self {
        Self(std::collections::HashMap::new())
    }

    pub fn register(&mut self, action: Arc<dyn Action>) {
        let id = action.meta().id;
        self.0.insert(id, action);
    }
}
