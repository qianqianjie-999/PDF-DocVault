//! PDF 水印
//!
//! 文字水印:为每页创建一个独立的 content stream,内含 BT/Tf/rg/Tm/Tj/ET 操作,
//! 追加到页面 /Contents 数组。这样不破坏原 content stream。
//! 图片水印:将图片转为 PDF XObject,在 content stream 中用 Do 绘制。

use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use std::path::Path;

use crate::errors::{CoreError, Result};

/// 水印选项
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WatermarkOptions {
    /// 字体大小(磅)
    pub font_size: f32,
    /// 旋转角度(度)
    pub rotation: f32,
    /// 不透明度 0.0-1.0
    pub opacity: f32,
    /// RGB 颜色 0.0-1.0
    pub color: [f32; 3],
    /// 位置(页面左下角为原点,磅)
    pub x: f32,
    pub y: f32,
}

impl Default for WatermarkOptions {
    fn default() -> Self {
        Self {
            font_size: 48.0,
            rotation: 45.0,
            opacity: 0.3,
            color: [0.5, 0.5, 0.5],
            x: 150.0,
            y: 300.0,
        }
    }
}

/// 添加文字水印
pub fn add_text_watermark(
    input: &Path,
    text: &str,
    opts: &WatermarkOptions,
    output: &Path,
) -> Result<()> {
    let mut doc = Document::load(input)?;

    // 创建 Helvetica 字体对象
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });

    let pages = doc.get_pages();
    for (_pn, page_id) in pages {
        // 构造水印 content stream
        let content = build_text_watermark_content(text, opts);
        let stream = Stream::new(dictionary! {}, content);
        let content_id = doc.add_object(stream);

        // 在页面 Resources 中注册字体
        ensure_resources_font(&mut doc, page_id, b"F1", font_id)?;

        // 追加到页面 /Contents
        append_content(&mut doc, page_id, content_id)?;
    }

    doc.compress();
    doc.save(output)?;
    Ok(())
}

fn build_text_watermark_content(text: &str, opts: &WatermarkOptions) -> Vec<u8> {
    // 构造 PDF content stream
    // 转义文本中的特殊字符
    let escaped = text.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
    let (r, g, b) = (opts.color[0], opts.color[1], opts.color[2]);
    // Tm: a b c d e f,旋转矩阵
    let rad = opts.rotation.to_radians();
    let cos = rad.cos();
    let sin = rad.sin();
    let neg_sin = -sin;
    let content = format!(
        "q\n\
         /GS1 gs\n\
         {r} {g} {b} rg\n\
         BT\n\
         /F1 {fs} Tf\n\
         {cos:.4} {sin:.4} {nsin:.4} {cos:.4} {x} {y} Tm\n\
         ({escaped}) Tj\n\
         ET\n\
         Q\n",
        r = r,
        g = g,
        b = b,
        fs = opts.font_size,
        cos = cos,
        sin = sin,
        nsin = neg_sin,
        x = opts.x,
        y = opts.y,
        escaped = escaped
    );
    content.into_bytes()
}

/// 确保页面 Resources 中注册了指定字体。
/// 处理 Resources 为引用或内联字典两种情况(后者抽出为独立对象)。
fn ensure_resources_font(
    doc: &mut Document,
    page_id: ObjectId,
    font_key: &[u8],
    font_id: ObjectId,
) -> Result<()> {
    // 先取出 Resources 的引用方式,避免在持有 page 可变引用时调用 doc.add_object
    let resources_ref = {
        let page_obj = doc.get_object(page_id)?;
        let dict = page_obj.as_dict()?;
        dict.get(b"Resources")
            .map_err(|_| CoreError::Internal("页面缺少 Resources".into()))?
            .clone()
    };

    let resources_id = match resources_ref {
        Object::Reference(id) => id,
        Object::Dictionary(_) => {
            // Resources 内联在页面里,抽出为独立对象
            let id = doc.add_object(resources_ref);
            let page_obj = doc.get_object_mut(page_id)?;
            let dict = page_obj.as_dict_mut()?;
            dict.set("Resources", Object::Reference(id));
            id
        }
        _ => return Err(CoreError::Internal("Resources 类型异常".into())),
    };

    let res_obj = doc.get_object_mut(resources_id)?;
    let res_dict = res_obj.as_dict_mut()?;
    if res_dict.get(b"Font").is_err() {
        res_dict.set("Font", Object::Dictionary(dictionary! {}));
    }
    let font_obj = res_dict.get_mut(b"Font")?;
    let font_dict = font_obj.as_dict_mut()?;
    font_dict.set(font_key, Object::Reference(font_id));
    Ok(())
}

fn append_content(doc: &mut Document, page_id: ObjectId, new_content_id: ObjectId) -> Result<()> {
    let page_obj = doc.get_object_mut(page_id)?;
    let dict = page_obj.as_dict_mut()?;
    let contents = dict.get(b"Contents").map_err(|_| {
        CoreError::Internal("页面缺少 Contents".into())
    })?;
    let contents_clone = contents.clone();
    match contents_clone {
        Object::Reference(id) => {
            dict.set(
                "Contents",
                Object::Array(vec![Object::Reference(id), Object::Reference(new_content_id)]),
            );
        }
        Object::Array(arr) => {
            let mut new_arr = arr;
            new_arr.push(Object::Reference(new_content_id));
            dict.set("Contents", Object::Array(new_arr));
        }
        _ => {
            dict.set("Contents", Object::Reference(new_content_id));
        }
    }
    Ok(())
}

/// 添加图片水印(简化:将图片作为 XObject 绘制到每页)
pub fn add_image_watermark(
    input: &Path,
    image_path: &Path,
    opts: &WatermarkOptions,
    output: &Path,
) -> Result<()> {
    let mut doc = Document::load(input)?;

    // 读取图片并转为 PDF XObject
    let img_bytes = std::fs::read(image_path)?;
    let img = image::load_from_memory(&img_bytes)
        .map_err(|e| CoreError::Image(format!("读取图片失败: {e}")))?
        .to_rgb8();
    let (width, height) = img.dimensions();

    // 编码为 JPEG 以减小体积
    let mut jpeg_buf = std::io::Cursor::new(Vec::new());
    img.write_with_encoder(image::codecs::jpeg::JpegEncoder::new(&mut jpeg_buf))
        .map_err(|e| CoreError::Image(format!("编码 JPEG 失败: {e}")))?;
    let jpeg_data = jpeg_buf.into_inner();

    let stream_dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => width as i64,
        "Height" => height as i64,
        "ColorSpace" => "DeviceRGB",
        "BitsPerComponent" => 8,
        "Filter" => "DCTDecode",
    };
    let image_stream = Stream::new(stream_dict, jpeg_data);
    let image_id = doc.add_object(image_stream);

    // 资源名
    let img_name = "Im1";

    let pages = doc.get_pages();
    for (_pn, page_id) in pages {
        // 绘制图片的 content
        let content = format!(
            "q\n\
             {w} 0 0 {h} {x} {y} cm\n\
             /{name} Do\n\
             Q\n",
            w = opts.font_size * 2.0,
            h = opts.font_size * 2.0 * height as f32 / width as f32,
            x = opts.x,
            y = opts.y,
            name = img_name
        );
        let stream = Stream::new(dictionary! {}, content.into_bytes());
        let content_id = doc.add_object(stream);

        // 注册 XObject 到 Resources
        ensure_resources_xobject(&mut doc, page_id, img_name.as_bytes(), image_id)?;
        append_content(&mut doc, page_id, content_id)?;
    }

    doc.compress();
    doc.save(output)?;
    Ok(())
}

fn ensure_resources_xobject(
    doc: &mut Document,
    page_id: ObjectId,
    name: &[u8],
    xobject_id: ObjectId,
) -> Result<()> {
    // 先取出 Resources 的引用方式,避免在持有 page 可变引用时调用 doc.add_object
    let resources_ref = {
        let page_obj = doc.get_object(page_id)?;
        let dict = page_obj.as_dict()?;
        dict.get(b"Resources")
            .map_err(|_| CoreError::Internal("页面缺少 Resources".into()))?
            .clone()
    };

    let resources_id = match resources_ref {
        Object::Reference(id) => id,
        Object::Dictionary(_) => {
            let id = doc.add_object(resources_ref);
            let page_obj = doc.get_object_mut(page_id)?;
            let dict = page_obj.as_dict_mut()?;
            dict.set("Resources", Object::Reference(id));
            id
        }
        _ => return Err(CoreError::Internal("Resources 类型异常".into())),
    };

    let res_obj = doc.get_object_mut(resources_id)?;
    let res_dict = res_obj.as_dict_mut()?;
    if res_dict.get(b"XObject").is_err() {
        res_dict.set("XObject", Object::Dictionary(dictionary! {}));
    }
    let xo_obj = res_dict.get_mut(b"XObject")?;
    let xo_dict = xo_obj.as_dict_mut()?;
    xo_dict.set(name, Object::Reference(xobject_id));
    Ok(())
}
