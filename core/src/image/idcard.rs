//! 证件正反面排版 —— 两张证件照(可选扫描增强)合成单页 A4 PDF。
//!
//! 排版:上下两栏,每栏 = 图片(contain-fit 居中) + 下方标签文字;
//! 标签经系统 CJK 字体栅格化到画布(找不到字体时自动跳过标签,仅排版图片)。
//! 整页合成后以 JPEG DCTDecode 嵌入单页 A4 PDF。

use image::imageops::FilterType;
use image::{DynamicImage, Rgb, RgbImage};
use lopdf::dictionary; // 注意:lopdf 宏只能 use 后使用,`dictionary!{}` 路径形式不可用
use std::path::{Path, PathBuf};

use crate::{CoreError, Result};

use super::enhance::{process_to_image, ScanOptions};

/// 排版渲染 DPI(A4 300dpi = 2480x3508 像素,打印清晰度)
const LAYOUT_DPI: f32 = 300.0;
/// A4 页面尺寸(pt,72dpi 逻辑单位)
pub const A4_W_PT: f32 = 595.28;
pub const A4_H_PT: f32 = 841.89;

/// 证件排版选项
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IdCardOptions {
    /// 证件扫描增强(边界检测/透视矫正/去阴影等,同 ScanOptions)
    #[serde(default)]
    pub enhance: ScanOptions,
    /// 页边距 mm(默认 10)
    #[serde(default = "default_margin_mm")]
    pub margin_mm: f32,
    /// 两栏间距 mm(默认 8)
    #[serde(default = "default_gap_mm")]
    pub gap_mm: f32,
    /// 正面(第一张图)标签,空字符串不绘制
    #[serde(default = "default_label_front")]
    pub label_front: String,
    /// 反面(第二张图)标签
    #[serde(default = "default_label_back")]
    pub label_back: String,
    /// 标签字号 pt(默认 12)
    #[serde(default = "default_label_size")]
    pub label_size_pt: f32,
    /// 嵌入 JPEG 质量 1-100(默认 90)
    #[serde(default = "default_quality")]
    pub jpeg_quality: u8,
}

fn default_margin_mm() -> f32 {
    10.0
}
fn default_gap_mm() -> f32 {
    8.0
}
fn default_label_front() -> String {
    "人像面".into()
}
fn default_label_back() -> String {
    "国徽面".into()
}
fn default_label_size() -> f32 {
    12.0
}
fn default_quality() -> u8 {
    90
}

impl Default for IdCardOptions {
    fn default() -> Self {
        Self {
            enhance: ScanOptions::default(),
            margin_mm: default_margin_mm(),
            gap_mm: default_gap_mm(),
            label_front: default_label_front(),
            label_back: default_label_back(),
            label_size_pt: default_label_size(),
            jpeg_quality: default_quality(),
        }
    }
}

/// 排版统计
#[derive(Debug, Clone, serde::Serialize)]
pub struct IdCardStats {
    pub elapsed_ms: u128,
    /// 标签是否绘制成功(false = 未找到 CJK 字体,已跳过)
    pub labeled: bool,
    /// 单位 mm 的有效图片区域(上下栏相同,便于前端展示)
    pub slot_size_mm: [f32; 2],
}

/// 探测可用的 CJK 字体(环境变量 PDFDOCVAULT_FONT 优先)。
fn find_cjk_font() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("PDFDOCVAULT_FONT") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let candidates = [
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/noto/NotoSerifCJK-Regular.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        "/usr/share/fonts/wqy-microhei/wqy-microhei.ttc",
        "C:/Windows/Fonts/msyh.ttc",
        "C:/Windows/Fonts/simhei.ttf",
        "/System/Library/Fonts/PingFang.ttc",
    ];
    candidates.iter().map(PathBuf::from).find(|p| p.is_file())
}

/// 在画布 (cx, top) 顶部水平居中绘制一行文字,返回是否成功。
///
/// 使用 ab_glyph 直接栅格化(imageproc 0.25.1 已移除 text 模块,故自绘),
/// 白底黑字 alpha 混合。
fn draw_label_centered(
    canvas: &mut RgbImage,
    text: &str,
    cx: i64,
    top: i64,
    px_size: f32,
    font: &ab_glyph::FontArc,
) -> bool {
    // into_scaled 在 Font trait;其余方法在 ScaleFont trait
    use ab_glyph::{Font as _, ScaleFont as _};

    let pf = font.clone().into_scaled(ab_glyph::PxScale::from(px_size));

    // 逐字布置字形并累计总宽
    let mut glyphs: Vec<ab_glyph::Glyph> = Vec::with_capacity(text.chars().count());
    let mut width = 0.0f32;
    for c in text.chars() {
        let mut g = pf.scaled_glyph(c);
        g.position = ab_glyph::point(width, 0.0);
        width += pf.h_advance(g.id);
        glyphs.push(g);
    }
    if width <= 0.0 {
        return false;
    }
    let start_x = cx as f32 - width / 2.0;
    if start_x < 0.0 {
        return false;
    }
    let pen_y = top as f32 + pf.ascent();

    for g in glyphs {
        let mut g = g;
        g.position = ab_glyph::point(start_x + g.position.x, pen_y);
        if let Some(og) = pf.outline_glyph(g) {
            let min = og.px_bounds().min;
            og.draw(|x, y, alpha| {
                let px = min.x as i64 + x as i64;
                let py = min.y as i64 + y as i64;
                if px < 0 || py < 0 || px >= canvas.width() as i64 || py >= canvas.height() as i64 {
                    return;
                }
                // 白底黑字(20,20,20) alpha 混合
                let p = canvas.get_pixel_mut(px as u32, py as u32);
                let blend =
                    |c: u8| -> u8 { (f32::from(c) * (1.0 - alpha) + 20.0 * alpha).round() as u8 };
                *p = Rgb([blend(p[0]), blend(p[1]), blend(p[2])]);
            });
        }
    }
    true
}

/// 两张证件照(正面/反面)排版合成单页 A4 PDF。
///
/// 输入顺序:第 1 张 = 正面(上栏),第 2 张 = 反面(下栏)。
pub fn id_cards_to_pdf(
    inputs: &[PathBuf],
    output: &Path,
    opts: &IdCardOptions,
    progress: Option<&super::enhance::ProgressFn>,
) -> Result<IdCardStats> {
    let start = std::time::Instant::now();
    if inputs.len() != 2 {
        return Err(CoreError::InvalidArgument(format!(
            "证件排版需要恰好 2 张图片(正面/反面),收到 {} 张",
            inputs.len()
        )));
    }
    let notify = |cur: u32, total: u32| {
        if let Some(cb) = progress {
            cb(cur, total);
        }
    };

    // 1. 增强/解码两张图
    let mut imgs: Vec<RgbImage> = Vec::with_capacity(2);
    for (i, input) in inputs.iter().enumerate() {
        let (img, _) = process_to_image(input, &opts.enhance)?;
        imgs.push(img.to_rgb8());
        notify(i as u32 + 1, 4);
    }

    // 2. A4 画布与槽位几何
    let page_w = (210.0 / 25.4 * LAYOUT_DPI).round() as u32; // 2480
    let page_h = (297.0 / 25.4 * LAYOUT_DPI).round() as u32; // 3508
    let px_per_mm = LAYOUT_DPI / 25.4;
    let margin = (opts.margin_mm * px_per_mm).round() as i64;
    let gap = (opts.gap_mm * px_per_mm).round() as i64;
    let label_px = (opts.label_size_pt * LAYOUT_DPI / 72.0).ceil() as i64;
    let label_h = label_px * 2; // 字号 2 倍作标签区高度(含行距)
    let slot_w = page_w as i64 - 2 * margin;
    let slot_h = (page_h as i64 - 2 * margin - gap - 2 * label_h) / 2;

    let mut canvas = RgbImage::from_pixel(page_w, page_h, Rgb([255, 255, 255]));

    // 3. 字体(可选)
    let font = find_cjk_font().and_then(|p| {
        std::fs::read(&p)
            .ok()
            .and_then(|bytes| ab_glyph::FontArc::try_from_vec(bytes).ok())
    });
    let mut labeled = false;

    // 4. 逐栏放置图片 + 标签
    let labels = [opts.label_front.as_str(), opts.label_back.as_str()];
    for (i, img) in imgs.iter().enumerate() {
        let slot_top = margin + i as i64 * (slot_h + label_h + gap);
        // contain-fit 居中
        let scale = (slot_w as f32 / img.width() as f32)
            .min(slot_h as f32 / img.height() as f32)
            .min(1.0);
        let dw = (img.width() as f32 * scale).round() as u32;
        let dh = (img.height() as f32 * scale).round() as u32;
        let resized = if scale < 1.0 {
            image::imageops::resize(img, dw, dh, FilterType::Lanczos3)
        } else {
            img.clone()
        };
        let dx = margin + (slot_w - dw as i64) / 2;
        let dy = slot_top + (slot_h - dh as i64) / 2;
        image::imageops::overlay(&mut canvas, &resized, dx, dy);

        // 标签画在栏下方
        if !labels[i].is_empty() {
            if let Some(f) = &font {
                if draw_label_centered(
                    &mut canvas,
                    labels[i],
                    page_w as i64 / 2,
                    slot_top + slot_h,
                    label_px as f32,
                    f,
                ) {
                    labeled = true;
                }
            }
        }
        notify(i as u32 + 3, 4);
    }

    // 5. 编码 JPEG 并嵌入单页 A4 PDF(整页铺满)
    let mut jpeg = Vec::new();
    let mut encoder =
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, opts.jpeg_quality);
    encoder
        .encode_image(&DynamicImage::ImageRgb8(canvas))
        .map_err(|e| CoreError::Image(format!("JPEG 编码失败: {e}")))?;
    notify(4, 4);

    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();
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
    let content = format!(
        "q {A4_W_PT:.2} 0 0 {A4_H_PT:.2} 0 0 cm /Im1 Do Q"
    );
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
            lopdf::Object::Real(A4_W_PT),
            lopdf::Object::Real(A4_H_PT),
        ],
        "Resources" => dictionary! {
            "XObject" => dictionary! { "Im1" => image_id },
        },
        "Contents" => content_id,
    });
    doc.objects.insert(
        pages_id,
        lopdf::Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => 1,
            "Kids" => vec![page_id.into()],
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    // 不调用 compress()(JPEG 流已是 DCTDecode,二次压缩可能破坏数据)
    doc.save(output)?;

    Ok(IdCardStats {
        elapsed_ms: start.elapsed().as_millis(),
        labeled,
        slot_size_mm: [slot_w as f32 / px_per_mm, slot_h as f32 / px_per_mm],
    })
}

// ===== 单元测试 =====

#[cfg(test)]
mod tests {
    use super::*;

    /// mm/pt 换算系数(仅测试用)
    const MM_PER_PT: f32 = 25.4 / 72.0;

    #[test]
    fn test_a4_geometry() {
        // A4 @300dpi 尺寸
        let w = 210.0 / 25.4 * LAYOUT_DPI;
        let h = 297.0 / 25.4 * LAYOUT_DPI;
        assert!((w - 2480.3).abs() < 1.0);
        assert!((h - 3507.9).abs() < 1.0);
        // pt 与 mm 换算闭环
        assert!((A4_W_PT * MM_PER_PT - 210.0).abs() < 0.01);
        assert!((A4_H_PT * MM_PER_PT - 297.0).abs() < 0.01);
    }

    #[test]
    fn test_wrong_input_count() {
        let err = id_cards_to_pdf(
            &[PathBuf::from("/nonexistent1.png")],
            Path::new("/tmp/never.pdf"),
            &IdCardOptions::default(),
            None,
        )
        .unwrap_err();
        assert_eq!(err.code_string(), "INVALID_ARG");
    }

    /// 端到端排版:两张测试图 → 单页 A4 PDF。
    #[test]
    #[ignore = "生成真实 PDF,人工核对排版效果"]
    fn test_id_cards_to_pdf_end_to_end() {
        let front = Path::new("/tmp/ocr-test-zh.png");
        let back = Path::new("/tmp/ocr-test-table.png");
        if !front.is_file() || !back.is_file() {
            println!("跳过:缺少测试图");
            return;
        }
        let out = std::env::temp_dir().join("pdf-docvault-idcard-test.pdf");
        let stats = id_cards_to_pdf(
            &[front.to_path_buf(), back.to_path_buf()],
            &out,
            &IdCardOptions::default(),
            None,
        )
        .expect("排版应成功");
        println!(
            "elapsed_ms={}, labeled={}, slot={:?}mm",
            stats.elapsed_ms, stats.labeled, stats.slot_size_mm
        );
        assert!(out.is_file());
        // 单页 A4
        let doc = lopdf::Document::load(&out).expect("PDF 应可加载");
        assert_eq!(doc.get_pages().len(), 1);
        println!("产物保留在: {}", out.display());
    }
}
