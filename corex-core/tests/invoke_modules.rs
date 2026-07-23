//! invoke 层单元测试

use cx::invoke::{InvokeContext, WireArgs, invoke};
use cx::pipeline::context::PipelineContext;
use serde_json::json;

#[test]
fn invoke_unknown_module_fails() {
    let pipeline_ctx = PipelineContext::new();
    let ctx = InvokeContext::pipeline(&pipeline_ctx);
    let err = invoke("not-a-module", WireArgs::flags(json!({})), &ctx).unwrap_err();
    assert!(err.to_string().contains("未知或未启用的模块"));
}

#[test]
fn invoke_copy_returns_artifact_path() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src.txt");
    std::fs::write(&src, "hello").unwrap();
    let dst = dir.path().join("dst.txt");

    let ctx = InvokeContext::empty();
    let result = invoke(
        "copy",
        WireArgs::flags(json!({
            "from": src.display().to_string(),
            "to": dst.display().to_string(),
            "empty": false,
            "includes": [],
            "excludes": []
        })),
        &ctx,
    )
    .expect("invoke copy");

    assert!(dst.exists());
    assert_eq!(result.path_string().as_deref(), Some(dst.to_str().unwrap()));
}

#[test]
fn invoke_exec_ps1_json_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let artifact_path = dir.path().join("out.json");
    let artifact_display = artifact_path.display().to_string();

    let script_path = dir.path().join("emit.ps1");
    let ps1 = format!(
        r#"$result = @{{
    path = '{artifact_display}'
    data = @{{ version = '20260713' }}
}}
Write-Output ($result | ConvertTo-Json -Compress)"#
    );
    std::fs::write(&script_path, ps1).unwrap();

    let ctx = InvokeContext::empty();
    let result = invoke(
        "exec",
        WireArgs::action(
            "run",
            json!({
                "script": script_path.display().to_string(),
                "args": [],
                "capture": "json"
            }),
        ),
        &ctx,
    )
    .expect("invoke exec");

    assert_eq!(
        result.path_string().as_deref(),
        Some(artifact_path.to_str().unwrap())
    );
    let version = result
        .data
        .as_ref()
        .and_then(|d| d.get("version"))
        .and_then(|v| v.as_str());
    assert_eq!(version, Some("20260713"));
}

#[test]
fn invoke_copy_file_into_dir_returns_actual_path() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src.txt");
    std::fs::write(&src, "hello").unwrap();
    let out_dir = dir.path().join("out");
    std::fs::create_dir(&out_dir).unwrap();

    let ctx = InvokeContext::empty();
    let result = invoke(
        "copy",
        WireArgs::flags(json!({
            "from": src.display().to_string(),
            "to": out_dir.display().to_string(),
            "empty": false,
            "includes": [],
            "excludes": []
        })),
        &ctx,
    )
    .expect("invoke copy into dir");

    let expected = out_dir.join("src.txt");
    assert!(expected.exists());
    assert_eq!(
        result.path_string().as_deref(),
        Some(expected.to_str().unwrap())
    );
}

#[test]
fn app_error_maps_config_messages() {
    use cx::runtime::AppError;
    use cx::runtime::app_error_from_anyhow;

    let err = anyhow::anyhow!("配置 version 必须为 3，当前为 2");
    match app_error_from_anyhow(err) {
        AppError::Config(_) => {}
        other => panic!("expected Config, got {other:?}"),
    }
}
