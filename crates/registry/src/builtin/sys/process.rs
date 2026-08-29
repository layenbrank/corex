//! `process.list` / `process.kill`.

use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use std::collections::BTreeMap;
use std::sync::Arc;

pub struct ProcessList;
pub struct ProcessKill;

#[async_trait]
impl Action for ProcessList {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "process.list",
            "List Processes",
            "枚举进程（可选 name_contains）",
            ActionCategory::System,
        )
        .with_params(vec![ParamSchema::new(
            "name_contains",
            SchemaType::Str,
            false,
        )])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let filter = params
            .as_map()
            .and_then(|m| m.get("name_contains"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_ascii_lowercase());
        #[cfg(windows)]
        {
            return tokio::task::spawn_blocking(move || win::list_processes(filter.as_deref()))
                .await
                .map_err(|e| ActionError::execution(format!("process.list 失败: {e}")))?;
        }
        #[cfg(not(windows))]
        {
            let _ = filter;
            Err(ActionError::execution("process.* 需要 Windows"))
        }
    }
}

#[async_trait]
impl Action for ProcessKill {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "process.kill",
            "Kill Process",
            "按 pid 结束进程",
            ActionCategory::System,
        )
        .with_params(vec![ParamSchema::new("pid", SchemaType::Int, true)])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let pid = params
            .as_map()
            .and_then(|m| m.get("pid"))
            .and_then(|v| v.as_i64())
            .ok_or_else(|| ActionError::MissingParam("pid".into()))? as u32;
        #[cfg(windows)]
        {
            return tokio::task::spawn_blocking(move || win::kill_process(pid))
                .await
                .map_err(|e| ActionError::execution(format!("process.kill 失败: {e}")))?
                .map(|_| Value::Bool(true));
        }
        #[cfg(not(windows))]
        {
            let _ = pid;
            Err(ActionError::execution("process.* 需要 Windows"))
        }
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(ProcessList));
    registry.register(Arc::new(ProcessKill));
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::mem::size_of_val;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, TerminateProcess, PROCESS_TERMINATE,
    };

    pub fn list_processes(name_contains: Option<&str>) -> Result<Value, ActionError> {
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
                .map_err(|e| ActionError::execution(format!("CreateToolhelp32Snapshot: {e}")))?;
            let mut entry = PROCESSENTRY32W {
                dwSize: size_of_val(&PROCESSENTRY32W::default()) as u32,
                ..Default::default()
            };
            let mut list = Vec::new();
            if Process32FirstW(snap, &mut entry).is_ok() {
                loop {
                    let name = String::from_utf16_lossy(
                        &entry.szExeFile
                            [..entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(0)],
                    );
                    let pass = name_contains
                        .map(|f| name.to_ascii_lowercase().contains(f))
                        .unwrap_or(true);
                    if pass {
                        let mut m = BTreeMap::new();
                        m.insert("pid".into(), Value::Int(entry.th32ProcessID as i64));
                        m.insert("name".into(), Value::Str(name));
                        list.push(Value::Map(m));
                    }
                    if Process32NextW(snap, &mut entry).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snap);
            let mut out = BTreeMap::new();
            out.insert("processes".into(), Value::List(list));
            Ok(Value::Map(out))
        }
    }

    pub fn kill_process(pid: u32) -> Result<(), ActionError> {
        unsafe {
            let handle: HANDLE = OpenProcess(PROCESS_TERMINATE, false, pid)
                .map_err(|e| ActionError::execution(format!("OpenProcess: {e}")))?;
            let r = TerminateProcess(handle, 1);
            let _ = CloseHandle(handle);
            r.map_err(|e| ActionError::execution(format!("TerminateProcess: {e}")))?;
            Ok(())
        }
    }
}
