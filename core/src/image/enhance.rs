//! 图像增强与文档扫描(纯 Rust 实现)
//!
//! 扫描增强流水线(不依赖 OpenCV/Python):
//! 1. **文档边界检测**:降采样 → 灰度 → 高斯模糊 → Sobel 梯度 → Otsu 阈值 →
//!    膨胀连通 → 最大连通域 → 极值角点法提取四边形
//! 2. **透视矫正**:DLT 求单应矩阵(目标矩形 → 源四边形),双线性采样回采
//! 3. **去阴影**:平坦场校正(大 σ 模糊估计光照背景,逐通道除法归一)
//! 4. **增强**:百分位对比度拉伸 / 灰度 / Bradley 自适应二值化
//!
//! 另提供多图合成 PDF(JPEG DCTDecode 嵌入,页面尺寸按 dpi 换算)。
//! 基础能力(灰度/二值化/对比度)保留原有 API。

use image::imageops::FilterType;
use image::{DynamicImage, GrayImage, Luma, Rgb, RgbImage};
use lopdf::dictionary;
use std::path::{Path, PathBuf};

use crate::errors::{CoreError, Result};

// ===== 基础增强(原有 API) =====

/// 增强选项
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EnhanceOptions {
    pub grayscale: bool,
    pub binarize: bool,
    /// 二值化阈值 0-255
    pub threshold: u8,
    /// 对比度因子 1.0=原样
    pub contrast: f32,
}

impl Default for EnhanceOptions {
    fn default() -> Self {
        Self {
            grayscale: false,
            binarize: false,
            threshold: 180,
            contrast: 1.0,
        }
    }
}

/// 转灰度
pub fn grayscale(input: &Path, output: &Path) -> Result<()> {
    let img = image::open(input).map_err(|e| CoreError::Image(e.to_string()))?;
    let gray = DynamicImage::ImageLuma8(img.to_luma8());
    gray.save(output).map_err(|e| CoreError::Image(e.to_string()))?;
    Ok(())
}

/// 二值化
pub fn binarize(input: &Path, output: &Path, threshold: u8) -> Result<()> {
    let img = image::open(input).map_err(|e| CoreError::Image(e.to_string()))?;
    let gray = img.to_luma8();
    let mut out = GrayImage::new(gray.width(), gray.height());
    for (x, y, pixel) in gray.enumerate_pixels() {
        let v = if pixel[0] >= threshold { 255 } else { 0 };
        out.put_pixel(x, y, Luma([v]));
    }
    out.save(output).map_err(|e| CoreError::Image(e.to_string()))?;
    Ok(())
}

/// 调整对比度
pub fn adjust_contrast(input: &Path, output: &Path, factor: f32) -> Result<()> {
    let img = image::open(input).map_err(|e| CoreError::Image(e.to_string()))?;
    let adjusted = img.adjust_contrast(factor);
    adjusted
        .save(output)
        .map_err(|e| CoreError::Image(e.to_string()))?;
    Ok(())
}

// ===== 文档扫描增强 =====

/// 进度回调:`(current, total)`
pub type ProgressFn = dyn Fn(u32, u32) + Send + Sync;

/// 扫描增强选项
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScanOptions {
    /// 自动检测文档边界(失败时回退整图)
    pub detect_boundary: bool,
    /// 按检测到的四边形做透视矫正
    pub perspective: bool,
    /// 去阴影(平坦场光照归一)
    pub remove_shadow: bool,
    /// 输出灰度
    pub grayscale: bool,
    /// Bradley 自适应二值化(黑白扫描模式,优先于 grayscale)
    pub adaptive_binarize: bool,
    /// 百分位对比度拉伸强度 0.0-1.0(0=关闭,0.02 为常用值)
    pub contrast_stretch: f32,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            detect_boundary: true,
            perspective: true,
            remove_shadow: true,
            grayscale: false,
            adaptive_binarize: false,
            contrast_stretch: 0.02,
        }
    }
}

impl ScanOptions {
    /// 全部关闭:仅解码/转码,不做任何处理
    pub fn none() -> Self {
        Self {
            detect_boundary: false,
            perspective: false,
            remove_shadow: false,
            grayscale: false,
            adaptive_binarize: false,
            contrast_stretch: 0.0,
        }
    }
}

/// 单张扫描增强统计
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScanStats {
    /// 输出尺寸 (宽, 高)
    pub size: (u32, u32),
    /// 是否检测到文档边界
    pub boundary_detected: bool,
    /// 耗时(毫秒)
    pub elapsed_ms: u128,
}

/// 批量增强统计
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScanBatchStats {
    /// 每个输入对应的输出路径(顺序一致)
    pub outputs: Vec<PathBuf>,
    /// 检测到边界并矫正的图片数
    pub corrected: usize,
    pub elapsed_ms: u128,
}

/// 多图合成 PDF 统计
#[derive(Debug, Clone, serde::Serialize)]
pub struct ImagesToPdfStats {
    pub pages: u32,
    pub elapsed_ms: u128,
}

/// 对单张图片执行扫描增强并保存到 `output`(格式由扩展名决定)。
pub fn scan_enhance(input: &Path, output: &Path, opts: &ScanOptions) -> Result<ScanStats> {
    let start = std::time::Instant::now();
    let (img, detected) = process_to_image(input, opts)?;
    let size = (img.width(), img.height());
    img.save(output).map_err(|e| CoreError::Image(e.to_string()))?;
    Ok(ScanStats {
        size,
        boundary_detected: detected,
        elapsed_ms: start.elapsed().as_millis(),
    })
}

/// 批量扫描增强,输出文件名与输入同名,写入 `out_dir`。
///
/// `progress`: 可选 `(current, total)` 回调,用于 Tauri 事件推送。
pub fn scan_enhance_batch(
    inputs: &[PathBuf],
    out_dir: &Path,
    opts: &ScanOptions,
    progress: Option<&ProgressFn>,
) -> Result<ScanBatchStats> {
    let start = std::time::Instant::now();
    if inputs.is_empty() {
        return Err(CoreError::InvalidArgument("至少需要一张输入图片".into()));
    }
    std::fs::create_dir_all(out_dir)?;
    let total = inputs.len() as u32;
    let mut outputs = Vec::with_capacity(inputs.len());
    let mut corrected = 0usize;

    for (i, input) in inputs.iter().enumerate() {
        let (img, detected) = process_to_image(input, opts)?;
        if detected {
            corrected += 1;
        }
        let name = input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("scan");
        let output = out_dir.join(format!("{name}_scan.png"));
        img.save(&output)
            .map_err(|e| CoreError::Image(e.to_string()))?;
        outputs.push(output);
        if let Some(cb) = progress {
            cb(i as u32 + 1, total);
        }
    }

    Ok(ScanBatchStats {
        outputs,
        corrected,
        elapsed_ms: start.elapsed().as_millis(),
    })
}

/// 多张图片(可选扫描增强)合成一个 PDF,每图一页,JPEG DCTDecode 嵌入。
///
/// `dpi`: 用于像素 → PDF 点(pt)换算,页面尺寸 = 像素 × 72 / dpi。
pub fn images_to_pdf(
    inputs: &[PathBuf],
    output: &Path,
    dpi: f32,
    opts: &ScanOptions,
    progress: Option<&ProgressFn>,
) -> Result<ImagesToPdfStats> {
    let start = std::time::Instant::now();
    if inputs.is_empty() {
        return Err(CoreError::InvalidArgument("至少需要一张输入图片".into()));
    }
    if !(dpi >= 36.0 && dpi <= 1200.0) {
        return Err(CoreError::InvalidArgument(format!(
            "dpi 超出范围: {dpi}(支持 36-1200)"
        )));
    }
    let total = inputs.len() as u32;

    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let mut kids: Vec<lopdf::Object> = Vec::new();

    for (i, input) in inputs.iter().enumerate() {
        let (img, _) = process_to_image(input, opts)?;
        let rgb = img.to_rgb8();
        let (w, h) = (rgb.width(), rgb.height());

        // 编码 JPEG(DCTDecode 直接嵌入,无需额外滤波)
        let mut jpeg = Vec::new();
        let mut encoder =
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85);
        encoder
            .encode_image(&DynamicImage::ImageRgb8(rgb))
            .map_err(|e| CoreError::Image(format!("JPEG 编码失败: {e}")))?;

        let image_id = doc.add_object(lopdf::Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => i64::from(w),
                "Height" => i64::from(h),
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
                "Filter" => "DCTDecode",
            },
            jpeg,
        ));

        // 页面尺寸:像素 × 72 / dpi
        let pw = w as f32 * 72.0 / dpi;
        let ph = h as f32 * 72.0 / dpi;
        let content = format!("q {pw:.2} 0 0 {ph:.2} 0 0 cm /Im1 Do Q");
        let content_id = doc.add_object(lopdf::Stream::new(
            dictionary! {},
            content.into_bytes(),
        ));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![
                lopdf::Object::Real(0.0),
                lopdf::Object::Real(0.0),
                lopdf::Object::Real(pw),
                lopdf::Object::Real(ph),
            ],
            "Resources" => dictionary! {
                "XObject" => dictionary! { "Im1" => image_id },
            },
            "Contents" => content_id,
        });
        kids.push(page_id.into());

        if let Some(cb) = progress {
            cb(i as u32 + 1, total);
        }
    }

    let count = kids.len() as u32;
    doc.objects.insert(
        pages_id,
        lopdf::Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => count,
            "Kids" => kids,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    // 注意:不调用 compress(),JPEG 流已是 DCTDecode 压缩,二次压缩可能破坏数据
    doc.save(output)?;

    Ok(ImagesToPdfStats {
        pages: count,
        elapsed_ms: start.elapsed().as_millis(),
    })
}

// ===== 多图 A4 排版合成 PDF =====

/// A4 排版密度:每页放置几张图
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImagesPerA4 {
    /// 1 张/页(整页居中)
    One,
    /// 2 张/页(上下两栏)
    Two,
    /// 4 张/页(2×2 网格)
    Four,
}

impl Default for ImagesPerA4 {
    fn default() -> Self {
        Self::One
    }
}

impl ImagesPerA4 {
    pub fn per_page(self) -> u32 {
        match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Four => 4,
        }
    }
}

/// 页面方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PageOrientation {
    /// 纵向(默认)
    Portrait,
    /// 横向
    Landscape,
}

impl Default for PageOrientation {
    fn default() -> Self {
        Self::Portrait
    }
}

/// 图片填充模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFitMode {
    /// 等比缩放完整显示,周围留白(默认,不放大)
    Contain,
    /// 拉伸填满槽位(可能变形)
    Stretch,
    /// 等比缩放填满槽位,超出部分中心裁剪
    Cover,
}

impl Default for ImageFitMode {
    fn default() -> Self {
        Self::Contain
    }
}

/// 网格排列顺序
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GridOrder {
    /// 行优先:从左到右,再下一行(默认)
    RowMajor,
    /// 列优先:从上到下,再下一列
    ColumnMajor,
}

impl Default for GridOrder {
    fn default() -> Self {
        Self::RowMajor
    }
}

/// A4 排版选项
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImagesToA4Options {
    /// 图片扫描增强(边界检测/透视矫正/去阴影等,同 ScanOptions)
    #[serde(default)]
    pub enhance: ScanOptions,
    /// 每页几张图
    #[serde(default)]
    pub per_page: ImagesPerA4,
    /// 页边距 mm(默认 6)
    #[serde(default = "default_a4_margin_mm")]
    pub margin_mm: f32,
    /// 图片间距 mm(默认 3)
    #[serde(default = "default_a4_gap_mm")]
    pub gap_mm: f32,
    /// 渲染 DPI(默认 150;越高越清晰,文件越大)
    #[serde(default = "default_a4_dpi")]
    pub dpi: f32,
    /// 嵌入 JPEG 质量 1-100(默认 85)
    #[serde(default = "default_a4_quality")]
    pub jpeg_quality: u8,
    /// 页面方向(默认纵向)
    #[serde(default)]
    pub orientation: PageOrientation,
    /// 图片填充模式(默认等比完整显示)
    #[serde(default)]
    pub fit_mode: ImageFitMode,
    /// 网格排列顺序(默认行优先)
    #[serde(default)]
    pub grid_order: GridOrder,
}

fn default_a4_margin_mm() -> f32 {
    6.0
}
fn default_a4_gap_mm() -> f32 {
    3.0
}
fn default_a4_dpi() -> f32 {
    150.0
}
fn default_a4_quality() -> u8 {
    85
}

impl Default for ImagesToA4Options {
    fn default() -> Self {
        Self {
            enhance: ScanOptions::default(),
            per_page: ImagesPerA4::default(),
            margin_mm: default_a4_margin_mm(),
            gap_mm: default_a4_gap_mm(),
            dpi: default_a4_dpi(),
            jpeg_quality: default_a4_quality(),
            orientation: PageOrientation::default(),
            fit_mode: ImageFitMode::default(),
            grid_order: GridOrder::default(),
        }
    }
}

/// A4 页面尺寸(pt,72dpi 逻辑单位)
pub const A4_W_PT: f32 = 595.28;
pub const A4_H_PT: f32 = 841.89;

/// 多张图片按 A4 页面排版合成 PDF,可选每页 1/2/4 张图。
///
/// 排版策略:按 `per_page` 把图片分页,每页渲染成 A4 画布后以 JPEG DCTDecode 嵌入。
/// 单张图采用 contain-fit 居中(不放大,小图保留原尺寸,周围留白)。
/// 不足一页的剩余位置留空(白底)。
pub fn images_to_a4_pdf(
    inputs: &[PathBuf],
    output: &Path,
    opts: &ImagesToA4Options,
    progress: Option<&ProgressFn>,
) -> Result<ImagesToPdfStats> {
    let start = std::time::Instant::now();
    if inputs.is_empty() {
        return Err(CoreError::InvalidArgument("至少需要一张输入图片".into()));
    }
    if !(opts.dpi >= 36.0 && opts.dpi <= 1200.0) {
        return Err(CoreError::InvalidArgument(format!(
            "dpi 超出范围: {}(支持 36-1200)",
            opts.dpi
        )));
    }
    let landscape = opts.orientation == PageOrientation::Landscape;
    // 网格:(列, 行)。2 图/页纵向取上下,横向取左右
    let (cols, rows) = match opts.per_page {
        ImagesPerA4::One => (1u32, 1u32),
        ImagesPerA4::Two => {
            if landscape { (2, 1) } else { (1, 2) }
        }
        ImagesPerA4::Four => (2, 2),
    };
    let per_page = opts.per_page.per_page();
    let total = inputs.len() as u32;

    // A4 画布像素尺寸(横向时宽高互换)
    let (mm_w, mm_h) = if landscape {
        (297.0_f32, 210.0_f32)
    } else {
        (210.0_f32, 297.0_f32)
    };
    let page_w = (mm_w / 25.4 * opts.dpi).round() as u32;
    let page_h = (mm_h / 25.4 * opts.dpi).round() as u32;
    // 页面 pt 尺寸(横向互换,保证纵横比与画布一致)
    let (page_w_pt, page_h_pt) = if landscape {
        (A4_H_PT, A4_W_PT)
    } else {
        (A4_W_PT, A4_H_PT)
    };
    let px_per_mm = opts.dpi / 25.4;
    let margin = (opts.margin_mm * px_per_mm).round() as i64;
    let gap = (opts.gap_mm * px_per_mm).round() as i64;
    let slot_w = (page_w as i64 - 2 * margin - (cols as i64 - 1) * gap) / cols as i64;
    let slot_h = (page_h as i64 - 2 * margin - (rows as i64 - 1) * gap) / rows as i64;
    if slot_w < 16 || slot_h < 16 {
        return Err(CoreError::InvalidArgument(
            "页边距/间距过大,导致图片槽位过小".into(),
        ));
    }

    // 总页数 = ceil(N / per_page)
    let pages = total.div_ceil(per_page);

    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let mut kids: Vec<lopdf::Object> = Vec::new();

    for page_idx in 0..pages {
        let start_img = page_idx * per_page;
        let end_img = (start_img + per_page).min(total);

        // 白底 A4 画布
        let mut canvas = RgbImage::from_pixel(page_w, page_h, Rgb([255, 255, 255]));

        for slot_idx in 0..(end_img - start_img) {
            let img_idx = start_img + slot_idx;
            let input = &inputs[img_idx as usize];

            // 解码 + 增强
            let (img, _) = process_to_image(input, &opts.enhance)?;
            let rgb = img.to_rgb8();

            // 槽位位置(按网格排列顺序)
            let (col, row) = match opts.grid_order {
                GridOrder::RowMajor => (slot_idx % cols, slot_idx / cols),
                GridOrder::ColumnMajor => (slot_idx / rows, slot_idx % rows),
            };
            let slot_x = margin + col as i64 * (slot_w + gap);
            let slot_y = margin + row as i64 * (slot_h + gap);

            // 按填充模式把图片放置到槽位
            let sx = slot_w as f32 / rgb.width() as f32;
            let sy = slot_h as f32 / rgb.height() as f32;
            let placed: RgbImage = match opts.fit_mode {
                // 拉伸填满(可能变形)
                ImageFitMode::Stretch => image::imageops::resize(
                    &rgb,
                    slot_w as u32,
                    slot_h as u32,
                    FilterType::Lanczos3,
                ),
                // 等比完整显示,不放大,居中留白
                ImageFitMode::Contain => {
                    let scale = sx.min(sy).min(1.0);
                    if scale < 1.0 {
                        let dw = ((rgb.width() as f32 * scale).round() as u32).max(1);
                        let dh = ((rgb.height() as f32 * scale).round() as u32).max(1);
                        image::imageops::resize(&rgb, dw, dh, FilterType::Lanczos3)
                    } else {
                        rgb.clone()
                    }
                }
                // 等比填满槽位,超出部分中心裁剪
                ImageFitMode::Cover => {
                    let scale = sx.max(sy);
                    let dw = ((rgb.width() as f32 * scale).round() as u32).max(slot_w as u32);
                    let dh = ((rgb.height() as f32 * scale).round() as u32).max(slot_h as u32);
                    let big = image::imageops::resize(&rgb, dw, dh, FilterType::Lanczos3);
                    let cx = (big.width() - slot_w as u32) / 2;
                    let cy = (big.height() - slot_h as u32) / 2;
                    image::imageops::crop_imm(&big, cx, cy, slot_w as u32, slot_h as u32)
                        .to_image()
                }
            };
            // 居中放置(Stretch/Cover 恰好铺满槽位)
            let dx = slot_x + (slot_w - placed.width() as i64) / 2;
            let dy = slot_y + (slot_h - placed.height() as i64) / 2;
            image::imageops::overlay(&mut canvas, &placed, dx, dy);

            if let Some(cb) = progress {
                cb(img_idx + 1, total);
            }
        }

        // 整页编码 JPEG
        let mut jpeg = Vec::new();
        let mut encoder =
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, opts.jpeg_quality);
        encoder
            .encode_image(&DynamicImage::ImageRgb8(canvas))
            .map_err(|e| CoreError::Image(format!("JPEG 编码失败: {e}")))?;

        // 嵌入单页 A4 PDF
        let image_id = doc.add_object(lopdf::Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => i64::from(page_w),
                "Height" => i64::from(page_h),
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
                "Filter" => "DCTDecode",
            },
            jpeg,
        ));
        let content = format!("q {page_w_pt:.2} 0 0 {page_h_pt:.2} 0 0 cm /Im1 Do Q");
        let content_id = doc.add_object(lopdf::Stream::new(
            dictionary! {},
            content.into_bytes(),
        ));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![
                lopdf::Object::Real(0.0),
                lopdf::Object::Real(0.0),
                lopdf::Object::Real(page_w_pt),
                lopdf::Object::Real(page_h_pt),
            ],
            "Resources" => dictionary! {
                "XObject" => dictionary! { "Im1" => image_id },
            },
            "Contents" => content_id,
        });
        kids.push(page_id.into());
    }

    let count = kids.len() as u32;
    doc.objects.insert(
        pages_id,
        lopdf::Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => count,
            "Kids" => kids,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    // 不调用 compress():JPEG 流已是 DCTDecode,二次压缩可能破坏数据
    doc.save(output)?;

    Ok(ImagesToPdfStats {
        pages: count,
        elapsed_ms: start.elapsed().as_millis(),
    })
}

/// 核心处理:解码 → (边界检测 + 透视矫正) → 去阴影 → 灰度/二值化/拉伸。
/// 返回处理后的图像与"是否检测到文档边界"。
pub(crate) fn process_to_image(input: &Path, opts: &ScanOptions) -> Result<(DynamicImage, bool)> {
    let img = image::open(input).map_err(|e| CoreError::Image(e.to_string()))?;
    let mut rgb = img.to_rgb8();
    let mut detected = false;

    if opts.detect_boundary {
        if let Some(quad) = detect_document_quad(&rgb) {
            detected = true;
            if opts.perspective {
                rgb = warp_perspective(&rgb, &quad);
            }
        }
    }
    if opts.remove_shadow {
        rgb = remove_shadow(&rgb);
    }
    if opts.contrast_stretch > 0.0 {
        contrast_stretch(&mut rgb, opts.contrast_stretch);
    }

    let img = DynamicImage::ImageRgb8(rgb);
    if opts.adaptive_binarize {
        let gray = img.to_luma8();
        let bw = adaptive_binarize(&gray, 0.15);
        return Ok((DynamicImage::ImageLuma8(bw), detected));
    }
    if opts.grayscale {
        return Ok((DynamicImage::ImageLuma8(img.to_luma8()), detected));
    }
    Ok((img, detected))
}

// ===== 几何:四边形 =====

/// 文档四边形,顶点顺序:左上、右上、右下、左下(源图像素坐标)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quad {
    pub tl: [f32; 2],
    pub tr: [f32; 2],
    pub br: [f32; 2],
    pub bl: [f32; 2],
}

impl Quad {
    fn area(&self) -> f32 {
        let p = [
            self.tl,
            self.tr,
            self.br,
            self.bl,
        ];
        let mut a = 0.0;
        for i in 0..4 {
            let j = (i + 1) % 4;
            a += p[i][0] * p[j][1] - p[j][0] * p[i][1];
        }
        (a / 2.0).abs()
    }
}

/// 检测文档边界四边形。检测失败或区域过小则返回 `None`(调用方回退整图)。
pub fn detect_document_quad(rgb: &RgbImage) -> Option<Quad> {
    let (w, h) = (rgb.width(), rgb.height());
    if w < 32 || h < 32 {
        return None;
    }

    // 1. 降采样加速(最长边 ≤ 480)
    let scale = 480.0 / w.max(h) as f32;
    let ds_w = ((w as f32 * scale).round() as u32).max(16);
    let ds_h = ((h as f32 * scale).round() as u32).max(16);
    let small = image::imageops::resize(rgb, ds_w, ds_h, FilterType::Triangle);
    let gray = image::imageops::grayscale(&small);

    // 2. 模糊 → Sobel → Otsu 二值化 → 膨胀
    let blurred = blur_gray(&gray, 1.2);
    let edges = sobel_magnitude(&blurred);
    let t = otsu_threshold(&edges);
    let mut binary = GrayImage::new(ds_w, ds_h);
    for (x, y, p) in edges.enumerate_pixels() {
        let v = if p[0] >= t { 255 } else { 0 };
        binary.put_pixel(x, y, Luma([v]));
    }
    dilate3x3(&mut binary);
    dilate3x3(&mut binary);

    // 3. 最大连通域
    let comp = largest_component(&binary);
    if comp.len() < (ds_w * ds_h) as usize / 50 {
        return None; // 边缘过少,无明显文档轮廓
    }

    // 4. 极值角点法:TL=min(x+y) TR=max(x-y) BR=max(x+y) BL=min(x-y)
    let (mut tl, mut tr, mut br, mut bl) = (comp[0], comp[0], comp[0], comp[0]);
    let (mut s_min, mut d_max, mut s_max, mut d_min) = (f32::MAX, f32::MIN, f32::MIN, f32::MAX);
    for &[x, y] in &comp {
        let (s, d) = (x + y, x - y);
        if s < s_min {
            s_min = s;
            tl = [x, y];
        }
        if d > d_max {
            d_max = d;
            tr = [x, y];
        }
        if s > s_max {
            s_max = s;
            br = [x, y];
        }
        if d < d_min {
            d_min = d;
            bl = [x, y];
        }
    }

    // 5. 映射回原始分辨率
    let inv = 1.0 / scale;
    let up = |p: [f32; 2]| [p[0] * inv, p[1] * inv];
    let quad = Quad {
        tl: up(tl),
        tr: up(tr),
        br: up(br),
        bl: up(bl),
    };

    // 6. 面积校验:过小视为误检
    if quad.area() < 0.15 * (w * h) as f32 {
        return None;
    }
    Some(quad)
}

/// DLT 求单应矩阵:把目标矩形(矫正后)映射回源四边形,用于逆采样。
/// 目标矩形角点为像素索引极值 (0,0)..(out_w-1,out_h-1),warp 按整数坐标采样。
fn homography_rect_to_quad(quad: &Quad, out_w: f32, out_h: f32) -> [[f64; 3]; 3] {
    // 目标矩形四角与 Quad 顶点一一对应(TL TR BR BL)
    let dst = [
        [0.0f64, 0.0],
        [out_w as f64 - 1.0, 0.0],
        [out_w as f64 - 1.0, out_h as f64 - 1.0],
        [0.0, out_h as f64 - 1.0],
    ];
    let src: [[f64; 2]; 4] = [
        quad.tl.map(f64::from),
        quad.tr.map(f64::from),
        quad.br.map(f64::from),
        quad.bl.map(f64::from),
    ];

    // 8 方程 8 未知数 [h00 h01 h02 h10 h11 h12 h20 h21],h22=1
    let mut a = [[0.0f64; 9]; 8];
    for i in 0..4 {
        let (x, y) = (dst[i][0], dst[i][1]);
        let (u, v) = (src[i][0], src[i][1]);
        let r = i * 2;
        a[r] = [x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y, u];
        a[r + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y, v];
    }
    let hh = solve_linear8(&mut a);
    [
        [hh[0], hh[1], hh[2]],
        [hh[3], hh[4], hh[5]],
        [hh[6], hh[7], 1.0],
    ]
}

/// 高斯消元(部分主元)解 8×8 线性方程组,增广矩阵 8×9。
fn solve_linear8(a: &mut [[f64; 9]; 8]) -> [f64; 8] {
    let n = 8;
    for col in 0..n {
        // 选主元
        let mut pivot = col;
        for r in col + 1..n {
            if a[r][col].abs() > a[pivot][col].abs() {
                pivot = r;
            }
        }
        a.swap(col, pivot);
        let d = a[col][col];
        if d.abs() < 1e-12 {
            continue;
        }
        for r in 0..n {
            if r != col {
                let f = a[r][col] / d;
                if f != 0.0 {
                    for c in col..n + 1 {
                        a[r][c] -= f * a[col][c];
                    }
                }
            }
        }
    }
    let mut x = [0.0f64; 8];
    for r in 0..n {
        let d = a[r][r];
        x[r] = if d.abs() < 1e-12 { 0.0 } else { a[r][n] / d };
    }
    x
}

/// 透视矫正:双线性采样,出界像素填白。
pub fn warp_perspective(rgb: &RgbImage, quad: &Quad) -> RgbImage {
    // 输出尺寸 = 角点间距 + 1(角点为像素索引,边到边多覆盖一个像素)
    let out_w = (dist(quad.tl, quad.tr).max(dist(quad.bl, quad.br))).round() as u32 + 1;
    let out_h = (dist(quad.tl, quad.bl).max(dist(quad.tr, quad.br))).round() as u32 + 1;
    let hmat = homography_rect_to_quad(quad, out_w as f32, out_h as f32);
    let (w, h) = (rgb.width() as i64, rgb.height() as i64);
    let mut out = RgbImage::new(out_w, out_h);

    for y in 0..out_h {
        for x in 0..out_w {
            let px = x as f64;
            let py = y as f64;
            let sx = hmat[0][0] * px + hmat[0][1] * py + hmat[0][2];
            let sy = hmat[1][0] * px + hmat[1][1] * py + hmat[1][2];
            let sw = hmat[2][0] * px + hmat[2][1] * py + hmat[2][2];
            let (sx, sy) = if sw.abs() > 1e-9 {
                (sx / sw, sy / sw)
            } else {
                (sx, sy)
            };
            let pixel = bilinear_sample(rgb, w, h, sx, sy);
            out.put_pixel(x, y, pixel);
        }
    }
    out
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// 双线性采样,越界返回白色。
fn bilinear_sample(rgb: &RgbImage, w: i64, h: i64, x: f64, y: f64) -> Rgb<u8> {
    if x < -1.0 || y < -1.0 || x > w as f64 || y > h as f64 {
        return Rgb([255, 255, 255]);
    }
    let x0 = x.floor();
    let y0 = y.floor();
    let fx = (x - x0) as f32;
    let fy = (y - y0) as f32;
    let xi = x0 as i64;
    let yi = y0 as i64;
    let get = |cx: i64, cy: i64| -> [f32; 3] {
        if cx < 0 || cy < 0 || cx >= w || cy >= h {
            [255.0; 3]
        } else {
            let p = rgb.get_pixel(cx as u32, cy as u32);
            [p[0] as f32, p[1] as f32, p[2] as f32]
        }
    };
    let (c00, c10, c01, c11) = (
        get(xi, yi),
        get(xi + 1, yi),
        get(xi, yi + 1),
        get(xi + 1, yi + 1),
    );
    let mut out = [0u8; 3];
    for k in 0..3 {
        let top = c00[k] * (1.0 - fx) + c10[k] * fx;
        let bottom = c01[k] * (1.0 - fx) + c11[k] * fx;
        let v = top * (1.0 - fy) + bottom * fy;
        out[k] = v.round().clamp(0.0, 255.0) as u8;
    }
    Rgb(out)
}

// ===== 去阴影与增强 =====

/// 去阴影(平坦场校正):大 σ 模糊估计光照背景,逐通道除法归一。
///
/// `out = clamp(p / bg × mean(bg))`:平坦区 p/bg ≈ 1,输出恢复平均亮度,
/// 光照梯度被消除,文字等高频细节保留。建议在透视裁剪后使用(画面主体为纸面)。
pub fn remove_shadow(rgb: &RgbImage) -> RgbImage {
    let (w, h) = (rgb.width(), rgb.height());
    if w < 8 || h < 8 {
        return rgb.clone();
    }
    let sigma = (w.min(h) as f32 / 20.0).clamp(12.0, 80.0);
    let mut out = RgbImage::new(w, h);

    // 逐通道估计背景并归一
    for c in 0..3usize {
        let mut ch = GrayImage::new(w, h);
        for (x, y, p) in rgb.enumerate_pixels() {
            ch.put_pixel(x, y, Luma([p[c]]));
        }
        let bg_img = blur_gray(&ch, sigma);
        let mean_bg: f32 = bg_img.pixels().map(|p| f32::from(p[0])).sum::<f32>()
            / (w * h) as f32;

        for (x, y, p) in rgb.enumerate_pixels() {
            let bg = f32::from(bg_img.get_pixel(x, y)[0]).max(5.0);
            let v = f32::from(p.0[c]) / bg * mean_bg;
            out.get_pixel_mut(x, y).0[c] = v.round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// 百分位对比度拉伸:按 luma 直方图取 [p, 100-p] 分位映射到 [0, 255]。
pub fn contrast_stretch(rgb: &mut RgbImage, pct: f32) {
    let pct = pct.clamp(0.001, 0.25);
    let (w, h) = (rgb.width(), rgb.height());
    let total = (w * h) as usize;
    let mut hist = [0u64; 256];
    for p in rgb.pixels() {
        let luma = (0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2]))
            .round() as usize;
        hist[luma.min(255)] += 1;
    }
    let low_n = (total as f64 * pct as f64) as u64;
    let high_n = (total as f64 * (1.0 - pct) as f64) as u64;
    let (mut acc, mut low, mut high) = (0u64, 0u8, 255u8);
    for (v, &cnt) in hist.iter().enumerate() {
        acc += cnt;
        if acc <= low_n {
            low = v as u8;
        }
        if acc <= high_n {
            high = v as u8;
        }
    }
    if high.saturating_sub(low) < 10 {
        return; // 动态范围已满,无需拉伸
    }
    let lo = f32::from(low);
    let scale = 255.0 / f32::from(high - low);
    for p in rgb.pixels_mut() {
        for c in 0..3 {
            let v = (f32::from(p.0[c]) - lo) * scale;
            p.0[c] = v.round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Bradley 自适应二值化(积分图局部均值,`t` 为相对阈值偏移,常用 0.15)。
pub fn adaptive_binarize(gray: &GrayImage, t: f32) -> GrayImage {
    let (w, h) = (gray.width(), gray.height());
    let win = ((w.min(h) / 40).max(8) as i64) | 1; // 奇数窗口
    let half = (win / 2).max(1);

    // 积分图 (w+1)×(h+1)
    let mut integral = vec![0u64; (w as usize + 1) * (h as usize + 1)];
    for y in 0..h as usize {
        let mut row_sum = 0u64;
        for x in 0..w as usize {
            row_sum += u64::from(gray.get_pixel(x as u32, y as u32)[0]);
            integral[(y + 1) * (w as usize + 1) + x + 1] =
                integral[y * (w as usize + 1) + x + 1] + row_sum;
        }
    }

    let mut out = GrayImage::new(w, h);
    for y in 0..h as i64 {
        let y1 = (y - half).clamp(0, h as i64 - 1) as usize;
        let y2 = (y + half).clamp(0, h as i64 - 1) as usize;
        for x in 0..w as i64 {
            let x1 = (x - half).clamp(0, w as i64 - 1) as usize;
            let x2 = (x + half).clamp(0, w as i64 - 1) as usize;
            let count = ((y2 - y1 + 1) * (x2 - x1 + 1)) as f64;
            // 矩形 [x1..=x2, y1..=y2] 的积分图区间和(用 i64 避免中间量下溢)
            let sum = (integral[(y2 + 1) * (w as usize + 1) + x2 + 1] as i64)
                - (integral[y1 * (w as usize + 1) + x2 + 1] as i64)
                - (integral[(y2 + 1) * (w as usize + 1) + x1] as i64)
                + (integral[y1 * (w as usize + 1) + x1] as i64);
            let mean = sum as f64 / count;
            let v = f64::from(gray.get_pixel(x as u32, y as u32)[0]);
            let bw = if v < mean * (1.0 - t as f64) { 0 } else { 255 };
            out.put_pixel(x as u32, y as u32, Luma([bw]));
        }
    }
    out
}

// ===== 图像处理原语 =====

/// 灰度高斯模糊(`imageops::blur` 的空 σ 防护封装)
fn blur_gray(gray: &GrayImage, sigma: f32) -> GrayImage {
    if sigma <= 0.01 {
        return gray.clone();
    }
    image::imageops::blur(gray, sigma)
}

/// Sobel 梯度幅值(边界置 0,避免图像外框产生假边缘)
fn sobel_magnitude(gray: &GrayImage) -> GrayImage {
    let (w, h) = (gray.width(), gray.height());
    let mut out = GrayImage::new(w, h);
    let at = |x: i64, y: i64| -> f32 {
        if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
            0.0
        } else {
            f32::from(gray.get_pixel(x as u32, y as u32)[0])
        }
    };
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            if x == 0 || y == 0 || x == w as i64 - 1 || y == h as i64 - 1 {
                continue; // 边界置 0
            }
            let gx = -at(x - 1, y - 1) - 2.0 * at(x - 1, y) - at(x - 1, y + 1)
                + at(x + 1, y - 1)
                + 2.0 * at(x + 1, y)
                + at(x + 1, y + 1);
            let gy = -at(x - 1, y - 1) - 2.0 * at(x, y - 1) - at(x + 1, y - 1)
                + at(x - 1, y + 1)
                + 2.0 * at(x, y + 1)
                + at(x + 1, y + 1);
            let m = (gx * gx + gy * gy).sqrt().round().clamp(0.0, 255.0) as u8;
            out.put_pixel(x as u32, y as u32, Luma([m]));
        }
    }
    out
}

/// Otsu 全局阈值
fn otsu_threshold(gray: &GrayImage) -> u8 {
    let mut hist = [0u64; 256];
    for p in gray.pixels() {
        hist[p[0] as usize] += 1;
    }
    let total: u64 = hist.iter().sum();
    if total == 0 {
        return 128;
    }
    let mut sum_all = 0u64;
    for (v, &cnt) in hist.iter().enumerate() {
        sum_all += v as u64 * cnt;
    }
    let (mut sum_b, mut w_b, mut best, mut threshold) = (0u64, 0u64, 0.0f64, 128u8);
    for (v, &cnt) in hist.iter().enumerate() {
        w_b += cnt;
        if w_b == 0 {
            continue;
        }
        let w_f = (total - w_b) as f64;
        if w_f == 0.0 {
            break;
        }
        sum_b += v as u64 * cnt;
        let m_b = sum_b as f64 / w_b as f64;
        let m_f = (sum_all - sum_b) as f64 / w_f;
        let between = (w_b as f64) * w_f * (m_b - m_f) * (m_b - m_f);
        if between > best {
            best = between;
            threshold = v as u8;
        }
    }
    threshold
}

/// 3×3 最大值膨胀(连接断裂边缘)
fn dilate3x3(img: &mut GrayImage) {
    let (w, h) = (img.width(), img.height());
    let src = img.clone();
    for y in 0..h {
        for x in 0..w {
            let mut m = 0u8;
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    let nx = x as i64 + dx;
                    let ny = y as i64 + dy;
                    if nx >= 0 && ny >= 0 && nx < w as i64 && ny < h as i64 {
                        m = m.max(src.get_pixel(nx as u32, ny as u32)[0]);
                    }
                }
            }
            img.put_pixel(x, y, Luma([m]));
        }
    }
}

/// 最大 8 连通域像素集合(降采样坐标系,子像素精度)
fn largest_component(binary: &GrayImage) -> Vec<[f32; 2]> {
    let (w, h) = (binary.width() as usize, binary.height() as usize);
    let mut labels = vec![0u32; w * h];
    let mut best_label = 0u32;
    let mut best_count = 0usize;
    let mut current = 0u32;
    let mut stack: Vec<usize> = Vec::new();

    let idx = |x: usize, y: usize| y * w + x;
    for start in 0..w * h {
        if labels[start] != 0 {
            continue;
        }
        let px = binary.get_pixel((start % w) as u32, (start / w) as u32)[0];
        if px < 128 {
            continue;
        }
        current += 1;
        labels[start] = current;
        stack.clear();
        stack.push(start);
        let mut count = 0usize;
        while let Some(i) = stack.pop() {
            count += 1;
            let x = i % w;
            let y = i / w;
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let nx = x as i64 + dx;
                    let ny = y as i64 + dy;
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                        continue;
                    }
                    let ni = idx(nx as usize, ny as usize);
                    if labels[ni] == 0
                        && binary.get_pixel(nx as u32, ny as u32)[0] >= 128
                    {
                        labels[ni] = current;
                        stack.push(ni);
                    }
                }
            }
        }
        if count > best_count {
            best_count = count;
            best_label = current;
        }
    }

    // 收集最大连通域像素(带 0.5 亚像素偏移对齐像素中心)
    let mut pts = Vec::with_capacity(best_count);
    if best_label > 0 {
        for y in 0..h {
            for x in 0..w {
                if labels[idx(x, y)] == best_label {
                    pts.push([x as f32 + 0.5, y as f32 + 0.5]);
                }
            }
        }
    }
    pts
}

// ===== 测试 =====

#[cfg(test)]
mod tests {
    use super::*;

    /// 生成合成"拍照文档":深色背景 + 白色倾斜纸张 + 阴影渐变
    fn make_photo_doc(w: u32, h: u32) -> RgbImage {
        let mut img = RgbImage::new(w, h);
        // 纸张四边形(约占 60% 面积,轻微倾斜)
        let paper = Quad {
            tl: [0.18 * w as f32, 0.15 * h as f32],
            tr: [0.85 * w as f32, 0.20 * h as f32],
            br: [0.80 * w as f32, 0.85 * h as f32],
            bl: [0.22 * w as f32, 0.80 * h as f32],
        };
        for y in 0..h {
            for x in 0..w {
                let inside = point_in_quad(&paper, x as f32, y as f32);
                // 背景深灰,纸面亮白带阴影(左上暗角)
                let shadow = 1.0 - 0.45 * (1.0 - x as f32 / w as f32);
                let v = if inside { (235.0 * shadow) as u8 } else { 40 };
                img.put_pixel(x, y, Rgb([v, v, v]));
            }
        }
        img
    }

    fn point_in_quad(q: &Quad, x: f32, y: f32) -> bool {
        let p = [q.tl, q.tr, q.br, q.bl];
        let mut sign = 0.0f32;
        for i in 0..4 {
            let j = (i + 1) % 4;
            let cross =
                (p[j][0] - p[i][0]) * (y - p[i][1]) - (p[j][1] - p[i][1]) * (x - p[i][0]);
            if cross != 0.0 {
                let s = cross.signum();
                if sign == 0.0 {
                    sign = s;
                } else if s != sign {
                    return false;
                }
            }
        }
        true
    }

    #[test]
    fn otsu_on_bimodal() {
        let mut img = GrayImage::new(100, 100);
        for (x, _y, p) in img.enumerate_pixels_mut() {
            *p = if x < 50 { Luma([30]) } else { Luma([220]) };
        }
        let t = otsu_threshold(&img);
        // 完美双峰分布在两峰间任意阈值方差相同,取首个等值点即可
        assert!((30..=220).contains(&t), "Otsu 阈值异常: {t}");
    }

    #[test]
    fn detect_quad_on_synthetic_photo() {
        let photo = make_photo_doc(800, 600);
        let quad = detect_document_quad(&photo).expect("应检测到文档边界");
        // 检测角点接近真实角点(降采样精度,允许 8% 误差)
        let w = 800.0;
        let h = 600.0;
        let check = |got: [f32; 2], exp: [f32; 2]| {
            let dx = (got[0] - exp[0]).abs() / w;
            let dy = (got[1] - exp[1]).abs() / h;
            assert!(
                dx < 0.08 && dy < 0.08,
                "角点偏差过大: got {got:?}, exp {exp:?}"
            );
        };
        check(quad.tl, [0.18 * w, 0.15 * h]);
        check(quad.tr, [0.85 * w, 0.20 * h]);
        check(quad.br, [0.80 * w, 0.85 * h]);
        check(quad.bl, [0.22 * w, 0.80 * h]);
    }

    #[test]
    fn detect_quad_fallback_on_flat_image() {
        // 均匀图像无边缘 → 回退 None
        let img = RgbImage::from_pixel(400, 300, Rgb([128, 128, 128]));
        assert!(detect_document_quad(&img).is_none());
    }

    #[test]
    fn warp_axis_aligned_quad_is_identity() {
        // 轴对齐全幅四边形,矫正后应≈原图
        let mut img = RgbImage::new(100, 80);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = Rgb([(x % 255) as u8, (y % 255) as u8, 128]);
        }
        let quad = Quad {
            tl: [0.0, 0.0],
            tr: [99.0, 0.0],
            br: [99.0, 79.0],
            bl: [0.0, 79.0],
        };
        let warped = warp_perspective(&img, &quad);
        assert_eq!(warped.dimensions(), (100, 80));
        for (x, y, p) in warped.enumerate_pixels() {
            let orig = img.get_pixel(x, y);
            let diff = |a: u8, b: u8| (i16::from(a) - i16::from(b)).unsigned_abs();
            assert!(diff(p[0], orig[0]) <= 2 && diff(p[1], orig[1]) <= 2, "({x},{y}) 偏差过大");
        }
    }

    #[test]
    fn remove_shadow_flattens_illumination() {
        // 全幅纸面 + 水平光照渐变(0.55 → 1.0 × 235)
        let (w, h) = (400u32, 300u32);
        let mut img = RgbImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let v = (235.0 * (0.55 + 0.45 * x as f32 / w as f32)).round() as u8;
                img.put_pixel(x, y, Rgb([v, v, v]));
            }
        }
        let before =
            (mean_luma_region(&img, 0.0, 0.0, 0.25, 1.0) - mean_luma_region(&img, 0.75, 0.0, 1.0, 1.0)).abs();
        let out = remove_shadow(&img);
        let after =
            (mean_luma_region(&out, 0.0, 0.0, 0.25, 1.0) - mean_luma_region(&out, 0.75, 0.0, 1.0, 1.0)).abs();
        assert!(before > 60.0, "光照渐变应显著: {before}");
        assert!(after < 15.0, "去阴影后亮度应均匀: diff={after}");
    }

    fn mean_luma_region(img: &RgbImage, x0: f32, y0: f32, x1: f32, y1: f32) -> f32 {
        let (w, h) = (img.width(), img.height());
        let (xa, xb) = ((x0 * w as f32) as u32, (x1 * w as f32) as u32);
        let (ya, yb) = ((y0 * h as f32) as u32, (y1 * h as f32) as u32);
        let mut sum = 0.0f64;
        let mut n = 0u64;
        for y in ya..yb {
            for x in xa..xb {
                let p = img.get_pixel(x, y);
                sum += f64::from(p[0]);
                n += 1;
            }
        }
        if n == 0 {
            0.0
        } else {
            (sum / n as f64) as f32
        }
    }

    #[test]
    fn adaptive_binarize_separates_text() {
        // 白底 + 十字细线(4px):窗口以背景为主,验证局部阈值
        let mut img = GrayImage::from_pixel(200, 200, Luma([230]));
        for y in 0..200 {
            for x in 98..102 {
                img.put_pixel(x, y, Luma([20]));
            }
        }
        for y in 98..102 {
            for x in 0..200 {
                img.put_pixel(x, y, Luma([20]));
            }
        }
        let bw = adaptive_binarize(&img, 0.15);
        assert_eq!(bw.get_pixel(10, 10)[0], 255); // 背景白
        assert_eq!(bw.get_pixel(100, 50)[0], 0); // 竖线黑
        assert_eq!(bw.get_pixel(50, 100)[0], 0); // 横线黑
    }

    #[test]
    fn scan_enhance_end_to_end() {
        let tmp = tempfile::tempdir().unwrap();
        let input = tmp.path().join("photo.png");
        let output = tmp.path().join("out.jpg");
        DynamicImage::ImageRgb8(make_photo_doc(640, 480))
            .save(&input)
            .unwrap();
        let stats = scan_enhance(&input, &output, &ScanOptions::default()).unwrap();
        assert!(output.exists());
        assert!(stats.boundary_detected, "合成样张应检测到边界");
        let out = image::open(&output).unwrap();
        assert!(out.width() > 100 && out.height() > 100);
    }

    #[test]
    fn images_to_pdf_end_to_end() {
        let tmp = tempfile::tempdir().unwrap();
        let mut inputs = Vec::new();
        for i in 0..2 {
            let p = tmp.path().join(format!("p{i}.png"));
            DynamicImage::ImageRgb8(make_photo_doc(600, 450))
                .save(&p)
                .unwrap();
            inputs.push(p);
        }
        let output = tmp.path().join("scan.pdf");
        let stats = images_to_pdf(&inputs, &output, 150.0, &ScanOptions::default(), None).unwrap();
        assert_eq!(stats.pages, 2);
        let doc = lopdf::Document::load(&output).unwrap();
        assert_eq!(doc.get_pages().len(), 2);
    }

    #[test]
    fn images_to_pdf_rejects_bad_dpi() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("a.png");
        DynamicImage::ImageRgb8(make_photo_doc(100, 100))
            .save(&p)
            .unwrap();
        let out = tmp.path().join("x.pdf");
        assert!(images_to_pdf(&[p], &out, 10.0, &ScanOptions::none(), None).is_err());
    }

    /// 生成固定尺寸纯色测试图(不同颜色便于区分)
    fn make_color_img(w: u32, h: u32, color: u8) -> RgbImage {
        RgbImage::from_pixel(
            w,
            h,
            Rgb([color, color.wrapping_add(80), color.wrapping_add(160)]),
        )
    }

    #[test]
    fn images_to_a4_pdf_one_per_page() {
        // 3 张图,1 张/页 → 3 页
        let tmp = tempfile::tempdir().unwrap();
        let mut inputs = Vec::new();
        for i in 0..3 {
            let p = tmp.path().join(format!("p{i}.png"));
            DynamicImage::ImageRgb8(make_color_img(800, 600, 100))
                .save(&p)
                .unwrap();
            inputs.push(p);
        }
        let out = tmp.path().join("a4-one.pdf");
        let stats = images_to_a4_pdf(
            &inputs,
            &out,
            &ImagesToA4Options {
                per_page: ImagesPerA4::One,
                dpi: 96.0,
                ..Default::default()
            },
            None,
        )
        .unwrap();
        assert_eq!(stats.pages, 3);
        let doc = lopdf::Document::load(&out).unwrap();
        assert_eq!(doc.get_pages().len(), 3);
    }

    #[test]
    fn images_to_a4_pdf_two_per_page() {
        // 5 张图,2 张/页 → 3 页(最后一页 1 张)
        let tmp = tempfile::tempdir().unwrap();
        let mut inputs = Vec::new();
        for i in 0..5 {
            let p = tmp.path().join(format!("p{i}.png"));
            DynamicImage::ImageRgb8(make_color_img(400, 300, 50))
                .save(&p)
                .unwrap();
            inputs.push(p);
        }
        let out = tmp.path().join("a4-two.pdf");
        let stats = images_to_a4_pdf(
            &inputs,
            &out,
            &ImagesToA4Options {
                per_page: ImagesPerA4::Two,
                dpi: 96.0,
                ..Default::default()
            },
            None,
        )
        .unwrap();
        // 5 张 ÷ 2/页 = 3 页
        assert_eq!(stats.pages, 3);
        let doc = lopdf::Document::load(&out).unwrap();
        assert_eq!(doc.get_pages().len(), 3);
    }

    #[test]
    fn images_to_a4_pdf_four_per_page() {
        // 6 张图,4 张/页 → 2 页(第二页 2 张)
        let tmp = tempfile::tempdir().unwrap();
        let mut inputs = Vec::new();
        for i in 0..6 {
            let p = tmp.path().join(format!("p{i}.png"));
            DynamicImage::ImageRgb8(make_color_img(300, 200, 80))
                .save(&p)
                .unwrap();
            inputs.push(p);
        }
        let out = tmp.path().join("a4-four.pdf");
        let stats = images_to_a4_pdf(
            &inputs,
            &out,
            &ImagesToA4Options {
                per_page: ImagesPerA4::Four,
                dpi: 96.0,
                ..Default::default()
            },
            None,
        )
        .unwrap();
        // 6 张 ÷ 4/页 = 2 页
        assert_eq!(stats.pages, 2);
        let doc = lopdf::Document::load(&out).unwrap();
        assert_eq!(doc.get_pages().len(), 2);
    }

    #[test]
    fn images_to_a4_pdf_rejects_bad_dpi() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("a.png");
        DynamicImage::ImageRgb8(make_color_img(100, 100, 100))
            .save(&p)
            .unwrap();
        let out = tmp.path().join("x.pdf");
        assert!(images_to_a4_pdf(
            &[p],
            &out,
            &ImagesToA4Options {
                dpi: 10.0,
                ..Default::default()
            },
            None
        )
        .is_err());
    }

    #[test]
    fn images_to_a4_pdf_progress_callback() {
        let tmp = tempfile::tempdir().unwrap();
        let mut inputs = Vec::new();
        for i in 0..3 {
            let p = tmp.path().join(format!("p{i}.png"));
            DynamicImage::ImageRgb8(make_color_img(200, 150, 60))
                .save(&p)
                .unwrap();
            inputs.push(p);
        }
        let out = tmp.path().join("progress.pdf");
        let last = std::sync::Arc::new(std::sync::Mutex::new((0u32, 0u32)));
        let last2 = last.clone();
        let cb = move |c: u32, t: u32| {
            *last2.lock().unwrap() = (c, t);
        };
        let stats = images_to_a4_pdf(
            &inputs,
            &out,
            &ImagesToA4Options {
                per_page: ImagesPerA4::Two,
                dpi: 96.0,
                ..Default::default()
            },
            Some(&cb),
        )
        .unwrap();
        assert_eq!(stats.pages, 2);
        let (c, t) = *last.lock().unwrap();
        assert_eq!(c, 3);
        assert_eq!(t, 3);
    }

    /// 生成指定 RGB 纯色图(布局测试用)
    fn make_solid(w: u32, h: u32, c: [u8; 3]) -> RgbImage {
        RgbImage::from_pixel(w, h, Rgb(c))
    }

    /// 颜色逐通道比较(JPEG 有损,允许小误差)
    fn colors_near(a: [u8; 3], b: [u8; 3]) -> bool {
        a.iter()
            .zip(b.iter())
            .all(|(x, y)| (i16::from(*x) - i16::from(*y)).abs() < 16)
    }

    /// 提取 PDF 中第一个嵌入 JPEG 并解码(测试辅助)
    fn first_embedded_image(path: &Path) -> RgbImage {
        let doc = lopdf::Document::load(path).unwrap();
        let jpeg = doc
            .objects
            .values()
            .find_map(|o| match o {
                lopdf::Object::Stream(s) if s.content.starts_with(&[0xFF, 0xD8]) => {
                    Some(s.content.clone())
                }
                _ => None,
            })
            .expect("PDF 应包含 JPEG 图像流");
        image::load_from_memory(&jpeg).unwrap().to_rgb8()
    }

    #[test]
    fn images_to_a4_pdf_orientation_landscape() {
        // 横向 A4:嵌入图像应宽大于高,2 图/页为左右并排
        let tmp = tempfile::tempdir().unwrap();
        let mut inputs = Vec::new();
        for i in 0..2 {
            let p = tmp.path().join(format!("p{i}.png"));
            DynamicImage::ImageRgb8(make_color_img(400, 300, 60))
                .save(&p)
                .unwrap();
            inputs.push(p);
        }
        let out = tmp.path().join("landscape.pdf");
        let stats = images_to_a4_pdf(
            &inputs,
            &out,
            &ImagesToA4Options {
                per_page: ImagesPerA4::Two,
                orientation: PageOrientation::Landscape,
                dpi: 96.0,
                ..Default::default()
            },
            None,
        )
        .unwrap();
        assert_eq!(stats.pages, 1);
        let img = first_embedded_image(&out);
        assert!(
            img.width() > img.height(),
            "横向页面画布应宽大于高: {}x{}",
            img.width(),
            img.height()
        );
    }

    #[test]
    fn images_to_a4_pdf_column_order() {
        // 4 张纯色图,列优先:左上红、左下绿、右上蓝、右下黄
        let tmp = tempfile::tempdir().unwrap();
        let palette: [[u8; 3]; 4] = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0]];
        let mut inputs = Vec::new();
        for (i, c) in palette.iter().enumerate() {
            let p = tmp.path().join(format!("p{i}.png"));
            DynamicImage::ImageRgb8(make_solid(300, 200, *c))
                .save(&p)
                .unwrap();
            inputs.push(p);
        }
        let out = tmp.path().join("order.pdf");
        images_to_a4_pdf(
            &inputs,
            &out,
            &ImagesToA4Options {
                per_page: ImagesPerA4::Four,
                grid_order: GridOrder::ColumnMajor,
                dpi: 96.0,
                enhance: ScanOptions::none(),
                ..Default::default()
            },
            None,
        )
        .unwrap();
        let img = first_embedded_image(&out);
        let (w, h) = (img.width(), img.height());
        let sample = |x: u32, y: u32| -> [u8; 3] {
            let p = img.get_pixel(x, y);
            [p[0], p[1], p[2]]
        };
        // 列优先:先填满左列(上→下),再填右列
        assert!(colors_near(sample(w / 4, h / 4), palette[0]), "左上应为红");
        assert!(
            colors_near(sample(w / 4, h * 3 / 4), palette[1]),
            "左下应为绿"
        );
        assert!(colors_near(sample(w * 3 / 4, h / 4), palette[2]), "右上应为蓝");
        assert!(
            colors_near(sample(w * 3 / 4, h * 3 / 4), palette[3]),
            "右下应为黄"
        );
    }

    #[test]
    fn images_to_a4_pdf_stretch_fills_page() {
        // 拉伸模式:单图应铺满整页(角落与中心同色)
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("p.png");
        DynamicImage::ImageRgb8(make_solid(100, 80, [200, 30, 30]))
            .save(&p)
            .unwrap();
        let out = tmp.path().join("stretch.pdf");
        images_to_a4_pdf(
            &[p],
            &out,
            &ImagesToA4Options {
                per_page: ImagesPerA4::One,
                fit_mode: ImageFitMode::Stretch,
                dpi: 96.0,
                enhance: ScanOptions::none(),
                ..Default::default()
            },
            None,
        )
        .unwrap();
        let img = first_embedded_image(&out);
        let (w, h) = (img.width(), img.height());
        // 取样点避开 10mm(≈38px)页边距
        for (x, y) in [(50u32, 50u32), (w / 2, h / 2), (w - 50, h - 50)] {
            let px = img.get_pixel(x, y);
            assert!(
                colors_near([px[0], px[1], px[2]], [200, 30, 30]),
                "({x},{y}) 应为拉伸后的纯色"
            );
        }
    }
}
