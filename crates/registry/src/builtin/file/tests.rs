//! `file.*` 动作及其写入模式策略的测试。

use super::*;
use corex_core::ExecutionContext;
use tempfile::tempdir;

#[tokio::test]
async fn overwrite_and_splice() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.txt");
    let mut ctx = ExecutionContext::default();

    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(path.display().to_string()));
    params.insert(
        "content".into(),
        Value::Str("<!--START-->old<!--END-->".into()),
    );
    FileWrite
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap();

    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(path.display().to_string()));
    params.insert("mode".into(), Value::Str("splice".into()));
    params.insert("start".into(), Value::Str("<!--START-->".into()));
    params.insert("end".into(), Value::Str("<!--END-->".into()));
    params.insert("content".into(), Value::Str("new".into()));
    let out = FileWrite
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap();
    assert!(
        out.as_map()
            .unwrap()
            .get("changed")
            .unwrap()
            .as_bool()
            .unwrap()
    );

    let text = tokio::fs::read_to_string(&path).await.unwrap();
    assert_eq!(text, "<!--START-->new<!--END-->");
}

#[tokio::test]
async fn splice_nth_and_on_missing_noop() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("t.txt");
    tokio::fs::write(&path, "<<a>>x<<b>>").await.unwrap();
    let mut ctx = ExecutionContext::default();

    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(path.display().to_string()));
    params.insert("mode".into(), Value::Str("splice".into()));
    params.insert("start".into(), Value::Str("<<".into()));
    params.insert("end".into(), Value::Str(">>".into()));
    params.insert("content".into(), Value::Str("Y".into()));
    params.insert("nth".into(), Value::Int(2));
    FileWrite
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap();
    let text = tokio::fs::read_to_string(&path).await.unwrap();
    assert_eq!(text, "<<a>>x<<Y>>");

    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(path.display().to_string()));
    params.insert("mode".into(), Value::Str("splice".into()));
    params.insert("start".into(), Value::Str("NOPE".into()));
    params.insert("end".into(), Value::Str("X".into()));
    params.insert("content".into(), Value::Str("Z".into()));
    params.insert("on_missing".into(), Value::Str("noop".into()));
    let out = FileWrite
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap();
    assert!(
        !out.as_map()
            .unwrap()
            .get("changed")
            .unwrap()
            .as_bool()
            .unwrap()
    );
}

#[tokio::test]
async fn append_str_replace_and_lines() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("edit.txt");
    let mut ctx = ExecutionContext::default();

    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(path.display().to_string()));
    params.insert("content".into(), Value::Str("a\nb\nc\n".into()));
    FileWrite
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap();

    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(path.display().to_string()));
    params.insert("mode".into(), Value::Str("append".into()));
    params.insert("content".into(), Value::Str("d\n".into()));
    FileWrite
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap();

    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(path.display().to_string()));
    params.insert("mode".into(), Value::Str("str_replace".into()));
    params.insert("old".into(), Value::Str("b\n".into()));
    params.insert("new".into(), Value::Str("B\n".into()));
    FileWrite
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap();

    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(path.display().to_string()));
    params.insert("mode".into(), Value::Str("replace_lines".into()));
    params.insert("start_line".into(), Value::Int(1));
    params.insert("end_line".into(), Value::Int(1));
    params.insert("content".into(), Value::Str("A\n".into()));
    FileWrite
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap();

    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(path.display().to_string()));
    params.insert("mode".into(), Value::Str("insert_lines".into()));
    params.insert("after_line".into(), Value::Int(0));
    params.insert("content".into(), Value::Str("Z\n".into()));
    FileWrite
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap();

    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(path.display().to_string()));
    params.insert("mode".into(), Value::Str("delete_lines".into()));
    params.insert("start_line".into(), Value::Int(4));
    params.insert("end_line".into(), Value::Int(4));
    FileWrite
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap();

    let text = tokio::fs::read_to_string(&path).await.unwrap();
    assert_eq!(text, "Z\nA\nB\nd\n");
}

#[tokio::test]
async fn str_replace_requires_unique() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("u.txt");
    tokio::fs::write(&path, "xx").await.unwrap();
    let mut ctx = ExecutionContext::default();
    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(path.display().to_string()));
    params.insert("mode".into(), Value::Str("str_replace".into()));
    params.insert("old".into(), Value::Str("x".into()));
    params.insert("new".into(), Value::Str("y".into()));
    let err = FileWrite
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("匹配"), "got: {err}");
}

#[tokio::test]
async fn newline_crlf() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("nl.txt");
    let mut ctx = ExecutionContext::default();
    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(path.display().to_string()));
    params.insert("content".into(), Value::Str("a\nb\n".into()));
    params.insert("newline".into(), Value::Str("crlf".into()));
    FileWrite
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap();
    let bytes = tokio::fs::read(&path).await.unwrap();
    assert_eq!(bytes, b"a\r\nb\r\n");
}

#[tokio::test]
async fn read_lines_with_limit() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("r.txt");
    tokio::fs::write(&path, "a\nb\nc\nd\n").await.unwrap();
    let mut ctx = ExecutionContext::default();
    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(path.display().to_string()));
    params.insert("mode".into(), Value::Str("lines".into()));
    params.insert("start_line".into(), Value::Int(2));
    params.insert("limit".into(), Value::Int(2));
    let out = FileRead
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap();
    let m = out.as_map().unwrap();
    let lines = m.get("lines").unwrap().as_array().unwrap();
    assert_eq!(lines.len(), 2);
    assert_eq!(
        lines[0]
            .as_map()
            .unwrap()
            .get("text")
            .unwrap()
            .as_str()
            .unwrap(),
        "b"
    );
    assert_eq!(
        lines[1]
            .as_map()
            .unwrap()
            .get("text")
            .unwrap()
            .as_str()
            .unwrap(),
        "c"
    );
}

#[tokio::test]
async fn patch_unified() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("p.txt");
    tokio::fs::write(&path, "hello\n").await.unwrap();
    let mut ctx = ExecutionContext::default();
    let diff = "\
--- a/p.txt
+++ b/p.txt
@@ -1 +1 @@
-hello
+world
";
    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(path.display().to_string()));
    params.insert("mode".into(), Value::Str("patch".into()));
    params.insert("diff".into(), Value::Str(diff.into()));
    FileWrite
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap();
    let text = tokio::fs::read_to_string(&path).await.unwrap();
    assert_eq!(text, "world\n");
}

#[tokio::test]
async fn json_set_mode() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("cfg.json");
    tokio::fs::write(&path, r#"{"build":{"version":"0.1.0"}}"#)
        .await
        .unwrap();
    let mut ctx = ExecutionContext::default();
    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(path.display().to_string()));
    params.insert("mode".into(), Value::Str("json_set".into()));
    params.insert("pointer".into(), Value::Str("build.version".into()));
    params.insert("value".into(), Value::Str("1.0.0".into()));
    FileWrite
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap();
    let text = tokio::fs::read_to_string(&path).await.unwrap();
    assert!(text.contains("1.0.0"));
}

#[tokio::test]
async fn filesystem_roots_rejects_outside() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("allowed");
    std::fs::create_dir_all(&root).unwrap();
    let outside = dir.path().join("denied");
    std::fs::create_dir_all(&outside).unwrap();
    let file = outside.join("x.txt");
    std::fs::write(&file, b"x").unwrap();

    let cfg = corex_core::RuntimeConfig {
        filesystem_roots: vec![root],
        ..Default::default()
    };
    let mut ctx = ExecutionContext::new(cfg);

    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(file.display().to_string()));
    params.insert("content".into(), Value::Str("nope".into()));
    let err = FileWrite
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("越界") || msg.contains("不在") || msg.contains("无法解析"),
        "got: {msg}"
    );
}

#[tokio::test]
async fn regex_mode_rejects_long_pattern() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("t.txt");
    tokio::fs::write(&path, "hello").await.unwrap();
    let mut ctx = ExecutionContext::default();
    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(path.display().to_string()));
    params.insert("mode".into(), Value::Str("regex".into()));
    params.insert("pattern".into(), Value::Str("a".repeat(2000)));
    params.insert("replacement".into(), Value::Str("b".into()));
    let err = FileWrite
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("1024") || err.to_string().contains("pattern"),
        "got: {err}"
    );
}

#[tokio::test]
async fn read_exists_and_stat() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("x.txt");
    tokio::fs::write(&path, b"hi").await.unwrap();
    let mut ctx = ExecutionContext::default();

    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(path.display().to_string()));
    params.insert("mode".into(), Value::Str("exists".into()));
    let exists = FileRead
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap();
    assert!(exists.as_bool().unwrap());

    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(path.display().to_string()));
    params.insert("mode".into(), Value::Str("stat".into()));
    let stat = FileRead
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap();
    let m = stat.as_map().unwrap();
    assert_eq!(m.get("kind").unwrap().as_str().unwrap(), "file");
    assert_eq!(m.get("size").unwrap().as_i64().unwrap(), 2);
}

#[tokio::test]
async fn update_rename_and_remove() {
    let dir = tempdir().unwrap();
    let from = dir.path().join("a.txt");
    let to = dir.path().join("b.txt");
    tokio::fs::write(&from, b"x").await.unwrap();
    let mut ctx = ExecutionContext::default();

    let mut params = BTreeMap::new();
    params.insert("from".into(), Value::Str(from.display().to_string()));
    params.insert("to".into(), Value::Str(to.display().to_string()));
    FileUpdate
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap();
    assert!(!from.exists());
    assert!(to.exists());

    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(to.display().to_string()));
    FileRemove
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap();
    assert!(!to.exists());
}

/// `content` 是 `Bytes` 时原样落盘：下载来的图片过一趟 `String` 就废了。
///
/// 这也是 `http.send`（`response: binary`）→ `file.write` 那条链的下半截。
#[tokio::test]
async fn write_bytes_content() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("blob.bin");
    // 0x89 0x50 0x4E 0x47 不是合法 UTF-8，文本模式一定会改坏它。
    let bytes = vec![0x89u8, 0x50, 0x4E, 0x47, 0xFF, 0x00, 0x1A];
    let text = dir.path().join("blob-copy.bin");
    let mut ctx = ExecutionContext::default();

    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(path.display().to_string()));
    params.insert("content".into(), Value::Bytes(bytes.clone()));
    let out = FileWrite
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap();
    assert_eq!(
        out.as_map().unwrap().get("bytes_written"),
        Some(&Value::Int(bytes.len() as i64))
    );
    assert_eq!(tokio::fs::read(&path).await.unwrap(), bytes);
    // 字节流没有换行风格可说，别让调用方以为被改过。
    assert_eq!(
        out.as_map().unwrap().get("newline").unwrap().as_str(),
        Some("none")
    );

    // `append` 接着写；`create_dirs` 默认开着，父目录会自动建。
    let nested = dir.path().join("a").join("b.bin");
    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(nested.display().to_string()));
    params.insert("mode".into(), Value::Str("append".into()));
    params.insert("content".into(), Value::Bytes(b"hi".to_vec()));
    FileWrite
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap();
    assert_eq!(tokio::fs::read(&nested).await.unwrap(), b"hi");

    // 文本模式那些花活对字节流没有意义，选错了要直说。
    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(text.display().to_string()));
    params.insert("mode".into(), Value::Str("str_replace".into()));
    params.insert("old".into(), Value::Str("a".into()));
    params.insert("new".into(), Value::Str("b".into()));
    params.insert("content".into(), Value::Bytes(bytes.clone()));
    let err = FileWrite
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("overwrite"), "{err}");
    assert!(!text.exists(), "报错之前不该已经落盘");
}

/// `mode: bytes` 读出来的就是 `Bytes`，能原样交给 `file.write`。
#[tokio::test]
async fn read_bytes_roundtrips_through_write() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src.bin");
    let dst = dir.path().join("dst.bin");
    let bytes: Vec<u8> = (0..=255u8).collect();
    tokio::fs::write(&src, &bytes).await.unwrap();
    let mut ctx = ExecutionContext::default();

    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(src.display().to_string()));
    params.insert("mode".into(), Value::Str("bytes".into()));
    params.insert("offset".into(), Value::Int(10));
    params.insert("length".into(), Value::Int(5));
    let chunk = FileRead
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap();
    assert_eq!(chunk, Value::Bytes(bytes[10..15].to_vec()));

    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(dst.display().to_string()));
    params.insert("content".into(), chunk);
    FileWrite
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap();
    assert_eq!(tokio::fs::read(&dst).await.unwrap(), bytes[10..15].to_vec());
}

/// IPC 把 `Bytes` 序列化成整数数组，反序列化只还原成 `Array`。
///
/// 这条退化路径也得能落盘，否则 `http.send(response: binary)` → `file.write`
/// 一过 daemon 就断。
#[tokio::test]
async fn write_accepts_bytes_degraded_to_integers() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("from-ipc.bin");
    let mut ctx = ExecutionContext::default();

    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(path.display().to_string()));
    params.insert(
        "content".into(),
        Value::Array(vec![Value::Int(0x89), Value::Int(0x50), Value::Int(0xFF)]),
    );
    FileWrite
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap();
    assert_eq!(
        tokio::fs::read(&path).await.unwrap(),
        vec![0x89u8, 0x50, 0xFF]
    );

    // 超出 0..=255 的数组不是字节：仍按文本模式报错，不假装写得进去。
    let mut params = BTreeMap::new();
    params.insert("path".into(), Value::Str(path.display().to_string()));
    params.insert("content".into(), Value::Array(vec![Value::Int(300)]));
    let err = FileWrite
        .execute(Value::Map(params), &mut ctx)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("content"), "{err}");
}
