//! PDF 压缩
//!
//! 两档策略,由 `level`(1..=9)控制:
//!
//! | level | 策略 | 是否损失画质 |
//! |---|---|---|
//! | 1..=3 | 结构压缩:`prune_objects` + `renumber_objects` + 未压缩 content stream 转 FlateDecode | 否 |
//! | 4..=9 | 结构压缩 + 内嵌图片有损重编码(重采样 + 降 JPEG 质量) | 是 |
//!
//! 图片重编码刻意保守:
//! - 只处理 `DeviceRGB` / `DeviceGray` 且 `BitsPerComponent == 8` 的位图;
//!   Indexed / ICCBased / CMYK / Separation 一律原样保留(转换易错色)。
//! - 只处理 `DCTDecode`(JPEG)与无 predictor 的 `FlateDecode`,其余(JPX/CCITT/JBIG2/LZW)跳过。
//! - 带 `SMask` / `Mask` / `Decode` 的图跳过:重编码会丢透明或反色语义。
//! - 重编码后字节没变小就回滚为原图——压缩绝不能把文件搞大。
//! - 单图失败不中断整本;最终重新加载校验,页数不符则回退为纯结构压缩的结果。

use std::io::Read;
use std::path::Path;
use std::time::Instant;

use image::{DynamicImage, ImageFormat};
use lopdf::{Dictionary, Document, Object, Stream};

use crate::errors::Result;

/// 低于该像素数的图不值得重编码:体积收益小,反而可能因基线开销变大。
const MIN_PIXELS: u64 = 100_000;

/// 压缩统计
#[derive(Debug, Clone, serde::Serialize)]
pub struct CompressStats {
    /// 原始字节数
    pub original_size: u64,
    /// 压缩后字节数
    pub compressed_size: u64,
    /// 压缩比 %
    pub ratio: f32,
    /// 耗时(毫秒)
    pub elapsed_ms: u128,
    /// 实际重编码的图片数量(level 1..=3 恒为 0)
    pub images_reencoded: usize,
    /// 是否因校验失败回退为纯结构压缩
    pub fell_back: bool,
}

/// 某一档位对应的图片重编码预算:`(jpeg 质量, 长边上限像素)`。`None` = 本档不做有损处理。
fn image_budget(level: u8) -> Option<(u8, u32)> {
    match level {
        0..=3 => None,
        4 => Some((82, 2400)),
        5 => Some((78, 2000)),
        6 => Some((72, 1800)),
        7 => Some((65, 1500)),
        8 => Some((55, 1280)),
        _ => Some((45, 1000)),
    }
}

/// 压缩 PDF。`level` 夹取到 1..=9;4 以上才会动图片。
pub fn compress(input: &Path, output: &Path, level: u8) -> Result<CompressStats> {
    let start = Instant::now();
    let level = level.clamp(1, 9);
    let original_size = std::fs::metadata(input)?.len();

    let mut doc = Document::load(input)?;
    // 与其他页面级操作一致:加密文档的流是按对象号派生密钥的,
    // 这里会 prune/renumber 并重编码图片,不先解密会产出无法打开的成品
    super::decrypt_if_needed(&mut doc)?;
    let page_count = doc.get_pages().len();
    doc.prune_objects();
    doc.renumber_objects();

    let mut images_reencoded = 0usize;
    if let Some((quality, max_edge)) = image_budget(level) {
        for obj in doc.objects.values_mut() {
            if let Object::Stream(stream) = obj {
                if shrink_image(stream, quality, max_edge) {
                    images_reencoded += 1;
                }
            }
        }
    }
    doc.compress();
    doc.save(output)?;

    // 校验:重编码出错可能是坏文件(解码器面对畸形流往往"能解但解错"),
    // 所以以「重新加载 + 页数一致」为唯一判据,失败就退回纯结构压缩。
    let mut fell_back = false;
    if images_reencoded > 0 && !loads_with_pages(output, page_count) {
        tracing::warn!("压缩后校验失败(页数不一致或无法加载),回退为纯结构压缩");
        fell_back = true;
        let mut safe = Document::load(input)?;
        super::decrypt_if_needed(&mut safe)?;
        safe.prune_objects();
        safe.renumber_objects();
        safe.compress();
        safe.save(output)?;
        images_reencoded = 0;
    } else {
        // 结构压缩本身也可能产出坏文件(极端畸形输入),同样兜一层
        if !loads_with_pages(output, page_count) {
            tracing::warn!("结构压缩后校验失败,原样复制输入");
            fell_back = true;
            std::fs::copy(input, output)?;
        }
    }

    let compressed_size = std::fs::metadata(output)?.len();
    let ratio = if original_size > 0 {
        (1.0 - compressed_size as f32 / original_size as f32) * 100.0
    } else {
        0.0
    };
    Ok(CompressStats {
        original_size,
        compressed_size,
        ratio,
        elapsed_ms: start.elapsed().as_millis(),
        images_reencoded,
        fell_back,
    })
}

fn loads_with_pages(path: &Path, expect: usize) -> bool {
    Document::load(path)
        .map(|d| d.get_pages().len() == expect)
        .unwrap_or(false)
}

// ===== 单张图片重编码 =====

/// 原地重编码一个图片 XObject。返回 `true` 表示内容被替换。
fn shrink_image(stream: &mut Stream, quality: u8, max_edge: u32) -> bool {
    // 只处理真正的图片 XObject
    if dict_name(&stream.dict, b"Subtype") != Some("Image") {
        return false;
    }
    if dict_bool(&stream.dict, b"ImageMask") {
        return false;
    }
    if dict_u32(&stream.dict, b"BitsPerComponent") != Some(8) {
        return false;
    }
    // 带透明蒙版或反色 Decode 数组的图,重编码语义会变,一律不动
    for key in [
        b"SMask".as_slice(),
        b"Mask".as_slice(),
        b"Decode".as_slice(),
    ] {
        if stream.dict.get(key).is_ok() {
            return false;
        }
    }
    let gray = match dict_name(&stream.dict, b"ColorSpace") {
        Some("DeviceGray") => true,
        Some("DeviceRGB") => false,
        _ => return false, // Indexed/ICCBased/DeviceCMYK/Separation:保留原样
    };
    let (w, h) = match (
        dict_u32(&stream.dict, b"Width"),
        dict_u32(&stream.dict, b"Height"),
    ) {
        (Some(w), Some(h)) if w > 0 && h > 0 => (w, h),
        _ => return false,
    };
    if (w as u64) * (h as u64) < MIN_PIXELS {
        return false;
    }

    let has_params = stream.dict.get(b"DecodeParms").is_ok();
    let decoded: DynamicImage = match dict_name(&stream.dict, b"Filter") {
        // JPEG:原始字节直接可解码
        Some("DCTDecode") => {
            match image::load_from_memory_with_format(&stream.content, ImageFormat::Jpeg) {
                Ok(img) => img,
                Err(_) => return false,
            }
        }
        // 未压缩 / FlateDecode(无 predictor):按声明尺寸重建位图
        Some("FlateDecode") if !has_params => match inflate(&stream.content) {
            Ok(raw) => match build_bitmap(&raw, w, h, gray) {
                Some(img) => img,
                None => return false,
            },
            Err(_) => return false,
        },
        None if !has_params => match build_bitmap(&stream.content, w, h, gray) {
            Some(img) => img,
            None => return false,
        },
        // JPXDecode / CCITTFaxDecode / JBIG2Decode / LZW / 多级 Filter:不动
        _ => return false,
    };

    // 已声明是灰度而解码结果带彩通道,或反之,说明流与字典不符,放弃
    if gray && decoded.color().channel_count() > 2 {
        return false;
    }

    let (nw, nh) = fit_within(w, h, max_edge);
    let scaled = if nw != w || nh != h {
        decoded.resize(nw, nh, image::imageops::FilterType::Lanczos3)
    } else {
        decoded
    };

    let mut encoded = Vec::new();
    {
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, quality);
        let ok = if gray {
            enc.encode_image(&scaled.to_luma8()).is_ok()
        } else {
            enc.encode_image(&scaled.to_rgb8()).is_ok()
        };
        if !ok {
            return false;
        }
    }
    // 没变小就当没做——压缩不能把文件搞大
    if encoded.len() >= stream.content.len() {
        return false;
    }

    stream
        .dict
        .set("Filter", Object::Name(b"DCTDecode".to_vec()));
    stream.dict.remove(b"DecodeParms");
    stream.dict.remove(b"Decode");
    stream.dict.set("Width", nw as i64);
    stream.dict.set("Height", nh as i64);
    stream.dict.set("BitsPerComponent", 8);
    stream.dict.set(
        "ColorSpace",
        Object::Name(if gray {
            b"DeviceGray".to_vec()
        } else {
            b"DeviceRGB".to_vec()
        }),
    );
    stream.set_content(encoded);
    true
}

/// 按长边等比缩放到 `max_edge` 以内(不放大)
fn fit_within(w: u32, h: u32, max_edge: u32) -> (u32, u32) {
    let long = w.max(h);
    if long <= max_edge {
        return (w, h);
    }
    let scale = max_edge as f64 / long as f64;
    let nw = ((w as f64 * scale).round() as u32).max(1);
    let nh = ((h as f64 * scale).round() as u32).max(1);
    (nw, nh)
}

/// 由原始像素字节重建位图,长度不足则返回 None
fn build_bitmap(raw: &[u8], w: u32, h: u32, gray: bool) -> Option<DynamicImage> {
    let channels = if gray { 1usize } else { 3 };
    let need = (w as usize)
        .checked_mul(h as usize)?
        .checked_mul(channels)?;
    if raw.len() < need {
        return None;
    }
    let buf = raw[..need].to_vec();
    if gray {
        image::GrayImage::from_raw(w, h, buf).map(DynamicImage::ImageLuma8)
    } else {
        image::RgbImage::from_raw(w, h, buf).map(DynamicImage::ImageRgb8)
    }
}

fn inflate(data: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(data).read_to_end(&mut out)?;
    Ok(out)
}

fn dict_name<'a>(dict: &'a Dictionary, key: &[u8]) -> Option<&'a str> {
    dict.get(key).ok().and_then(|o| o.as_name_str().ok())
}

fn dict_bool(dict: &Dictionary, key: &[u8]) -> bool {
    dict.get(key)
        .ok()
        .and_then(|o| o.as_bool().ok())
        .unwrap_or(false)
}

fn dict_u32(dict: &Dictionary, key: &[u8]) -> Option<u32> {
    dict.get(key)
        .ok()
        .and_then(|o| o.as_i64().ok())
        .and_then(|v| u32::try_from(v).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::content::{Content, Operation};
    use lopdf::dictionary;

    // ===== 夹具:构造一页带内嵌图片的 PDF =====

    fn noise_jpeg(w: u32, h: u32) -> Vec<u8> {
        let mut img = image::RgbImage::new(w, h);
        let mut seed: u32 = 0x1234_5678;
        for p in img.pixels_mut() {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *p = image::Rgb([(seed >> 16) as u8, (seed >> 8) as u8, seed as u8]);
        }
        let mut buf = Vec::new();
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 95);
        enc.encode_image(&img).unwrap();
        buf
    }

    /// 构造单页 PDF,页面内容绘制一个铺满页面的图片 XObject
    fn build_pdf(
        image_bytes: Vec<u8>,
        w: u32,
        h: u32,
        color_space: &str,
        filter: Option<&str>,
    ) -> Document {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();

        let mut img_dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => w as i64,
            "Height" => h as i64,
            "ColorSpace" => color_space,
            "BitsPerComponent" => 8,
        };
        if let Some(f) = filter {
            img_dict.set("Filter", f);
        }
        let img_id = doc.add_object(Stream::new(img_dict, image_bytes));

        let resources_id = doc.add_object(dictionary! {
            "XObject" => dictionary! { "Im0" => img_id },
        });

        let content = Content {
            operations: vec![
                Operation::new("q", vec![]),
                Operation::new(
                    "cm",
                    vec![
                        595.into(),
                        0.into(),
                        0.into(),
                        842.into(),
                        0.into(),
                        0.into(),
                    ],
                ),
                Operation::new("Do", vec![Object::Name(b"Im0".to_vec())]),
                Operation::new("Q", vec![]),
            ],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));

        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Resources" => resources_id,
            "Contents" => content_id,
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        doc
    }

    fn write_and_compress(mut doc: Document, dir: &Path, level: u8) -> (u64, CompressStats) {
        let src = dir.join(format!("in-{level}.pdf"));
        let out = dir.join(format!("out-{level}.pdf"));
        doc.save(&src).unwrap();
        let stats = compress(&src, &out, level).unwrap();
        assert!(
            loads_with_pages(&out, 1),
            "输出必须是可加载且页数一致的 PDF"
        );
        (stats.compressed_size, stats)
    }

    #[test]
    fn budget_gates_structural_levels() {
        for level in 1..=3u8 {
            assert!(
                image_budget(level).is_none(),
                "level {level} 不应做有损处理"
            );
        }
        for level in 4..=9u8 {
            assert!(
                image_budget(level).is_some(),
                "level {level} 应做图片重编码"
            );
        }
        // 越界值夹取
        assert!(image_budget(0).is_none());
        assert!(image_budget(255).is_some());
    }

    #[test]
    fn high_level_shrinks_embedded_jpeg() {
        let dir = tempfile::tempdir().unwrap();
        let jpeg = noise_jpeg(600, 600);
        let (low_size, low_stats) = write_and_compress(
            build_pdf(jpeg.clone(), 600, 600, "DeviceRGB", Some("DCTDecode")),
            dir.path(),
            1,
        );
        assert_eq!(low_stats.images_reencoded, 0, "结构档不应碰图片");

        let (high_size, high_stats) = write_and_compress(
            build_pdf(jpeg, 600, 600, "DeviceRGB", Some("DCTDecode")),
            dir.path(),
            9,
        );
        assert_eq!(high_stats.images_reencoded, 1, "高分档应重编码 1 张图");
        assert!(
            high_size < low_size,
            "level 9 应小于 level 1: {high_size} vs {low_size}"
        );
        assert!(!high_stats.fell_back);
    }

    #[test]
    fn flate_bitmap_is_reencoded() {
        let dir = tempfile::tempdir().unwrap();
        let w = 600u32;
        let h = 600u32;
        let mut img = image::RgbImage::new(w, h);
        let mut seed: u32 = 0xDEAD_BEEF;
        for p in img.pixels_mut() {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *p = image::Rgb([(seed >> 16) as u8, (seed >> 8) as u8, seed as u8]);
        }
        // 用 zlib 压缩原始 RGB 像素,模拟 PNG 转 PDF 的流
        let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        use std::io::Write;
        enc.write_all(img.as_raw()).unwrap();
        let flate = enc.finish().unwrap();

        let (low_size, _) = write_and_compress(
            build_pdf(flate.clone(), w, h, "DeviceRGB", Some("FlateDecode")),
            dir.path(),
            1,
        );
        let (high_size, stats) = write_and_compress(
            build_pdf(flate, w, h, "DeviceRGB", Some("FlateDecode")),
            dir.path(),
            9,
        );
        assert_eq!(stats.images_reencoded, 1);
        assert!(high_size < low_size, "{high_size} vs {low_size}");
    }

    #[test]
    fn tiny_image_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let jpeg = noise_jpeg(200, 200); // 40k px < MIN_PIXELS
        let (low_size, _) = write_and_compress(
            build_pdf(jpeg.clone(), 200, 200, "DeviceRGB", Some("DCTDecode")),
            dir.path(),
            1,
        );
        let (high_size, stats) = write_and_compress(
            build_pdf(jpeg, 200, 200, "DeviceRGB", Some("DCTDecode")),
            dir.path(),
            9,
        );
        assert_eq!(stats.images_reencoded, 0, "小图不值得重编码");
        assert_eq!(high_size, low_size, "小图档位间体积应一致");
    }

    #[test]
    fn indexed_colorspace_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let jpeg = noise_jpeg(600, 600);
        let (_size, stats) = write_and_compress(
            build_pdf(jpeg, 600, 600, "Indexed", Some("DCTDecode")),
            dir.path(),
            9,
        );
        assert_eq!(stats.images_reencoded, 0, "Indexed 色空间不应被重编码");
    }

    #[test]
    fn unsupported_filter_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let (_, stats) = write_and_compress(
            build_pdf(
                vec![0u8; 600 * 600 * 3],
                600,
                600,
                "DeviceRGB",
                Some("JPXDecode"),
            ),
            dir.path(),
            9,
        );
        assert_eq!(stats.images_reencoded, 0, "JPXDecode 不应被触碰");
    }

    #[test]
    fn fit_within_keeps_ratio() {
        assert_eq!(fit_within(1000, 500, 1000), (1000, 500));
        assert_eq!(fit_within(2000, 1000, 1000), (1000, 500));
        assert_eq!(fit_within(1000, 2000, 1000), (500, 1000));
        // 不放大
        assert_eq!(fit_within(100, 100, 1000), (100, 100));
    }
}
