//! Morph PDF 动作——经 lopdf 做导出/合并/拆分；没有 pdfium 时 meta/render 软失败。

use crate::ActionRegistry;
use crate::builtin::util::{
    confine_path, copy_bytes, ensure_parent, opt_i64, opt_strs, require_map, require_path,
    require_str,
};
use async_trait::async_trait;
use corex_core::{
    Action, ActionError, ActionMeta, Bucket, ExecutionContext, ParamSchema, PermissionSet,
    SchemaType, Unit, Value,
};
use lopdf::{Document as LopdfDoc, Object as LopdfObj, ObjectId as LopdfId, dictionary};
use std::collections::{HashSet, VecDeque};
use std::path::Path;
use std::sync::Arc;

pub struct MorphMeta;
pub struct MorphRender;
pub struct MorphMerge;
pub struct MorphSplit;
pub struct MorphExport;

#[async_trait]
impl Action for MorphMeta {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::FILESYSTEM
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "morph.meta",
            "PDF 元数据",
            "读取 PDF 元数据（需要 pdfium）",
            Bucket::Data,
        )
        .with_params(vec![ParamSchema::new("path", SchemaType::File, true)])
    }

    async fn execute(
        &self,
        _params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        Err(ActionError::execution(
            "morph.meta 需要 pdfium 原生库，当前构建未启用",
        ))
    }
}

#[async_trait]
impl Action for MorphRender {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::FILESYSTEM
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "morph.render",
            "PDF 渲染",
            "渲染 PDF 单页为 PNG（需要 pdfium）",
            Bucket::Data,
        )
        .with_params(vec![
            ParamSchema::new("path", SchemaType::File, true),
            ParamSchema::new("offset", SchemaType::Int, false).with_default(0),
            ParamSchema::new("scale", SchemaType::Float, false).with_default(2.0),
        ])
    }

    async fn execute(
        &self,
        _params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        Err(ActionError::execution(
            "morph.render 需要 pdfium 原生库，当前构建未启用",
        ))
    }
}

#[async_trait]
impl Action for MorphExport {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::FILESYSTEM
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "morph.export",
            "PDF 导出",
            "复制 PDF 到目标路径",
            Bucket::Data,
        )
        .with_params(vec![
            ParamSchema::new("src", SchemaType::File, true),
            ParamSchema::new("dest", SchemaType::File, true),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let src = confine_path(ctx, &require_path(map, "src")?)?;
        let dest = confine_path(ctx, &require_path(map, "dest")?)?;
        ensure_parent(&dest)?;
        let total = std::fs::metadata(&src).map(|m| m.len()).unwrap_or(0);
        copy_bytes(&src, &dest, 0, total, ctx).await?;
        Ok(Value::File(dest))
    }
}

#[async_trait]
impl Action for MorphMerge {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::FILESYSTEM
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new("morph.merge", "PDF 合并", "合并多个 PDF", Bucket::Data).with_params(vec![
            ParamSchema::new("paths", SchemaType::Array, true),
            ParamSchema::new("dest", SchemaType::File, true),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let paths = opt_strs(map, "paths");
        if paths.is_empty() {
            return Err(ActionError::InvalidParams(
                "至少需要一个输入文件".to_string(),
            ));
        }
        let mut confined = Vec::with_capacity(paths.len());
        for path in &paths {
            confined.push(confine_path(ctx, Path::new(path))?);
        }
        let dest = confine_path(ctx, &require_path(map, "dest")?)?;
        ensure_parent(&dest)?;

        let mut merged = LopdfDoc::with_version("1.5");
        let mut kids: Vec<LopdfId> = Vec::new();
        merged.max_id += 1;
        let pages_id: LopdfId = (merged.max_id, 0);
        // 一个个文件地合，进度就报在文件数上：拆不动的是单个大 PDF，不是合计。
        ctx.chunk(0, Some(confined.len() as u64), Unit::Items);
        for (done, path) in confined.iter().enumerate() {
            let mut src = LopdfDoc::load(path)
                .map_err(|e| ActionError::execution(format!("无法加载 {}: {e}", path.display())))?;
            src.renumber_objects_with(merged.max_id + 1);
            let pages_map = src.get_pages();
            let mut sorted: Vec<(u32, LopdfId)> = pages_map.into_iter().collect();
            sorted.sort_by_key(|(k, _)| *k);
            let page_ids: Vec<LopdfId> = sorted.into_iter().map(|(_, id)| id).collect();
            for &pid in &page_ids {
                if let Some(LopdfObj::Dictionary(dict)) = src.objects.get_mut(&pid) {
                    dict.set("Parent", LopdfObj::Reference(pages_id));
                }
            }
            for (id, obj) in src.objects {
                merged.objects.insert(id, obj);
            }
            merged.max_id = src.max_id;
            kids.extend(page_ids);
            ctx.chunk(done as u64 + 1, Some(confined.len() as u64), Unit::Items);
        }
        merged.objects.insert(
            pages_id,
            LopdfObj::Dictionary(dictionary! {
                "Type"  => LopdfObj::Name(b"Pages".to_vec()),
                "Kids"  => LopdfObj::Array(
                               kids.iter().map(|id| LopdfObj::Reference(*id)).collect()),
                "Count" => LopdfObj::Integer(kids.len() as i64),
            }),
        );
        merged.max_id += 1;
        let catalog_id: LopdfId = (merged.max_id, 0);
        merged.objects.insert(
            catalog_id,
            LopdfObj::Dictionary(dictionary! {
                "Type"  => LopdfObj::Name(b"Catalog".to_vec()),
                "Pages" => LopdfObj::Reference(pages_id),
            }),
        );
        merged.trailer.set("Root", LopdfObj::Reference(catalog_id));
        merged
            .trailer
            .set("Size", LopdfObj::Integer((merged.max_id + 1) as i64));
        merged
            .save(&dest)
            .map_err(|e| ActionError::execution(format!("保存合并 PDF 失败: {e}")))?;
        Ok(Value::File(dest))
    }
}

#[async_trait]
impl Action for MorphSplit {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::FILESYSTEM
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "morph.split",
            "PDF 拆分",
            "按页数上限拆分 PDF",
            Bucket::Data,
        )
        .with_params(vec![
            ParamSchema::new("path", SchemaType::File, true),
            ParamSchema::new("dir", SchemaType::File, true),
            ParamSchema::new("limit", SchemaType::Int, false).with_default(1),
            ParamSchema::new("ranges", SchemaType::Array, false),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let path = confine_path(ctx, Path::new(&require_str(map, "path")?))?;
        let dir = confine_path(ctx, Path::new(&require_str(map, "dir")?))?;
        std::fs::create_dir_all(&dir)?;

        let ranges = if let Some(Value::Array(raw)) = map.get("ranges") {
            let mut out = Vec::new();
            for item in raw {
                let s = item.as_str().ok_or_else(|| {
                    ActionError::InvalidParams("ranges 项必须是字符串 start-end".to_string())
                })?;
                let parts: Vec<&str> = s.split('-').collect();
                if parts.len() != 2 {
                    return Err(ActionError::InvalidParams(format!("无效页码范围: {s}")));
                }
                let start: u32 = parts[0]
                    .trim()
                    .parse()
                    .map_err(|_| ActionError::InvalidParams("无效起始页".to_string()))?;
                let end: u32 = parts[1]
                    .trim()
                    .parse()
                    .map_err(|_| ActionError::InvalidParams("无效结束页".to_string()))?;
                out.push([start, end]);
            }
            out
        } else {
            let limit = opt_i64(map, "limit", 1).max(1) as u32;
            let page_count = LopdfDoc::load(&path)
                .map_err(|e| ActionError::execution(format!("加载 PDF 失败: {e}")))?
                .get_pages()
                .len() as u32;
            if page_count == 0 {
                return Err(ActionError::execution("PDF 无页面"));
            }
            let mut ranges = Vec::new();
            let mut start = 1u32;
            while start <= page_count {
                let end = (start + limit - 1).min(page_count);
                ranges.push([start, end]);
                start = end + 1;
            }
            ranges
        };

        let paths = split_pdf(&path.to_string_lossy(), ranges, &dir.to_string_lossy(), ctx)?;
        Ok(Value::Array(
            paths.into_iter().map(|p| Value::File(p.into())).collect(),
        ))
    }
}

fn push_refs(obj: &LopdfObj, needed: &mut HashSet<LopdfId>, queue: &mut VecDeque<LopdfId>) {
    match obj {
        LopdfObj::Reference(id) if needed.insert(*id) => {
            queue.push_back(*id);
        }
        LopdfObj::Array(arr) => {
            for item in arr {
                push_refs(item, needed, queue);
            }
        }
        LopdfObj::Dictionary(dict) => {
            for (key, val) in dict.iter() {
                if key.as_slice() == b"Parent" {
                    continue;
                }
                push_refs(val, needed, queue);
            }
        }
        LopdfObj::Stream(stream) => {
            for (key, val) in stream.dict.iter() {
                if key.as_slice() == b"Parent" {
                    continue;
                }
                push_refs(val, needed, queue);
            }
        }
        _ => {}
    }
}

fn walk_refs(source: &LopdfDoc, root_ids: &[LopdfId]) -> HashSet<LopdfId> {
    let mut needed = HashSet::new();
    let mut queue = VecDeque::new();
    for id in root_ids {
        if needed.insert(*id) {
            queue.push_back(*id);
        }
    }
    while let Some(id) = queue.pop_front() {
        if let Some(obj) = source.objects.get(&id) {
            push_refs(obj, &mut needed, &mut queue);
        }
    }
    needed
}

fn write_pages(source: &LopdfDoc, page_ids: &[LopdfId], dest: &str) -> Result<(), ActionError> {
    if page_ids.is_empty() {
        return Err(ActionError::execution("至少需要一页"));
    }
    let needed = walk_refs(source, page_ids);
    let mut doc = LopdfDoc::with_version("1.5");
    for id in &needed {
        if let Some(obj) = source.objects.get(id) {
            doc.objects.insert(*id, obj.clone());
        }
    }
    doc.max_id = source.max_id;
    doc.max_id += 1;
    let pages_id: LopdfId = (doc.max_id, 0);
    for &pid in page_ids {
        if let Some(LopdfObj::Dictionary(dict)) = doc.objects.get_mut(&pid) {
            dict.set("Parent", LopdfObj::Reference(pages_id));
        }
    }
    doc.objects.insert(
        pages_id,
        LopdfObj::Dictionary(dictionary! {
            "Type"  => LopdfObj::Name(b"Pages".to_vec()),
            "Kids"  => LopdfObj::Array(
                          page_ids.iter().map(|id| LopdfObj::Reference(*id)).collect()),
            "Count" => LopdfObj::Integer(page_ids.len() as i64),
        }),
    );
    doc.max_id += 1;
    let catalog_id: LopdfId = (doc.max_id, 0);
    doc.objects.insert(
        catalog_id,
        LopdfObj::Dictionary(dictionary! {
            "Type"  => LopdfObj::Name(b"Catalog".to_vec()),
            "Pages" => LopdfObj::Reference(pages_id),
        }),
    );
    doc.trailer.set("Root", LopdfObj::Reference(catalog_id));
    doc.trailer
        .set("Size", LopdfObj::Integer((doc.max_id + 1) as i64));
    doc.save(dest)
        .map_err(|e| ActionError::execution(format!("写出拆分 PDF 失败: {e}")))?;
    Ok(())
}

fn split_pdf(
    path: &str,
    ranges: Vec<[u32; 2]>,
    dir: &str,
    ctx: &ExecutionContext,
) -> Result<Vec<String>, ActionError> {
    let source =
        LopdfDoc::load(path).map_err(|e| ActionError::execution(format!("加载 PDF 失败: {e}")))?;
    let pages_map = source.get_pages();
    let page_count = pages_map.len() as u32;
    let stem = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let mut output_paths = Vec::new();
    ctx.chunk(0, Some(ranges.len() as u64), Unit::Items);
    for range in &ranges {
        let start = range[0];
        let end = range[1];
        if start < 1 || end < start || end > page_count {
            return Err(ActionError::execution(format!(
                "无效页码范围: {start}-{end}（文档共 {page_count} 页）"
            )));
        }
        let mut range_ids = Vec::new();
        for n in start..=end {
            let id = pages_map
                .get(&n)
                .copied()
                .ok_or_else(|| ActionError::execution(format!("页 {n} 不存在")))?;
            range_ids.push(id);
        }
        let out_path = format!("{dir}/{stem}_{start}_{end}.pdf");
        write_pages(&source, &range_ids, &out_path)?;
        output_paths.push(out_path);
        ctx.chunk(
            output_paths.len() as u64,
            Some(ranges.len() as u64),
            Unit::Items,
        );
    }
    Ok(output_paths)
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(MorphMeta));
    registry.register(Arc::new(MorphRender));
    registry.register(Arc::new(MorphMerge));
    registry.register(Arc::new(MorphSplit));
    registry.register(Arc::new(MorphExport));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::util::probe::Probe;
    use corex_core::Observer;
    use std::collections::BTreeMap;
    use tempfile::tempdir;

    /// 导出就是一次拷贝：进度得跟 `file.copy` 一样按字节走，而不是「开始 / 结束」两帧。
    #[tokio::test]
    async fn export_reports_bytes() {
        const SIZE: usize = 3 * 1024 * 1024;
        let dir = tempdir().unwrap();
        let src = dir.path().join("in.pdf");
        let dst = dir.path().join("out.pdf");
        std::fs::write(&src, vec![7u8; SIZE]).unwrap();

        let probe = Probe::bytes();
        let mut ctx = ExecutionContext::default();
        ctx.observer = Some(Arc::clone(&probe) as Arc<dyn Observer>);
        ctx.enter_step("export", "morph.export");

        let out = MorphExport
            .execute(
                Value::Map(BTreeMap::from([
                    ("src".into(), Value::Str(src.to_string_lossy().into())),
                    ("dest".into(), Value::Str(dst.to_string_lossy().into())),
                ])),
                &mut ctx,
            )
            .await
            .unwrap();

        assert_eq!(out, Value::File(dst.clone()));
        assert_eq!(std::fs::read(&dst).unwrap().len(), SIZE);

        let total = SIZE as u64;
        let marks = probe.marks();
        assert_eq!(marks.last(), Some(&(total, Some(total))), "{marks:?}");
        assert!(
            marks.iter().all(|(_, each)| *each == Some(total)),
            "分母要始终是这个文件：{marks:?}"
        );
    }
}
