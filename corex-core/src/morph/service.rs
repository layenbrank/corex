#![allow(non_snake_case)]
//! Morph service：方法名 camelCase（toMeta / toRender / …）。
//! schema 字段见 `schema.rs`（count / limit / offset / dir / base64）。

use std::collections::{HashSet, VecDeque};
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use lopdf::{Document as LopdfDoc, Object as LopdfObj, dictionary};
use pdfium_render::prelude::*;
use serde_json::Value;

use crate::morph::schema::{
    Args, DocumentArgs, ExtractArgs, ImagesArgs, MatchArgs, MergeArgs, MetaArgs, PageImage,
    PdfMeta, RemoveArgs, RenderArgs, ReorderArgs, RotateArgs, SplitArgs, SplitMode, ThumbnailsArgs,
};
use crate::utils::paths::{validate_output_dir, validate_read_file, validate_write_path};

type LopdfId = lopdf::ObjectId;

const MAX_SCALE: f32 = 10.0;
/// 单次渲染/导出允许的最大页数
const MAX_PAGES: usize = 200;
/// 缩略图 base64 总字节上限
const MAX_THUMB_BYTES: usize = 50 * 1024 * 1024;

static PDFIUM: OnceLock<Result<Mutex<Pdfium>, String>> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct Output {
    pub path: Option<String>,
    pub data: Option<Value>,
}

pub fn run(args: &Args) -> Result<()> {
    let output = execute(args)?;
    if let Some(path) = &output.path {
        println!("✅ {path}");
    }
    if let Some(data) = &output.data {
        println!("{}", serde_json::to_string_pretty(data)?);
    }
    Ok(())
}

pub fn execute(args: &Args) -> Result<Output> {
    match args {
        Args::Meta(a) => toMeta(a),
        Args::Render(a) => toRender(a),
        Args::Thumbnails(a) => toThumbnails(a),
        Args::Match(a) => toMatch(a),
        Args::Export(a) => toExport(a),
        Args::Merge(a) => toMerge(a),
        Args::Split(a) => toSplit(a),
        Args::Images(a) => toImages(a),
        Args::Document(a) => toDocument(a),
        Args::Reorder(a) => toReorder(a),
        Args::Rotate(a) => toRotate(a),
        Args::Remove(a) => toRemove(a),
        Args::Extract(a) => toExtract(a),
    }
}

fn check_scale(scale: f32) -> Result<()> {
    if !scale.is_finite() || scale <= 0.0 || scale > MAX_SCALE {
        bail!("scale 必须在 (0, {MAX_SCALE}] 范围内");
    }
    Ok(())
}

/// 校验页数未超过 `MAX_PAGES`
fn check_pages(count: usize, op: &str) -> Result<()> {
    if count > MAX_PAGES {
        bail!("{op} 页数 {count} 超过上限 {MAX_PAGES}");
    }
    Ok(())
}

/// 懒加载并锁定全局 Pdfium 实例
fn bootstrap() -> Result<std::sync::MutexGuard<'static, Pdfium>> {
    PDFIUM
        .get_or_init(|| super::pdfium::load().map(|b| Mutex::new(Pdfium::new(b))))
        .as_ref()
        .map_err(|e| anyhow::anyhow!(e.clone()))?
        .lock()
        .map_err(|e| anyhow::anyhow!("pdfium mutex poisoned: {e}"))
}

/// PNG → base64
fn png_b64(img: image::DynamicImage) -> Result<String> {
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)?;
    Ok(STANDARD.encode(&buf))
}

/// 按 scale 栅格化单页，返回 (图像, 宽, 高)
fn page_img(page: &PdfPage, scale: f32) -> Result<(image::DynamicImage, u32, u32)> {
    let target_w = (page.width().value * scale) as i32;
    let target_h = (page.height().value * scale) as i32;
    let config = PdfRenderConfig::new()
        .set_target_width(target_w)
        .set_maximum_height(target_h);
    let bitmap = page.render_with_config(&config)?;
    let img = bitmap.as_image()?;
    let (w, h) = (img.width(), img.height());
    Ok((img, w, h))
}

fn toMeta(args: &MetaArgs) -> Result<Output> {
    validate_read_file(&args.path)?;
    let pdfium = bootstrap()?;
    let doc = pdfium
        .load_pdf_from_file(&args.path, None)
        .with_context(|| format!("无法打开 PDF: {}", args.path))?;
    let count = doc.pages().len() as u32;
    let meta = doc.metadata();
    let title = meta
        .get(PdfDocumentMetadataTagType::Title)
        .map(|t| t.value().to_string())
        .unwrap_or_default();
    let author = meta
        .get(PdfDocumentMetadataTagType::Author)
        .map(|t| t.value().to_string())
        .unwrap_or_default();
    let (width, height) = if count > 0 {
        let page = doc.pages().get(0)?;
        (page.width().value, page.height().value)
    } else {
        (595.0, 842.0)
    };
    let pdf_meta = PdfMeta {
        path: args.path.clone(),
        title,
        author,
        count,
        width,
        height,
    };
    Ok(Output {
        path: None,
        data: Some(serde_json::to_value(pdf_meta)?),
    })
}

fn toRender(args: &RenderArgs) -> Result<Output> {
    validate_read_file(&args.path)?;
    check_scale(args.scale)?;
    let pdfium = bootstrap()?;
    let doc = pdfium.load_pdf_from_file(&args.path, None)?;
    let page = doc.pages().get(args.offset as i32)?;
    let (img, w, h) = page_img(&page, args.scale)?;
    let page_image = PageImage {
        base64: png_b64(img)?,
        width: w,
        height: h,
        offset: args.offset,
    };
    Ok(Output {
        path: None,
        data: Some(serde_json::to_value(page_image)?),
    })
}

fn toThumbnails(args: &ThumbnailsArgs) -> Result<Output> {
    validate_read_file(&args.path)?;
    check_scale(args.scale)?;
    let pdfium = bootstrap()?;
    let doc = pdfium.load_pdf_from_file(&args.path, None)?;
    let page_count = doc.pages().len() as usize;
    check_pages(page_count, "thumbnails")?;
    let mut results = Vec::with_capacity(page_count);
    let mut payload_bytes = 0usize;
    for i in 0..page_count {
        let page = doc.pages().get(i as i32)?;
        let (img, w, h) = page_img(&page, args.scale)?;
        let b64 = png_b64(img)?;
        payload_bytes += b64.len();
        if payload_bytes > MAX_THUMB_BYTES {
            bail!("缩略图总输出超过 {MAX_THUMB_BYTES} 字节上限");
        }
        results.push(PageImage {
            base64: b64,
            width: w,
            height: h,
            offset: i as u32,
        });
    }
    Ok(Output {
        path: None,
        data: Some(serde_json::to_value(results)?),
    })
}

fn toMatch(args: &MatchArgs) -> Result<Output> {
    validate_read_file(&args.path)?;
    if args.query.trim().is_empty() {
        bail!("搜索关键词不能为空");
    }
    let pdfium = bootstrap()?;
    let doc = pdfium.load_pdf_from_file(&args.path, None)?;
    let page_count = doc.pages().len();
    let mut hits = Vec::new();
    for i in 0..page_count {
        let page = doc.pages().get(i as i32)?;
        let text = page.text()?;
        let content = text.all();
        hits.extend(crate::morph::hit::find_hits(
            &content,
            &args.query,
            i as u32,
        ));
    }
    Ok(Output {
        path: None,
        data: Some(serde_json::to_value(hits)?),
    })
}

fn toExport(args: &crate::morph::schema::ExportArgs) -> Result<Output> {
    validate_read_file(&args.src)?;
    validate_write_path(&args.dest)?;
    fs::copy(&args.src, &args.dest)
        .with_context(|| format!("复制 {} -> {}", args.src, args.dest))?;
    Ok(Output {
        path: Some(args.dest.clone()),
        data: None,
    })
}

/// 按连续空格拆成文本列
fn split_cols(line: &str) -> Vec<String> {
    let mut cols = Vec::new();
    let mut current = String::new();
    let mut space_run = 0usize;
    for ch in line.chars() {
        if ch == ' ' {
            space_run += 1;
            if space_run < 2 {
                current.push(ch);
            } else if space_run == 2 {
                if current.ends_with(' ') {
                    current.pop();
                }
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    cols.push(trimmed);
                }
                current = String::new();
            }
        } else {
            space_run = 0;
            current.push(ch);
        }
    }
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        cols.push(trimmed);
    }
    if cols.is_empty() && !line.trim().is_empty() {
        cols.push(line.trim().to_string());
    }
    cols
}

fn toMerge(args: &MergeArgs) -> Result<Output> {
    if args.paths.is_empty() {
        bail!("至少需要一个输入文件");
    }
    for path in &args.paths {
        validate_read_file(path)?;
    }
    validate_write_path(&args.dest)?;
    let mut merged = LopdfDoc::with_version("1.5");
    let mut kids: Vec<LopdfId> = Vec::new();
    merged.max_id += 1;
    let pages_id: LopdfId = (merged.max_id, 0);
    for path in &args.paths {
        let mut src = LopdfDoc::load(path).with_context(|| format!("无法加载 {path}"))?;
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
    merged.save(&args.dest)?;
    Ok(Output {
        path: Some(args.dest.clone()),
        data: None,
    })
}

fn parse_ranges(raw: &[String]) -> Result<Vec<[u32; 2]>> {
    raw.iter()
        .map(|s| {
            let parts: Vec<&str> = s.split('-').collect();
            if parts.len() != 2 {
                bail!("无效页码范围: {s}，期望格式 start-end");
            }
            let start: u32 = parts[0].trim().parse().context("无效起始页")?;
            let end: u32 = parts[1].trim().parse().context("无效结束页")?;
            Ok([start, end])
        })
        .collect()
}

/// 收集对象图中的间接引用（跳过 Parent，避免拉入整棵页树）
fn push_refs(obj: &LopdfObj, needed: &mut HashSet<LopdfId>, queue: &mut VecDeque<LopdfId>) {
    match obj {
        LopdfObj::Reference(id) => {
            if needed.insert(*id) {
                queue.push_back(*id);
            }
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

/// BFS：从根 page id 收集依赖对象
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

/// 从源文档按给定 page object id 顺序写出新 PDF。
fn write_pages(source: &LopdfDoc, page_ids: &[LopdfId], dest: &str) -> Result<()> {
    if page_ids.is_empty() {
        bail!("至少需要一页");
    }
    validate_write_path(dest)?;
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
    doc.save(dest)?;
    Ok(())
}

/// 按 1-based 页码顺序解析 page object id。
fn page_order(source: &LopdfDoc) -> Vec<LopdfId> {
    let pages_map = source.get_pages();
    let mut sorted: Vec<(u32, LopdfId)> = pages_map.into_iter().collect();
    sorted.sort_by_key(|(k, _)| *k);
    sorted.into_iter().map(|(_, id)| id).collect()
}

/// 0-based 页索引 → page object id
fn map_pages(page_ids: &[LopdfId], pages: &[u32], op: &str) -> Result<Vec<LopdfId>> {
    if pages.is_empty() {
        bail!("{op} 至少需要指定一页");
    }
    let count = page_ids.len() as u32;
    let mut out = Vec::with_capacity(pages.len());
    for &idx in pages {
        if idx >= count {
            bail!("{op} 页索引 {idx} 超出范围（0..{count}）");
        }
        out.push(page_ids[idx as usize]);
    }
    Ok(out)
}

/// 按 1-based 闭区间写出拆分文件
fn split_pdf(path: &str, ranges: Vec<[u32; 2]>, dir: &str) -> Result<Vec<String>> {
    validate_read_file(path)?;
    validate_output_dir(dir)?;
    let source = LopdfDoc::load(path)?;
    let pages_map = source.get_pages();
    let page_count = pages_map.len() as u32;
    let stem = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    fs::create_dir_all(dir)?;
    let mut output_paths = Vec::new();
    for range in &ranges {
        let start = range[0];
        let end = range[1];
        if start < 1 || end < start || end > page_count {
            bail!("无效页码范围: {start}-{end}（文档共 {page_count} 页）");
        }
        let range_ids: Vec<LopdfId> = (start..=end)
            .map(|n| {
                pages_map
                    .get(&n)
                    .copied()
                    .ok_or_else(|| anyhow::anyhow!("页 {n} 不存在"))
            })
            .collect::<Result<_>>()?;
        let out_path = format!("{dir}/{stem}_{start}_{end}.pdf");
        write_pages(&source, &range_ids, &out_path)?;
        output_paths.push(out_path);
    }
    Ok(output_paths)
}

fn toSplit(args: &SplitArgs) -> Result<Output> {
    validate_read_file(&args.path)?;
    validate_output_dir(&args.dir)?;
    let ranges = match args.mode()? {
        SplitMode::Ranges { ranges } => parse_ranges(&ranges)?,
        SplitMode::Limit { limit } => {
            let page_count = LopdfDoc::load(&args.path)?.get_pages().len() as u32;
            if page_count == 0 {
                bail!("PDF 无页面");
            }
            let mut ranges = Vec::new();
            let mut start = 1u32;
            while start <= page_count {
                let end = (start + limit - 1).min(page_count);
                ranges.push([start, end]);
                start = end + 1;
            }
            ranges
        }
    };
    let paths = split_pdf(&args.path, ranges, &args.dir)?;
    if paths.is_empty() {
        bail!("未生成任何拆分文件，请检查页码范围");
    }
    Ok(Output {
        path: None,
        data: Some(serde_json::to_value(paths)?),
    })
}

fn toImages(args: &ImagesArgs) -> Result<Output> {
    validate_read_file(&args.path)?;
    check_scale(args.scale)?;
    validate_output_dir(&args.dir)?;
    let pdfium = bootstrap()?;
    let doc = pdfium.load_pdf_from_file(&args.path, None)?;
    let page_count = doc.pages().len() as usize;
    check_pages(page_count, "images")?;
    let stem = Path::new(&args.path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    fs::create_dir_all(&args.dir)?;
    let is_png = !matches!(args.format.to_lowercase().as_str(), "jpg" | "jpeg");
    let ext = if is_png { "png" } else { "jpg" };
    let mut output_paths = Vec::new();
    for i in 0..page_count {
        let page = doc.pages().get(i as i32)?;
        let (img, _, _) = page_img(&page, args.scale)?;
        let out_path = format!("{}/{stem}_{:04}.{ext}", args.dir, i + 1);
        if is_png {
            img.save_with_format(&out_path, image::ImageFormat::Png)?;
        } else {
            img.save_with_format(&out_path, image::ImageFormat::Jpeg)?;
        }
        output_paths.push(out_path);
    }
    Ok(Output {
        path: None,
        data: Some(serde_json::to_value(output_paths)?),
    })
}

fn toDocument(args: &DocumentArgs) -> Result<Output> {
    validate_read_file(&args.path)?;
    validate_output_dir(&args.dir)?;
    let stem = Path::new(&args.path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    fs::create_dir_all(&args.dir)?;
    let pdfium = bootstrap()?;
    let doc = pdfium.load_pdf_from_file(&args.path, None)?;
    let page_count = doc.pages().len() as usize;
    check_pages(page_count, "document")?;
    let out_path = match args.format.to_lowercase().as_str() {
        "docx" => {
            use docx_rs::{BreakType, Docx, Paragraph, Run};
            let mut docx = Docx::new();
            let mut first_page = true;
            for i in 0..page_count {
                let page = doc.pages().get(i as i32)?;
                let content = page.text()?.all();
                if !first_page {
                    docx = docx.add_paragraph(
                        Paragraph::new().add_run(Run::new().add_break(BreakType::Page)),
                    );
                }
                first_page = false;
                for line in content.split('\n') {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        docx = docx.add_paragraph(Paragraph::new());
                    } else {
                        docx = docx
                            .add_paragraph(Paragraph::new().add_run(Run::new().add_text(trimmed)));
                    }
                }
            }
            let out = Path::new(&args.dir).join(format!("{stem}.docx"));
            docx.build().pack(fs::File::create(&out)?)?;
            out.to_string_lossy().into_owned()
        }
        "xlsx" => {
            use rust_xlsxwriter::Workbook;
            let mut workbook = Workbook::new();
            for i in 0..page_count {
                let content = doc.pages().get(i as i32)?.text()?.all();
                let ws = workbook.add_worksheet();
                ws.set_name(format!("Page {}", i + 1))?;
                let mut excel_row: u32 = 0;
                for line in content.split('\n') {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    for (col_idx, cell) in split_cols(trimmed).iter().enumerate() {
                        if !cell.is_empty() {
                            ws.write_string(excel_row, col_idx as u16, cell.as_str())?;
                        }
                    }
                    excel_row += 1;
                }
            }
            let out = Path::new(&args.dir).join(format!("{stem}.xlsx"));
            workbook.save(&out)?;
            out.to_string_lossy().into_owned()
        }
        other => bail!("不支持的格式: {other}，请选择 docx 或 xlsx"),
    };
    Ok(Output {
        path: Some(out_path),
        data: None,
    })
}

fn toReorder(args: &ReorderArgs) -> Result<Output> {
    validate_read_file(&args.path)?;
    validate_write_path(&args.dest)?;
    let source = LopdfDoc::load(&args.path)?;
    let page_ids = page_order(&source);
    let count = page_ids.len() as u32;
    if args.order.len() as u32 != count {
        bail!(
            "reorder-pages order 长度 {} 与页数 {count} 不一致",
            args.order.len()
        );
    }
    let mut seen = HashSet::with_capacity(args.order.len());
    for &idx in &args.order {
        if idx >= count {
            bail!("reorder-pages 页索引 {idx} 超出范围（0..{count}）");
        }
        if !seen.insert(idx) {
            bail!("reorder-pages order 含重复页索引 {idx}");
        }
    }
    let ordered: Vec<LopdfId> = args
        .order
        .iter()
        .map(|&idx| page_ids[idx as usize])
        .collect();
    write_pages(&source, &ordered, &args.dest)?;
    Ok(Output {
        path: Some(args.dest.clone()),
        data: None,
    })
}

fn toRotate(args: &RotateArgs) -> Result<Output> {
    validate_read_file(&args.path)?;
    validate_write_path(&args.dest)?;
    if args.degrees % 90 != 0 {
        bail!("rotate-pages degrees 须为 90 的倍数，收到 {}", args.degrees);
    }
    let delta = ((args.degrees % 360) + 360) % 360;
    let source = LopdfDoc::load(&args.path)?;
    let page_ids = page_order(&source);
    map_pages(&page_ids, &args.pages, "rotate")?;
    // 先按原顺序写出，再在目标文件上改 Rotate（避免改源文件）
    write_pages(&source, &page_ids, &args.dest)?;
    let mut dest_doc = LopdfDoc::load(&args.dest)?;
    let dest_ids = page_order(&dest_doc);
    for &idx in &args.pages {
        let pid = dest_ids[idx as usize];
        if let Some(LopdfObj::Dictionary(dict)) = dest_doc.objects.get_mut(&pid) {
            let current = match dict.get(b"Rotate") {
                Ok(LopdfObj::Integer(v)) => *v,
                _ => 0,
            };
            let next = (current + delta as i64).rem_euclid(360);
            dict.set("Rotate", LopdfObj::Integer(next));
        } else {
            bail!("rotate-pages 无法读取页对象");
        }
    }
    dest_doc.save(&args.dest)?;
    Ok(Output {
        path: Some(args.dest.clone()),
        data: None,
    })
}

fn toRemove(args: &RemoveArgs) -> Result<Output> {
    validate_read_file(&args.path)?;
    validate_write_path(&args.dest)?;
    let source = LopdfDoc::load(&args.path)?;
    let page_ids = page_order(&source);
    let count = page_ids.len() as u32;
    if args.pages.is_empty() {
        bail!("delete-pages 至少需要指定一页");
    }
    let mut remove = HashSet::new();
    for &idx in &args.pages {
        if idx >= count {
            bail!("delete-pages 页索引 {idx} 超出范围（0..{count}）");
        }
        remove.insert(idx);
    }
    if remove.len() as u32 >= count {
        bail!("delete-pages 不能删除全部页面");
    }
    let kept: Vec<LopdfId> = page_ids
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !remove.contains(&(*i as u32)))
        .map(|(_, id)| id)
        .collect();
    write_pages(&source, &kept, &args.dest)?;
    Ok(Output {
        path: Some(args.dest.clone()),
        data: None,
    })
}

fn toExtract(args: &ExtractArgs) -> Result<Output> {
    validate_read_file(&args.path)?;
    validate_write_path(&args.dest)?;
    let source = LopdfDoc::load(&args.path)?;
    let page_ids = page_order(&source);
    let selected = map_pages(&page_ids, &args.pages, "extract")?;
    write_pages(&source, &selected, &args.dest)?;
    Ok(Output {
        path: Some(args.dest.clone()),
        data: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::morph::schema::{ExtractArgs, RemoveArgs, ReorderArgs, RotateArgs};
    use lopdf::{Object, Stream, dictionary};

    fn make_blank_pdf(path: &Path, page_count: u32) -> Result<()> {
        let mut doc = LopdfDoc::with_version("1.5");
        let pages_id = doc.new_object_id();
        let mut kids = Vec::new();
        for i in 0..page_count {
            let content_id = doc.add_object(Stream::new(
                dictionary! {},
                format!("BT /F1 12 Tf 100 700 Td (page-{i}) Tj ET").into_bytes(),
            ));
            let page_id = doc.add_object(dictionary! {
                "Type" => Object::Name(b"Page".to_vec()),
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => Object::Array(vec![
                    Object::Integer(0),
                    Object::Integer(0),
                    Object::Integer(612),
                    Object::Integer(792),
                ]),
                "Contents" => Object::Reference(content_id),
            });
            kids.push(Object::Reference(page_id));
        }
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Pages".to_vec()),
                "Kids" => Object::Array(kids),
                "Count" => Object::Integer(page_count as i64),
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
        });
        doc.trailer.set("Root", Object::Reference(catalog_id));
        doc.save(path)?;
        Ok(())
    }

    #[test]
    fn reorder_reverses() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("in.pdf");
        let dest = dir.path().join("out.pdf");
        make_blank_pdf(&src, 3).unwrap();
        toReorder(&ReorderArgs {
            path: src.to_string_lossy().into_owned(),
            order: vec![2, 1, 0],
            dest: dest.to_string_lossy().into_owned(),
        })
        .unwrap();
        let doc = LopdfDoc::load(&dest).unwrap();
        assert_eq!(page_order(&doc).len(), 3);
    }

    #[test]
    fn remove_shrinks() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("in.pdf");
        let dest = dir.path().join("out.pdf");
        make_blank_pdf(&src, 4).unwrap();
        toRemove(&RemoveArgs {
            path: src.to_string_lossy().into_owned(),
            pages: vec![1, 3],
            dest: dest.to_string_lossy().into_owned(),
        })
        .unwrap();
        let doc = LopdfDoc::load(&dest).unwrap();
        assert_eq!(page_order(&doc).len(), 2);
        // 被删页内容流不得残留
        let raw = fs::read(&dest).unwrap();
        let text = String::from_utf8_lossy(&raw);
        assert!(text.contains("page-0"));
        assert!(text.contains("page-2"));
        assert!(!text.contains("page-1"));
        assert!(!text.contains("page-3"));
    }

    #[test]
    fn split_rejects_bad_range() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("in.pdf");
        make_blank_pdf(&src, 3).unwrap();
        let path = src.to_string_lossy().into_owned();
        let out = dir.path().to_string_lossy().into_owned();
        assert!(split_pdf(&path, vec![[1, 10]], &out).is_err());
        assert!(split_pdf(&path, vec![[3, 1]], &out).is_err());
        assert!(split_pdf(&path, vec![[0, 1]], &out).is_err());
    }

    #[test]
    fn extract_keeps() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("in.pdf");
        let dest = dir.path().join("out.pdf");
        make_blank_pdf(&src, 5).unwrap();
        toExtract(&ExtractArgs {
            path: src.to_string_lossy().into_owned(),
            pages: vec![0, 2, 4],
            dest: dest.to_string_lossy().into_owned(),
        })
        .unwrap();
        let doc = LopdfDoc::load(&dest).unwrap();
        assert_eq!(page_order(&doc).len(), 3);
    }

    #[test]
    fn rotate_sets() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("in.pdf");
        let dest = dir.path().join("out.pdf");
        make_blank_pdf(&src, 2).unwrap();
        toRotate(&RotateArgs {
            path: src.to_string_lossy().into_owned(),
            pages: vec![0],
            degrees: 90,
            dest: dest.to_string_lossy().into_owned(),
        })
        .unwrap();
        let doc = LopdfDoc::load(&dest).unwrap();
        let ids = page_order(&doc);
        let rotate = match doc.objects.get(&ids[0]) {
            Some(Object::Dictionary(dict)) => match dict.get(b"Rotate") {
                Ok(Object::Integer(v)) => *v,
                _ => 0,
            },
            _ => panic!("missing page"),
        };
        assert_eq!(rotate, 90);
    }
}
