//! invoke 占位符解析与 IPC 数据回传测试

use std::collections::HashMap;

use cx::invoke::{InvokeContext, invoke, ipc_data};
use cx::pipeline::context::PipelineContext;
use serde_json::json;

#[test]
fn invoke_codec_parse_output_placeholder() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("hash.txt");
    let mut vars = HashMap::new();
    vars.insert("out".to_string(), out.display().to_string());

    let pipeline_ctx = PipelineContext::with_variables(vars);
    let ctx = InvokeContext::pipeline(&pipeline_ctx);
    invoke(
        "codec",
        json!({
            "Hash": {
                "scheme": {
                    "Md5": {
                        "input": "hello",
                        "output": "${var.out}"
                    }
                }
            }
        }),
        &ctx,
    )
    .expect("invoke codec");

    assert!(out.is_file());
    let content = std::fs::read_to_string(&out).unwrap();
    assert_eq!(content.trim(), "5d41402abc4b2a76b9719d911017c592");
}

#[test]
fn invoke_compression_parse_path_placeholders() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("a.txt"), b"data").unwrap();
    let archive = dir.path().join("out.zip");

    let mut vars = HashMap::new();
    vars.insert("src".to_string(), src.display().to_string());
    vars.insert("dst".to_string(), archive.display().to_string());

    let pipeline_ctx = PipelineContext::with_variables(vars);
    let ctx = InvokeContext::pipeline(&pipeline_ctx);
    invoke(
        "compression",
        json!({
            "Compress": {
                "scheme": {
                    "Zip": {
                        "from": "${var.src}",
                        "to": "${var.dst}"
                    }
                }
            }
        }),
        &ctx,
    )
    .expect("invoke compression");

    assert!(archive.is_file());
}

#[cfg(feature = "screenshot")]
#[test]
fn invoke_screenshot_monitors_returns_data() {
    let ctx = InvokeContext::empty();
    let result = invoke("screenshot", json!({"Monitors": null}), &ctx).expect("monitors");

    let data = ipc_data(&result).or(result.data).expect("data field");
    assert!(data.is_array());
}

#[cfg(feature = "screenshot")]
#[test]
fn invoke_screenshot_windows_returns_data() {
    let ctx = InvokeContext::empty();
    let result = invoke("screenshot", json!({"Windows": null}), &ctx).expect("windows");

    let data = ipc_data(&result).or(result.data).expect("data field");
    assert!(data.is_array());
}
