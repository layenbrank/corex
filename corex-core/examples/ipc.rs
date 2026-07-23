//! 最小 IPC 客户端验证示例
//!
//! ```powershell
//! # 终端 1
//! cargo run -p corex-serve
//!
//! # 终端 2
//! cargo run -p corex-core --example ipc --features serve -- C:\Temp\screenshots
//! ```

use std::env;

use cx::invoke::WireArgs;

fn main() -> anyhow::Result<()> {
    let to = env::args()
        .nth(1)
        .unwrap_or_else(|| "C:\\Temp\\screenshots".to_string());

    let resp = cx::serve::request(
        r"\\.\pipe\corex",
        "screenshot",
        WireArgs::action("capture", serde_json::json!({ "to": to })),
    )?;

    if resp.ok {
        println!("ok: path={:?} ({}ms)", resp.path, resp.ms);
    } else {
        eprintln!("error: {} ({}ms)", resp.error.unwrap_or_default(), resp.ms);
        std::process::exit(1);
    }

    Ok(())
}
