//! exec 模块集成测试

use cx::invoke::{InvokeContext, WireArgs, invoke};
use serde_json::json;
use std::fs;
use std::path::PathBuf;

fn write_ps1(dir: &PathBuf, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, body).unwrap();
    path
}

#[test]
fn invoke_exec_ps1_returns_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let out_file = dir.path().join("result.json");
    let out_str = out_file.display().to_string().replace('\\', "/");

    let script = write_ps1(
        &dir.path().to_path_buf(),
        "ok.ps1",
        &format!(
            r#"
$out = "{out_str}"
@{{ version = "20260713" }} | ConvertTo-Json -Compress | Set-Content $out
$result = @{{
    path = $out
    data = @{{ version = "20260713" }}
}}
Write-Output ($result | ConvertTo-Json -Compress)
"#
        ),
    );

    let ctx = InvokeContext::empty();
    let result = invoke(
        "exec",
        WireArgs::action(
            "run",
            json!({
                "script": script.display().to_string(),
                "args": [],
                "capture": "json"
            }),
        ),
        &ctx,
    )
    .expect("invoke exec");

    assert!(out_file.exists());
    let expected = out_file.canonicalize().unwrap();
    let actual = PathBuf::from(result.path_string().unwrap())
        .canonicalize()
        .unwrap();
    assert_eq!(actual, expected);
    let version = result
        .artifact
        .as_ref()
        .and_then(|a| a.data.get("version"))
        .and_then(|v| v.as_str());
    assert_eq!(version, Some("20260713"));
}

#[test]
fn invoke_exec_fails_on_nonzero_exit() {
    let dir = tempfile::tempdir().unwrap();
    let script = write_ps1(
        &dir.path().to_path_buf(),
        "fail.ps1",
        "Write-Error 'boom'; exit 1",
    );

    let ctx = InvokeContext::empty();
    let err = invoke(
        "exec",
        WireArgs::action(
            "run",
            json!({
                "script": script.display().to_string(),
                "args": [],
                "capture": "json"
            }),
        ),
        &ctx,
    )
    .unwrap_err();

    assert!(err.to_string().contains("退出码"));
}

#[test]
fn invoke_exec_bat_returns_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("from_bat.json");
    let marker_str = marker.display().to_string().replace('\\', "/");

    let bat = dir.path().join("ok.bat");
    let bat_body = format!(
        "@echo off\r\n\
         echo {{\"path\":\"{marker_str}\",\"data\":{{\"source\":\"bat\"}}}}\r\n"
    );
    fs::write(&bat, bat_body).unwrap();

    let ctx = cx::invoke::InvokeContext::empty();
    let result = invoke(
        "exec",
        WireArgs::action(
            "run",
            json!({
                "script": bat.display().to_string(),
                "args": [],
                "capture": "json"
            }),
        ),
        &ctx,
    )
    .expect("invoke exec bat");

    assert_eq!(
        result
            .artifact
            .as_ref()
            .and_then(|a| a.data.get("source"))
            .and_then(|v| v.as_str()),
        Some("bat")
    );
}
