//! generate Path 流式 stage 测试

use std::fs;

use cx::generate::schema::PathArgs;
use cx::pipeline::stream::run_path_stream;

#[test]
fn path_stream_writes_output_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("a.txt"), "hello").unwrap();
    fs::write(src.join("b.txt"), "world").unwrap();

    let out = dir.path().join("paths.txt");
    let args = PathArgs {
        from: src.display().to_string(),
        to: out.display().to_string(),
        transform: "{{filename}}".to_string(),
        index: 1,
        separator: "/".to_string(),
        pad: false,
        includes: vec![],
        excludes: vec![],
        uppercase: vec![],
        id: None,
        description: None,
    };

    let (artifact, items) = run_path_stream(&args).expect("path stream");
    assert!(out.exists());
    assert_eq!(items, 2);
    assert_eq!(artifact.path.as_deref(), Some(out.as_path()));

    let content = fs::read_to_string(&out).unwrap();
    assert!(content.contains("a.txt"));
    assert!(content.contains("b.txt"));
}
