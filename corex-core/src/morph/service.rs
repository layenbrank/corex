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
    Args, MergeArgs, MetaArgs, PageImage, PdfMeta, RenderPageArgs, RenderThumbnailsArgs,
    SearchArgs, SplitArgs, SplitByCountArgs, ToImagesArgs, ToOfficeArgs,
};
use crate::utils::paths::{validate_output_dir, validate_read_file, validate_write_path};

type LopdfId = lopdf::ObjectId;

const MAX_SCALE: f32 = 10.0;
const MAX_RENDER_PAGES: usize = 200;
const MAX_THUMBNAIL_PAYLOAD_BYTES: usize = 50 * 1024 * 1024;

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
        Args::Meta(a) => meta(a),
        Args::RenderPage(a) => render_page(a),
        Args::RenderThumbnails(a) => render_thumbnails(a),
        Args::Search(a) => search(a),
        Args::Export(a) => export(a),
        Args::Merge(a) => merge(a),
        Args::Split(a) => split(a),
        Args::SplitByCount(a) => split_by_count(a),
        Args::ToImages(a) => to_images(a),
        Args::ToOffice(a) => to_office(a),
    }
}

fn validate_scale(scale: f32) -> Result<()> {
    if !scale.is_finite() || scale <= 0.0 || scale > MAX_SCALE {
        bail!("scale 必须在 (0, {MAX_SCALE}] 范围内");
    }
    Ok(())
}

fn ensure_page_count(page_count: usize, op: &str) -> Result<()> {
    if page_count > MAX_RENDER_PAGES {
        bail!("{op} 页数 {page_count} 超过上限 {MAX_RENDER_PAGES}");
    }
    Ok(())
}

fn load_pdfium() -> Result<std::sync::MutexGuard<'static, Pdfium>> {
    PDFIUM
        .get_or_init(|| {
            super::pdfium::load().map(|b| Mutex::new(Pdfium::new(b)))
        })
        .as_ref()
        .map_err(|e| anyhow::anyhow!(e.clone()))?
        .lock()
        .map_err(|e| anyhow::anyhow!("pdfium mutex poisoned: {e}"))
}

fn image_to_base64_png(img: image::DynamicImage) -> Result<String> {
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)?;
    Ok(STANDARD.encode(&buf))
}

fn render_page_to_image(page: &PdfPage, scale: f32) -> Result<(image::DynamicImage, u32, u32)> {
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

fn meta(args: &MetaArgs) -> Result<Output> {
    validate_read_file(&args.path)?;
    let pdfium = load_pdfium()?;
    let doc = pdfium
        .load_pdf_from_file(&args.path, None)
        .with_context(|| format!("无法打开 PDF: {}", args.path))?;
    let page_count = doc.pages().len() as u32;
    let meta = doc.metadata();
    let title = meta
        .get(PdfDocumentMetadataTagType::Title)
        .map(|t| t.value().to_string())
        .unwrap_or_default();
    let author = meta
        .get(PdfDocumentMetadataTagType::Author)
        .map(|t| t.value().to_string())
        .unwrap_or_default();
    let (page_width, page_height) = if page_count > 0 {
        let page = doc.pages().get(0)?;
        (page.width().value, page.height().value)
    } else {
        (595.0, 842.0)
    };
    let pdf_meta = PdfMeta {
        path: args.path.clone(),
        title,
        author,
        page_count,
        page_width,
        page_height,
    };
    Ok(Output {
        path: None,
        data: Some(serde_json::to_value(pdf_meta)?),
    })
}

fn render_page(args: &RenderPageArgs) -> Result<Output> {
    validate_read_file(&args.path)?;
    validate_scale(args.scale)?;
    let pdfium = load_pdfium()?;
    let doc = pdfium.load_pdf_from_file(&args.path, None)?;
    let page = doc.pages().get(args.page_index as i32)?;
    let (img, w, h) = render_page_to_image(&page, args.scale)?;
    let page_image = PageImage {
        data_base64: image_to_base64_png(img)?,
        width: w,
        height: h,
        page_index: args.page_index,
    };
    Ok(Output {
        path: None,
        data: Some(serde_json::to_value(page_image)?),
    })
}

fn render_thumbnails(args: &RenderThumbnailsArgs) -> Result<Output> {
    validate_read_file(&args.path)?;
    validate_scale(args.scale)?;
    let pdfium = load_pdfium()?;
    let doc = pdfium.load_pdf_from_file(&args.path, None)?;
    let page_count = doc.pages().len() as usize;
    ensure_page_count(page_count, "render-thumbnails")?;
    let mut results = Vec::with_capacity(page_count);
    let mut payload_bytes = 0usize;
    for i in 0..page_count {
        let page = doc.pages().get(i as i32)?;
        let (img, w, h) = render_page_to_image(&page, args.scale)?;
        let b64 = image_to_base64_png(img)?;
        payload_bytes += b64.len();
        if payload_bytes > MAX_THUMBNAIL_PAYLOAD_BYTES {
            bail!("缩略图总输出超过 {MAX_THUMBNAIL_PAYLOAD_BYTES} 字节上限");
        }
        results.push(PageImage {
            data_base64: b64,
            width: w,
            height: h,
            page_index: i as u32,
        });
    }
    Ok(Output {
        path: None,
        data: Some(serde_json::to_value(results)?),
    })
}

fn search(args: &SearchArgs) -> Result<Output> {
    validate_read_file(&args.path)?;
    if args.query.trim().is_empty() {
        bail!("搜索关键词不能为空");
    }
    let pdfium = load_pdfium()?;
    let doc = pdfium.load_pdf_from_file(&args.path, None)?;
    let page_count = doc.pages().len();
    let mut matches = Vec::new();
    for i in 0..page_count {
        let page = doc.pages().get(i as i32)?;
        let text = page.text()?;
        let content = text.all();
        matches.extend(crate::morph::search::search_text(
            &content,
            &args.query,
            i as u32,
        ));
    }
    Ok(Output {
        path: None,
        data: Some(serde_json::to_value(matches)?),
    })
}

fn export(args: &crate::morph::schema::ExportArgs) -> Result<Output> {
    validate_read_file(&args.src)?;
    validate_write_path(&args.dest)?;
    fs::copy(&args.src, &args.dest)
        .with_context(|| format!("复制 {} -> {}", args.src, args.dest))?;
    Ok(Output {
        path: Some(args.dest.clone()),
        data: None,
    })
}

fn split_columns(line: &str) -> Vec<String> {
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

fn merge(args: &MergeArgs) -> Result<Output> {
    if args.paths.is_empty() {
        bail!("至少需要一个输入文件");
    }
    for path in &args.paths {
        validate_read_file(path)?;
    }
    validate_write_path(&args.dest)?;
    let mut merged = LopdfDoc::with_version("1.5");
    let mut all_page_ids: Vec<LopdfId> = Vec::new();
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
        all_page_ids.extend(page_ids);
    }
    merged.objects.insert(
        pages_id,
        LopdfObj::Dictionary(dictionary! {
            "Type"  => LopdfObj::Name(b"Pages".to_vec()),
            "Kids"  => LopdfObj::Array(
                           all_page_ids.iter().map(|id| LopdfObj::Reference(*id)).collect()),
            "Count" => LopdfObj::Integer(all_page_ids.len() as i64),
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

fn collect_refs(obj: &LopdfObj, needed: &mut HashSet<LopdfId>, queue: &mut VecDeque<LopdfId>) {
    match obj {
        LopdfObj::Reference(id) => {
            if needed.insert(*id) {
                queue.push_back(*id);
            }
        }
        LopdfObj::Array(arr) => {
            for item in arr {
                collect_refs(item, needed, queue);
            }
        }
        LopdfObj::Dictionary(dict) => {
            for (_, val) in dict.iter() {
                collect_refs(val, needed, queue);
            }
        }
        LopdfObj::Stream(stream) => {
            for (_, val) in stream.dict.iter() {
                collect_refs(val, needed, queue);
            }
        }
        _ => {}
    }
}

fn collect_object_subgraph(source: &LopdfDoc, root_ids: &[LopdfId]) -> HashSet<LopdfId> {
    let mut needed = HashSet::new();
    let mut queue = VecDeque::new();
    for id in root_ids {
        if needed.insert(*id) {
            queue.push_back(*id);
        }
    }
    while let Some(id) = queue.pop_front() {
        if let Some(obj) = source.objects.get(&id) {
            collect_refs(obj, &mut needed, &mut queue);
        }
    }
    needed
}

fn split_pdf(path: &str, ranges: Vec<[u32; 2]>, dest_dir: &str) -> Result<Vec<String>> {
    validate_read_file(path)?;
    validate_output_dir(dest_dir)?;
    let source = LopdfDoc::load(path)?;
    let pages_map = source.get_pages();
    let page_count = pages_map.len() as u32;
    let stem = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    fs::create_dir_all(dest_dir)?;
    let mut output_paths = Vec::new();
    for range in &ranges {
        let start = range[0].max(1).min(page_count);
        let end = range[1].max(start).min(page_count);
        let range_ids: Vec<LopdfId> = (start..=end)
            .filter_map(|n| pages_map.get(&n).copied())
            .collect();
        if range_ids.is_empty() {
            continue;
        }
        let needed = collect_object_subgraph(&source, &range_ids);
        let mut doc = LopdfDoc::with_version("1.5");
        for id in &needed {
            if let Some(obj) = source.objects.get(id) {
                doc.objects.insert(*id, obj.clone());
            }
        }
        doc.max_id = source.max_id;
        doc.max_id += 1;
        let pages_id: LopdfId = (doc.max_id, 0);
        for &pid in &range_ids {
            if let Some(LopdfObj::Dictionary(dict)) = doc.objects.get_mut(&pid) {
                dict.set("Parent", LopdfObj::Reference(pages_id));
            }
        }
        doc.objects.insert(
            pages_id,
            LopdfObj::Dictionary(dictionary! {
                "Type"  => LopdfObj::Name(b"Pages".to_vec()),
                "Kids"  => LopdfObj::Array(
                               range_ids.iter().map(|id| LopdfObj::Reference(*id)).collect()),
                "Count" => LopdfObj::Integer(range_ids.len() as i64),
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
        let out_path = format!("{dest_dir}/{stem}_{start}_{end}.pdf");
        doc.save(&out_path)?;
        output_paths.push(out_path);
    }
    Ok(output_paths)
}

fn split(args: &SplitArgs) -> Result<Output> {
    validate_read_file(&args.path)?;
    validate_output_dir(&args.dest_dir)?;
    let ranges = parse_ranges(&args.ranges)?;
    let paths = split_pdf(&args.path, ranges, &args.dest_dir)?;
    if paths.is_empty() {
        bail!("未生成任何拆分文件，请检查页码范围");
    }
    Ok(Output {
        path: None,
        data: Some(serde_json::to_value(paths)?),
    })
}

fn split_by_count(args: &SplitByCountArgs) -> Result<Output> {
    if args.pages_per_file == 0 {
        bail!("每个文件的页数必须大于 0");
    }
    validate_read_file(&args.path)?;
    validate_output_dir(&args.dest_dir)?;
    let page_count = LopdfDoc::load(&args.path)?.get_pages().len() as u32;
    if page_count == 0 {
        bail!("PDF 无页面");
    }
    let mut ranges = Vec::new();
    let mut start = 1u32;
    while start <= page_count {
        let end = (start + args.pages_per_file - 1).min(page_count);
        ranges.push([start, end]);
        start = end + 1;
    }
    let paths = split_pdf(&args.path, ranges, &args.dest_dir)?;
    Ok(Output {
        path: None,
        data: Some(serde_json::to_value(paths)?),
    })
}

fn to_images(args: &ToImagesArgs) -> Result<Output> {
    validate_read_file(&args.path)?;
    validate_scale(args.scale)?;
    validate_output_dir(&args.dest_dir)?;
    let pdfium = load_pdfium()?;
    let doc = pdfium.load_pdf_from_file(&args.path, None)?;
    let page_count = doc.pages().len() as usize;
    ensure_page_count(page_count, "to-images")?;
    let stem = Path::new(&args.path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    fs::create_dir_all(&args.dest_dir)?;
    let is_png = !matches!(args.format.to_lowercase().as_str(), "jpg" | "jpeg");
    let ext = if is_png { "png" } else { "jpg" };
    let mut output_paths = Vec::new();
    for i in 0..page_count {
        let page = doc.pages().get(i as i32)?;
        let (img, _, _) = render_page_to_image(&page, args.scale)?;
        let out_path = format!("{}/{stem}_{:04}.{ext}", args.dest_dir, i + 1);
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

fn to_office(args: &ToOfficeArgs) -> Result<Output> {
    validate_read_file(&args.path)?;
    validate_output_dir(&args.dest_dir)?;
    let stem = Path::new(&args.path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    fs::create_dir_all(&args.dest_dir)?;
    let pdfium = load_pdfium()?;
    let doc = pdfium.load_pdf_from_file(&args.path, None)?;
    let page_count = doc.pages().len() as usize;
    ensure_page_count(page_count, "to-office")?;
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
            let out = Path::new(&args.dest_dir).join(format!("{stem}.docx"));
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
                    for (col_idx, cell) in split_columns(trimmed).iter().enumerate() {
                        if !cell.is_empty() {
                            ws.write_string(excel_row, col_idx as u16, cell.as_str())?;
                        }
                    }
                    excel_row += 1;
                }
            }
            let out = Path::new(&args.dest_dir).join(format!("{stem}.xlsx"));
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
