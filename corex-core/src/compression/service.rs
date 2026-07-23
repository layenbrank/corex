use anyhow::Result;

use crate::compression::formats::{
    compress_seven_z, compress_tar_gz, compress_zip, decompress_seven_z, decompress_tar_gz,
    decompress_zip,
};
use crate::compression::schema::{Args, CompressFormat, DecompressFormat};
use crate::utils::notify;

#[derive(Debug, Clone)]
pub struct Output {
    pub path: Option<String>,
}

pub fn run(args: &Args) -> Result<()> {
    match execute(args) {
        Ok(output) => {
            let _ = notify::success("压缩操作成功", "归档操作已完成");
            if let Some(path) = &output.path {
                println!("✅ {path}");
            }
            Ok(())
        }
        Err(e) => {
            let _ = notify::error("压缩操作失败", &format!("{e}"));
            Err(e)
        }
    }
}

pub fn execute(args: &Args) -> Result<Output> {
    match args {
        Args::Compress(a) => match &a.format {
            CompressFormat::Zip(z) => {
                compress_zip(z)?;
                Ok(Output {
                    path: Some(z.to.clone()),
                })
            }
            CompressFormat::TarGz(t) => {
                compress_tar_gz(t)?;
                Ok(Output {
                    path: Some(t.to.clone()),
                })
            }
            CompressFormat::SevenZ(s) => {
                compress_seven_z(s)?;
                Ok(Output {
                    path: Some(s.to.clone()),
                })
            }
        },
        Args::Decompress(a) => match &a.format {
            DecompressFormat::Zip(z) => {
                decompress_zip(z)?;
                Ok(Output {
                    path: Some(z.to.clone()),
                })
            }
            DecompressFormat::TarGz(t) => {
                decompress_tar_gz(t)?;
                Ok(Output {
                    path: Some(t.to.clone()),
                })
            }
            DecompressFormat::SevenZ(s) => {
                decompress_seven_z(s)?;
                Ok(Output {
                    path: Some(s.to.clone()),
                })
            }
        },
    }
}
