//! `scan.os` — system information via sysinfo.

use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, Value,
};
use std::collections::BTreeMap;
use std::sync::Arc;
use sysinfo::System;

pub struct ScanOs;

#[async_trait]
impl Action for ScanOs {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "scan.os",
            "Scan OS",
            "采集操作系统与硬件摘要信息",
            ActionCategory::System,
        )
    }

    async fn execute(
        &self, _params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let mut sys = System::new_all();
        sys.refresh_all();
        let cpu = sys.cpus().first();

        let mut cpu_map = BTreeMap::new();
        cpu_map.insert(
            "brand".into(),
            Value::Str(
                cpu.map(|c| c.brand().to_string())
                    .unwrap_or_else(|| "Unknown".into()),
            ),
        );
        cpu_map.insert(
            "frequency".into(),
            Value::Int(cpu.map(|c| c.frequency() as i64).unwrap_or(0)),
        );
        cpu_map.insert("cores".into(), Value::Int(sys.cpus().len() as i64));
        cpu_map.insert("arch".into(), Value::Str(System::cpu_arch()));

        let mut mem = BTreeMap::new();
        mem.insert("total".into(), Value::Int(sys.total_memory() as i64));
        mem.insert("used".into(), Value::Int(sys.used_memory() as i64));

        let mut swap = BTreeMap::new();
        swap.insert("total".into(), Value::Int(sys.total_swap() as i64));
        swap.insert("used".into(), Value::Int(sys.used_swap() as i64));

        let mut out = BTreeMap::new();
        out.insert(
            "OS".into(),
            Value::Str(System::name().unwrap_or_else(|| "Unknown".into())),
        );
        out.insert(
            "version".into(),
            Value::Str(System::os_version().unwrap_or_else(|| "Unknown".into())),
        );
        out.insert(
            "kernel".into(),
            Value::Str(System::kernel_version().unwrap_or_else(|| "Unknown".into())),
        );
        out.insert(
            "hostname".into(),
            Value::Str(System::host_name().unwrap_or_else(|| "Unknown".into())),
        );
        out.insert("CPU".into(), Value::Map(cpu_map));
        out.insert("memory".into(), Value::Map(mem));
        out.insert("swap".into(), Value::Map(swap));
        Ok(Value::Map(out))
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(ScanOs));
}

#[cfg(test)]
mod tests {
    use super::*;
    use corex_core::ExecutionContext;

    #[tokio::test]
    async fn scan_os_returns_map() {
        let mut ctx = ExecutionContext::default();
        let out = ScanOs
            .execute(Value::Map(BTreeMap::new()), &mut ctx)
            .await
            .unwrap();
        let m = out.as_map().unwrap();
        assert!(m.contains_key("OS"));
        assert!(m.contains_key("CPU"));
        assert!(m.contains_key("memory"));
    }
}
