//! PDF 转图片
//!
//! 使用 `pdfium-render`(Chrome 同款 PDFium 引擎)渲染 PDF 页面为位图。
//! 运行时动态加载 pdfium 动态库:优先环境变量 `PDFIUM_PATH`,其次 Tauri 资源目录,
//! 最后系统库路径。

use image::ImageFormat;
use pdfium_render::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::errors::{CoreError, Result};

/// 进度回调:`(current_page, total_pages)`
pub type ProgressFn = Box<dyn Fn(u32, u32) + Send + Sync>;

static PDFIUM: OnceLock<Pdfium> = OnceLock::new();

/// 定位 pdfium 动态库所在目录
fn locate_pdfium_dir() -> Result<PathBuf> {
    // 1. 环境变量
    if let Ok(p) = std::env::var("PDFIUM_PATH") {
        let path = PathBuf::from(p);
        if path.exists() {
            return Ok(path);
        }
    }
    // 2. 可执行文件同级 resources/pdfium
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let candidates = [
                parent.join("resources").join("pdfium"),
                parent.join("pdfium"),
            ];
            for c in candidates {
                if c.exists() {
                    return Ok(c);
                }
            }
        }
    }
    // 3. 当前工作目录
    let cwd_pdfium = PathBuf::from("frontend/src-tauri/resources/pdfium");
    if cwd_pdfium.exists() {
        return Ok(cwd_pdfium);
    }
    Err(CoreError::Pdfium(
        "未找到 pdfium 动态库,请设置 PDFIUM_PATH 环境变量".into(),
    ))
}

fn pdfium() -> Result<&'static Pdfium> {
    if let Some(p) = PDFIUM.get() {
        return Ok(p);
    }
    let dir = locate_pdfium_dir()?;
    let lib_path = Pdfium::pdfium_platform_library_name_at_path(&dir);
    let bindings = Pdfium::bind_to_library(lib_path)
        .map_err(|e| CoreError::Pdfium(format!("绑定 pdfium 库失败: {e}")))?;
    let pdfium = Pdfium::new(bindings);
    // 写入 OnceLock(若已被其他线程写入则忽略)
    let _ = PDFIUM.set(pdfium);
    Ok(PDFIUM.get().expect("pdfium 已初始化"))
}

/// 渲染单页为图片字节
pub fn render_page_to_bytes(
    input: &Path,
    page_index: u32,
    dpi: u32,
    fmt: ImageFormat,
) -> Result<Vec<u8>> {
    let pdfium = pdfium()?;
    let document = pdfium.load_pdf_from_file(input, None)?;
    let total = document.pages().len() as u32;
    if page_index >= total {
        return Err(CoreError::PageNotFound(page_index + 1));
    }
    let page = document.pages().get(page_index as PdfPageIndex)?;
    let cfg = render_config(&page, dpi);
    let img = page
        .render_with_config(&cfg)
        .map_err(|e| CoreError::Pdfium(format!("渲染失败: {e}")))?
        .as_image()?;
    let mut buf = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buf), fmt)
        .map_err(|e| CoreError::Image(format!("编码图片失败: {e}")))?;
    Ok(buf)
}

/// 渲染单页为 image::DynamicImage
pub fn render_page_to_image(input: &Path, page_index: u32, dpi: u32) -> Result<image::DynamicImage> {
    let pdfium = pdfium()?;
    let document = pdfium.load_pdf_from_file(input, None)?;
    let page = document.pages().get(page_index as PdfPageIndex)?;
    let cfg = render_config(&page, dpi);
    let img = page
        .render_with_config(&cfg)
        .map_err(|e| CoreError::Pdfium(format!("渲染失败: {e}")))?
        .as_image()?;
    Ok(img)
}

/// 渲染页面到指定目录。进度通过 `progress` 回调上报。
///
/// `pages` 为 `Some` 时仅渲染列表中的页(1-based,内部自动去重并升序排序),
/// 输出文件名中的页码保持原始 PDF 页码、不因过滤重新编号;为 `None` 时渲染全部页。
/// 页码越界(小于 1 或大于总页数)返回 [`CoreError::InvalidArgument`]。
pub fn render_all_pages(
    input: &Path,
    out_dir: &Path,
    dpi: u32,
    fmt: ImageFormat,
    pages: Option<&[u32]>,
    progress: impl Fn(u32, u32),
) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(out_dir)?;
    let pdfium = pdfium()?;
    let document = pdfium.load_pdf_from_file(input, None)?;
    let total = document.pages().len() as u32;
    let ext = match fmt {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::WebP => "webp",
        _ => "png",
    };

    // 归一化待渲染页列表:None = 全部页;Some = 去重 + 升序,并校验越界
    let selected: Vec<u32> = match pages {
        None => (1..=total).collect(),
        Some(list) => {
            if list.is_empty() {
                return Err(CoreError::InvalidArgument("页码列表为空".into()));
            }
            let mut sorted = list.to_vec();
            sorted.sort_unstable();
            sorted.dedup();
            // 排序后首元素为最小页码,末元素为最大页码
            if sorted[0] == 0 {
                return Err(CoreError::InvalidArgument("页码不能小于 1".into()));
            }
            let max_page = sorted[sorted.len() - 1];
            if max_page > total {
                return Err(CoreError::InvalidArgument(format!(
                    "页码 {} 超出范围(共 {} 页)",
                    max_page, total
                )));
            }
            sorted
        }
    };

    let total_selected = selected.len() as u32;
    let mut results = Vec::with_capacity(selected.len());
    for (idx, &page_num) in selected.iter().enumerate() {
        let page = document.pages().get((page_num - 1) as PdfPageIndex)?;
        let cfg = render_config(&page, dpi);
        let img = page
            .render_with_config(&cfg)
            .map_err(|e| CoreError::Pdfium(format!("渲染第 {} 页失败: {}", page_num, e)))?
            .as_image()?;
        // 文件名页码使用原始 PDF 页码,按页导出时不重新编号
        let out_path = out_dir.join(format!("page-{:04}.{}", page_num, ext));
        img.save_with_format(&out_path, fmt)
            .map_err(|e| CoreError::Image(format!("保存图片失败: {e}")))?;
        results.push(out_path);
        progress(idx as u32 + 1, total_selected);
    }
    Ok(results)
}

fn render_config(page: &PdfPage, dpi: u32) -> PdfRenderConfig {
    let width_pt = page.width().value;
    let height_pt = page.height().value;
    let scale = dpi as f32 / 72.0;
    PdfRenderConfig::new()
        .set_target_width((width_pt * scale) as Pixels)
        .set_maximum_height((height_pt * scale) as Pixels)
        .set_clear_color(PdfColor::WHITE)
}
