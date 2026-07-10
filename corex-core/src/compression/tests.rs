#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::compression::schema::{Args, CompressScheme, ZipEncryption, ZipFormatArgs};
    use crate::compression::service::execute;

    #[test]
    fn compress_ipc_roundtrip() {
        let v = serde_json::json!({
            "Compress": {
                "scheme": {
                    "Zip": { "from": "a", "to": "b.zip" }
                }
            }
        });
        let args: Args = serde_json::from_value(v).unwrap();
        assert!(matches!(args, Args::Compress(_)));
    }

    #[test]
    fn zip_roundtrip_plain() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("hello.txt"), b"hello").unwrap();
        let archive = dir.path().join("out.zip");
        let extract = dir.path().join("extract");

        let args = Args::Compress(crate::compression::schema::CompressArgs {
            scheme: CompressScheme::Zip(ZipFormatArgs {
                from: src.to_string_lossy().into_owned(),
                to: archive.to_string_lossy().into_owned(),
                level: 6,
                method: crate::compression::schema::ZipMethod::Deflated,
                encryption: ZipEncryption::None,
                io: Default::default(),
                description: None,
                id: None,
            }),
        });
        execute(&args).unwrap();
        assert!(archive.is_file());

        let decompress = Args::Decompress(crate::compression::schema::DecompressArgs {
            scheme: crate::compression::schema::DecompressScheme::Zip(
                crate::compression::schema::ZipDecompressArgs {
                    from: archive.to_string_lossy().into_owned(),
                    to: extract.to_string_lossy().into_owned(),
                    io: Default::default(),
                    description: None,
                    id: None,
                },
            ),
        });
        execute(&decompress).unwrap();
        assert!(extract.join("hello.txt").is_file());
    }

    #[test]
    fn zip_roundtrip_aes256() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("secret.txt"), b"secret-data").unwrap();
        let archive = dir.path().join("secure.zip");
        let extract = dir.path().join("extract");

        let mut io = crate::compression::schema::ArchiveIoArgs::default();
        io.password = Some("test-password".to_string());

        let args = Args::Compress(crate::compression::schema::CompressArgs {
            scheme: CompressScheme::Zip(ZipFormatArgs {
                from: src.to_string_lossy().into_owned(),
                to: archive.to_string_lossy().into_owned(),
                level: 6,
                method: crate::compression::schema::ZipMethod::Deflated,
                encryption: ZipEncryption::Aes256,
                io: io.clone(),
                description: None,
                id: None,
            }),
        });
        execute(&args).unwrap();

        let decompress = Args::Decompress(crate::compression::schema::DecompressArgs {
            scheme: crate::compression::schema::DecompressScheme::Zip(
                crate::compression::schema::ZipDecompressArgs {
                    from: archive.to_string_lossy().into_owned(),
                    to: extract.to_string_lossy().into_owned(),
                    io,
                    description: None,
                    id: None,
                },
            ),
        });
        execute(&decompress).unwrap();
        let content = fs::read_to_string(extract.join("secret.txt")).unwrap();
        assert_eq!(content, "secret-data");
    }

    #[test]
    fn tar_gz_rejects_password() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.txt"), b"x").unwrap();

        let mut io = crate::compression::schema::ArchiveIoArgs::default();
        io.password = Some("nope".to_string());

        let args = Args::Compress(crate::compression::schema::CompressArgs {
            scheme: CompressScheme::TarGz(crate::compression::schema::TarGzFormatArgs {
                from: src.to_string_lossy().into_owned(),
                to: dir.path().join("out.tar.gz").to_string_lossy().into_owned(),
                level: 6,
                preserve_permissions: false,
                io,
                description: None,
                id: None,
            }),
        });
        assert!(execute(&args).is_err());
    }
}
