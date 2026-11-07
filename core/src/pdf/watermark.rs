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
    // 空白文本会生成一条没有 Tj 文本的 content stream:界面提示「添加成功」,
    // 成品却看不出任何水印。与其静默产出无效果的文件,不如直接拒绝。
    if text.trim().is_empty() {
        return Err(CoreError::InvalidArgument("水印文本不能为空".into()));
    }

    let mut doc = Document::load(input)?;
    // 新增的水印流是明文,加密文档需先解密,否则阅读器解出来的水印会是乱码
    super::decrypt_if_needed(&mut doc)?;

    // 字体:纯 ASCII 用 Helvetica(兼容性最好且无额外体积);含中文时用阅读器内置的
    // STSong-Light 复合字体 + UniGB-UCS2-H。后者是「非嵌入」字体,pdfium/Acrobat/Foxit
    // 均内置,无需打包中文字体文件;若继续用 Helvetica,中文会全部乱码。
    let use_cjk = !text.is_ascii();
    let font_obj = if use_cjk {
        let descendant_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "CIDFontType0",
            "BaseFont" => "STSong-Light",
            "CIDSystemInfo" => dictionary! {
                "Registry" => "Adobe",
                "Ordering" => "GB1",
                "Supplement" => 2,
            },
        });
        dictionary! {
            "Type" => "Font",
            "Subtype" => "Type0",
            "BaseFont" => "STSong-Light",
            "Encoding" => "UniGB-UCS2-H",
            "DescendantFonts" => vec![Object::Reference(descendant_id)],
        }
    } else {
        dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        }
    };
    let font_id = doc.add_object(font_obj);
    // 透明度必须通过 ExtGState 的 CA/ca 生效;否则 content 里的 /GS1 gs
    // 引用的是一份不存在的资源,阅读器会直接忽略,滑块形同虚设
    let opacity = opts.opacity.clamp(0.0, 1.0);
    let gs_id = doc.add_object(dictionary! {
        "Type" => "ExtGState",
        "CA" => opacity,
        "ca" => opacity,
    });

    let pages = doc.get_pages();
    // 页面可能把 /Resources 写在中间 Pages 节点上由叶子页继承,
    // 不先落到页面自身,注册水印资源时会误判「页面缺少 Resources」而整份失败
    let page_ids: Vec<ObjectId> = pages.values().copied().collect();
    super::inherit_page_attributes(&mut doc, &page_ids);
    for (_pn, page_id) in pages {
        // 资源名必须避开页面已有同名项:水印流与原正文共用同一份 Resources,
        // 固定写死 /F1、/GS1 会把原文的字体、透明度状态覆盖掉,原内容随之错乱
        let font_name = unique_resource_name(&doc, page_id, b"Font", "F1")?;
        let gs_name = unique_resource_name(&doc, page_id, b"ExtGState", "GS1")?;
        // 构造水印 content stream
        let content = build_text_watermark_content(text, opts, &font_name, &gs_name, use_cjk);
        let stream = Stream::new(dictionary! {}, content);
        let content_id = doc.add_object(stream);

        // 在页面 Resources 中注册字体与透明度状态
        ensure_resources_font(&mut doc, page_id, font_name.as_bytes(), font_id)?;
        ensure_resources_entry(&mut doc, page_id, b"ExtGState", gs_name.as_bytes(), gs_id)?;

        // 追加到页面 /Contents
        append_content(&mut doc, page_id, content_id)?;
    }

    doc.compress();
    doc.save(output)?;
    Ok(())
}

fn build_text_watermark_content(
    text: &str,
    opts: &WatermarkOptions,
    font_name: &str,
    gs_name: &str,
    use_cjk: bool,
) -> Vec<u8> {
    // 复合字体用 <...> 十六进制 UTF-16BE 字面量,普通字体用 (...) 字面量并转义特殊字符
    let text_literal = if use_cjk {
        format!("<{}>", encode_utf16be_hex(text))
    } else {
        let escaped = text
            .replace('\\', "\\\\")
            .replace('(', "\\(")
            .replace(')', "\\)");
        format!("({escaped})")
    };
    let (r, g, b) = (opts.color[0], opts.color[1], opts.color[2]);
    // Tm: a b c d e f,旋转矩阵
    let rad = opts.rotation.to_radians();
    let cos = rad.cos();
    let sin = rad.sin();
    let neg_sin = -sin;
    let content = format!(
        "q\n\
         /{gs_name} gs\n\
         {r} {g} {b} rg\n\
         BT\n\
         /{font_name} {fs} Tf\n\
         {cos:.4} {sin:.4} {nsin:.4} {cos:.4} {x} {y} Tm\n\
         {text_literal} Tj\n\
         ET\n\
         Q\n",
        gs_name = gs_name,
        font_name = font_name,
        r = r,
        g = g,
        b = b,
        fs = opts.font_size,
        cos = cos,
        sin = sin,
        nsin = neg_sin,
        x = opts.x,
        y = opts.y,
        text_literal = text_literal
    );
    content.into_bytes()
}

/// 将文本编码为 UTF-16BE 十六进制字符串,用于复合字体 UniGB-UCS2-H 的 <...> 字面量
fn encode_utf16be_hex(text: &str) -> String {
    let mut hex = String::with_capacity(text.encode_utf16().count() * 4);
    for unit in text.encode_utf16() {
        hex.push_str(&format!("{unit:04X}"));
    }
    hex
}

/// 在页面 Resources 的指定子字典里找一个尚未被占用的资源名。
///
/// 水印内容流与页面原内容共用同一份 Resources,若直接注册固定的 /F1、
/// /GS1、/Im1,会覆盖原文档同名资源,导致正文换字体甚至原图被水印图替换。
fn unique_resource_name(
    doc: &Document,
    page_id: ObjectId,
    category: &[u8],
    base: &str,
) -> Result<String> {
    let page = doc.get_object(page_id)?.as_dict()?;
    let resources = match page.get(b"Resources").ok() {
        Some(Object::Reference(id)) => doc.get_object(*id)?.as_dict()?,
        Some(Object::Dictionary(dict)) => dict,
        _ => return Ok(base.to_string()),
    };
    let category_dict = match resources.get(category).ok() {
        Some(Object::Reference(id)) => doc.get_object(*id)?.as_dict()?,
        Some(Object::Dictionary(dict)) => dict,
        _ => return Ok(base.to_string()),
    };
    let mut name = base.to_string();
    let mut n = 1;
    while category_dict.get(name.as_bytes()).is_ok() {
        n += 1;
        name = format!("{base}_{n}");
    }
    Ok(name)
}

/// 确保页面 Resources 中注册了指定字体。
fn ensure_resources_font(
    doc: &mut Document,
    page_id: ObjectId,
    font_key: &[u8],
    font_id: ObjectId,
) -> Result<()> {
    ensure_resources_entry(doc, page_id, b"Font", font_key, font_id)
}

/// 在页面 Resources 的指定子字典(如 Font / ExtGState / XObject)下注册一个资源名。
/// 处理 Resources 为引用或内联字典两种情况(后者抽出为独立对象)。
fn ensure_resources_entry(
    doc: &mut Document,
    page_id: ObjectId,
    category: &[u8],
    name: &[u8],
    object_id: ObjectId,
) -> Result<()> {
    // 先取出 Resources 的引用方式,避免在持有 page 可变引用时调用 doc.add_object
    let resources_ref = {
        let page_obj = doc.get_object(page_id)?;
        let dict = page_obj.as_dict()?;
        // 规范允许页面不写 /Resources(等同于空资源集),
        // 这里补一个空字典,不能因为缺这一项就让整份水印失败
        dict.get(b"Resources")
            .cloned()
            .unwrap_or(Object::Dictionary(dictionary! {}))
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

    // 子字典可能内联,也可能是指向字典的间接引用(如 `/Font 10 0 R`);
    // 后者直接 as_dict_mut 会报类型错误,导致整份水印失败,这里统一解析后再注册
    let entry = {
        let res_obj = doc.get_object_mut(resources_id)?;
        let res_dict = res_obj.as_dict_mut()?;
        match res_dict.get(category).ok().cloned() {
            Some(obj) => obj,
            None => {
                res_dict.set(category, Object::Dictionary(dictionary! {}));
                Object::Dictionary(dictionary! {})
            }
        }
    };
    match entry {
        Object::Dictionary(_) => {
            let res_obj = doc.get_object_mut(resources_id)?;
            let res_dict = res_obj.as_dict_mut()?;
            res_dict
                .get_mut(category)?
                .as_dict_mut()?
                .set(name, Object::Reference(object_id));
        }
        Object::Reference(cat_id) => {
            let cat_obj = doc.get_object_mut(cat_id)?;
            cat_obj
                .as_dict_mut()?
                .set(name, Object::Reference(object_id));
        }
        _ => return Err(CoreError::Internal("资源子字典类型异常".into())),
    }
    Ok(())
}

fn append_content(doc: &mut Document, page_id: ObjectId, new_content_id: ObjectId) -> Result<()> {
    // /Contents 是可选键:空白页没有它也是合法 PDF。
    // 此前直接报错会让整份文档的水印失败,这里补挂新的内容流即可。
    // 另外 /Contents 也可能是指向数组对象的间接引用,直接把该引用塞进新数组
    // 会得到「数组套数组」的非法结构,需识别后直接往目标数组追加。
    let contents = {
        let page_obj = doc.get_object(page_id)?;
        page_obj.as_dict()?.get(b"Contents").ok().cloned()
    };
    match contents {
        Some(Object::Reference(id)) => {
            if let Object::Array(_) = doc.get_object(id)? {
                doc.get_object_mut(id)?
                    .as_array_mut()?
                    .push(Object::Reference(new_content_id));
            } else {
                let page_obj = doc.get_object_mut(page_id)?;
                page_obj.as_dict_mut()?.set(
                    "Contents",
                    Object::Array(vec![
                        Object::Reference(id),
                        Object::Reference(new_content_id),
                    ]),
                );
            }
        }
        Some(Object::Array(arr)) => {
            let mut new_arr = arr;
            new_arr.push(Object::Reference(new_content_id));
            let page_obj = doc.get_object_mut(page_id)?;
            page_obj
                .as_dict_mut()?
                .set("Contents", Object::Array(new_arr));
        }
        _ => {
            let page_obj = doc.get_object_mut(page_id)?;
            page_obj
                .as_dict_mut()?
                .set("Contents", Object::Reference(new_content_id));
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
    // 与文字水印同理:新增的 XObject 与 content 为明文
    super::decrypt_if_needed(&mut doc)?;

    // 读取图片并转为 PDF XObject。透明背景必须合成到白底再编码:
    // 直接 to_rgb8() 会丢掉 alpha,透明像素变成黑色,水印在页面上就是一个黑块
    let img_bytes = std::fs::read(image_path)?;
    let decoded = image::load_from_memory(&img_bytes)
        .map_err(|e| CoreError::Image(format!("读取图片失败: {e}")))?
        .to_rgba8();
    let (width, height) = decoded.dimensions();
    let mut img = image::RgbImage::new(width, height);
    for (x, y, p) in decoded.enumerate_pixels() {
        let alpha = p[3] as f32 / 255.0;
        let over_white = |c: u8| ((c as f32) * alpha + 255.0 * (1.0 - alpha)).round() as u8;
        img.put_pixel(
            x,
            y,
            image::Rgb([over_white(p[0]), over_white(p[1]), over_white(p[2])]),
        );
    }

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

    // 图片水印同样要遵守不透明度:没有 ExtGState 的 CA/ca,
    // 页面上的 opacity 参数只会被静默忽略,水印永远是全不透明
    let opacity = opts.opacity.clamp(0.0, 1.0);
    let gs_id = doc.add_object(dictionary! {
        "Type" => "ExtGState",
        "CA" => opacity,
        "ca" => opacity,
    });

    let pages = doc.get_pages();
    // 同文字水印:先把继承的 /Resources 落到页面自身
    let page_ids: Vec<ObjectId> = pages.values().copied().collect();
    super::inherit_page_attributes(&mut doc, &page_ids);
    for (_pn, page_id) in pages {
        // 原页面若已用 /Im1 或 /GS1,固定资源名会把原图/原状态覆盖掉
        let img_name = unique_resource_name(&doc, page_id, b"XObject", "Im1")?;
        let gs_name = unique_resource_name(&doc, page_id, b"ExtGState", "GS1")?;
        let content = format!(
            "q\n\
             /{gs_name} gs\n\
             {w} 0 0 {h} {x} {y} cm\n\
             /{name} Do\n\
             Q\n",
            gs_name = gs_name,
            w = opts.font_size * 2.0,
            h = opts.font_size * 2.0 * height as f32 / width as f32,
            x = opts.x,
            y = opts.y,
            name = img_name
        );
        let stream = Stream::new(dictionary! {}, content.into_bytes());
        let content_id = doc.add_object(stream);

        // 注册 XObject 与透明度状态到 Resources
        ensure_resources_xobject(&mut doc, page_id, img_name.as_bytes(), image_id)?;
        ensure_resources_entry(&mut doc, page_id, b"ExtGState", gs_name.as_bytes(), gs_id)?;
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
    ensure_resources_entry(doc, page_id, b"XObject", name, xobject_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 补全 Pages/Catalog 后返回页面对象号
    fn finish_single_page(doc: &mut Document, resources: Object, contents: Object) -> ObjectId {
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Resources" => resources,
            "Contents" => contents,
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
        page_id
    }

    fn resolve<'a>(doc: &'a Document, obj: &'a Object) -> &'a Object {
        match obj {
            Object::Reference(id) => doc.get_object(*id).expect("引用应可解析"),
            other => other,
        }
    }

    #[test]
    fn indirect_resource_subdict_is_registered() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = Document::with_version("1.5");
        // /Font 以间接引用挂到 Resources 下,此前 as_dict_mut 会直接报类型错误
        let font_dict_id = doc.add_object(dictionary! {});
        let resources_id = doc.add_object(dictionary! {
            "Font" => Object::Reference(font_dict_id),
        });
        let content_id = doc.add_object(Stream::new(dictionary! {}, b"q\nQ\n".to_vec()));
        finish_single_page(
            &mut doc,
            Object::Reference(resources_id),
            Object::Reference(content_id),
        );

        let src = dir.path().join("in.pdf");
        let out = dir.path().join("out.pdf");
        doc.save(&src).unwrap();
        add_text_watermark(&src, "DRAFT", &WatermarkOptions::default(), &out).unwrap();

        let out_doc = Document::load(&out).unwrap();
        let pages = out_doc.get_pages();
        let page = out_doc
            .get_object(*pages.get(&1).unwrap())
            .unwrap()
            .as_dict()
            .unwrap();
        let resources = resolve(&out_doc, page.get(b"Resources").unwrap())
            .as_dict()
            .unwrap();
        let fonts = resolve(&out_doc, resources.get(b"Font").unwrap())
            .as_dict()
            .unwrap();
        let font = resolve(&out_doc, fonts.get(b"F1").unwrap())
            .as_dict()
            .unwrap();
        assert_eq!(
            font.get(b"BaseFont").unwrap().as_name_str().unwrap(),
            "Helvetica"
        );
    }

    #[test]
    fn indirect_contents_array_is_appended() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = Document::with_version("1.5");
        let s1 = doc.add_object(Stream::new(dictionary! {}, b"q\nQ\n".to_vec()));
        let s2 = doc.add_object(Stream::new(dictionary! {}, b"q\nQ\n".to_vec()));
        // /Contents 指向间接数组对象,而不是单个内容流
        let arr_id = doc.add_object(Object::Array(vec![
            Object::Reference(s1),
            Object::Reference(s2),
        ]));
        finish_single_page(
            &mut doc,
            Object::Dictionary(dictionary! {}),
            Object::Reference(arr_id),
        );

        let src = dir.path().join("in.pdf");
        let out = dir.path().join("out.pdf");
        doc.save(&src).unwrap();
        add_text_watermark(&src, "DRAFT", &WatermarkOptions::default(), &out).unwrap();

        let out_doc = Document::load(&out).unwrap();
        let arr = out_doc.get_object(arr_id).unwrap().as_array().unwrap();
        assert_eq!(arr.len(), 3, "水印流应追加进目标数组,而不是数组套数组");
        for id in [s1, s2] {
            assert!(matches!(out_doc.get_object(id).unwrap(), Object::Stream(_)));
        }
        let last = match &arr[2] {
            Object::Reference(id) => out_doc.get_object(*id).unwrap(),
            other => other,
        };
        // 水印流很短,doc.compress() 往往不会改写它,没有 Filter 时直接取原始内容
        let stream = last.as_stream().unwrap();
        let raw = if stream.dict.get(b"Filter").is_ok() {
            stream.decompressed_content().unwrap()
        } else {
            stream.content.clone()
        };
        let text = String::from_utf8_lossy(&raw);
        assert!(text.contains("(DRAFT) Tj"), "水印内容应写入新流: {text}");
    }

    #[test]
    fn chinese_text_uses_cjk_font_and_utf16be() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = Document::with_version("1.5");
        let content_id = doc.add_object(Stream::new(dictionary! {}, b"q\nQ\n".to_vec()));
        finish_single_page(
            &mut doc,
            Object::Dictionary(dictionary! {}),
            Object::Reference(content_id),
        );
        let src = dir.path().join("in.pdf");
        let out = dir.path().join("out.pdf");
        doc.save(&src).unwrap();
        // 「机密」二字含非 ASCII,应走复合字体路径而非 Helvetica
        add_text_watermark(&src, "机密", &WatermarkOptions::default(), &out).unwrap();

        let out_doc = Document::load(&out).unwrap();
        let pages = out_doc.get_pages();
        let page = out_doc
            .get_object(*pages.get(&1).unwrap())
            .unwrap()
            .as_dict()
            .unwrap();
        let resources = resolve(&out_doc, page.get(b"Resources").unwrap())
            .as_dict()
            .unwrap();
        let fonts = resolve(&out_doc, resources.get(b"Font").unwrap())
            .as_dict()
            .unwrap();
        let font = resolve(&out_doc, fonts.get(b"F1").unwrap())
            .as_dict()
            .unwrap();
        assert_eq!(
            font.get(b"Subtype").unwrap().as_name_str().unwrap(),
            "Type0",
            "中文水印应使用复合字体"
        );
        assert_eq!(
            font.get(b"BaseFont").unwrap().as_name_str().unwrap(),
            "STSong-Light"
        );
        // 断言编码为 UTF-16BE:机=U+673A,密=U+5BC6
        let last_obj = match resolve(&out_doc, page.get(b"Contents").unwrap()) {
            Object::Array(arr) => resolve(&out_doc, arr.last().unwrap()),
            _ => panic!("应追加为 Contents 数组"),
        };
        let stream = last_obj.as_stream().unwrap();
        let raw = if stream.dict.get(b"Filter").is_ok() {
            stream.decompressed_content().unwrap()
        } else {
            stream.content.clone()
        };
        let text = String::from_utf8_lossy(&raw);
        assert!(
            text.contains("<673A5BC6> Tj"),
            "中文水印内容应为 UTF-16BE 十六进制字面量: {text}"
        );
    }
}
