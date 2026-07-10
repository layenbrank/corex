use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sysinfo::System;

use crate::scan::schema::Args;

#[derive(Debug, Serialize, Deserialize)]
pub struct Memory {
    pub total: u64,
    pub used: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Swap {
    pub total: u64,
    pub used: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Cpu {
    pub brand: String,
    pub frequency: u64,
    pub cores: usize,
    pub arch: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OsContext {
    #[serde(rename = "OS")]
    pub os: String,
    pub version: String,
    pub kernel: String,
    pub hostname: String,
    #[serde(rename = "CPU")]
    pub cpu: Cpu,
    pub memory: Memory,
    pub swap: Swap,
}

pub struct Scan;

impl Scan {
    pub fn os() -> OsContext {
        let mut sys = System::new_all();
        sys.refresh_all();
        let cpu = sys.cpus().first();
        OsContext {
            os: System::name().unwrap_or_else(|| "Unknown".to_string()),
            version: System::os_version().unwrap_or_else(|| "Unknown".to_string()),
            kernel: System::kernel_version().unwrap_or_else(|| "Unknown".to_string()),
            hostname: System::host_name().unwrap_or_else(|| "Unknown".to_string()),
            cpu: Cpu {
                brand: cpu
                    .map(|c| c.brand().to_string())
                    .unwrap_or_else(|| "Unknown".to_string()),
                frequency: cpu.map(|c| c.frequency()).unwrap_or(0),
                cores: sys.cpus().len(),
                arch: System::cpu_arch(),
            },
            memory: Memory {
                total: sys.total_memory(),
                used: sys.used_memory(),
            },
            swap: Swap {
                total: sys.total_swap(),
                used: sys.used_swap(),
            },
        }
    }
}

/// `corex scan` 命令入口
pub fn run(args: &Args) -> Result<()> {
    let ctx = execute(args)?;
    let json = serde_json::to_string_pretty(&ctx)?;
    println!("{json}");
    Ok(())
}

pub fn execute(args: &Args) -> Result<OsContext> {
    match args {
        Args::Os(_) => Ok(Scan::os()),
    }
}

impl OsContext {
    pub fn into_ipc_value(self) -> Value {
        json!(self)
    }

    /// 转为 invoke 层统一结果。
    pub fn into_invoke_result(self) -> crate::invoke::InvokeResult {
        use crate::invoke::{Artifact, InvokeResult};
        InvokeResult::from_artifact(Artifact::default().with_data("data", self.into_ipc_value()))
    }
}
