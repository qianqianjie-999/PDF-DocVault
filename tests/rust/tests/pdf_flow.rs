//! PDF 全流程集成测试:merge → split → pages → watermark → compress → 加密 → 渲染。
//!
//! 覆盖规划书阶段 I 要求:合并后页数正确、拆分后可重新 load、
//! 旋转后 /Rotate 正确、加密后能解密回原页数。
//! 外部依赖缺失(pdfium / pycore-pikepdf)时对应测试自动跳过。

mod common;

use common::{ensure_pdfium, make_pdf, page_count, pycrypt_available};
use lopdf::{dictionary, Object};
use pdf_docvault_core::pdf::{
    add_image_watermark, add_text_watermark, compress, decrypt, delete_pages, encrypt_aes256,
    extract_pages, insert_pages, merge, probe_encryption, reorder_pages, rotate_pages, split,
    EncryptOptions, PageRange, Permissions, SplitMode, WatermarkOptions,
};
use std::path::Path;

/// 构造「根 Pages → 中间 Pages(带 MediaBox/Resources/Rotate)→ 页面」的嵌套页面树。
/// 真实排版工具常把版式属性写在中间节点上由叶子页继承,用于验证重建页面树时的继承处理。
fn make_nested_pdf(pages: u32) -> std::path::PathBuf {
    // 复用 make_pdf 生成的唯一临时目录,避免测试并行时互相覆盖
    let dir = make_pdf(1).parent().unwrap().to_path_buf();
    let path = dir.join("nested.pdf");

    let mut doc = lopdf::Document::with_version("1.5");
    let root_id = doc.new_object_id();
    let mid_id = doc.new_object_id();
    let content_id = doc.add_object(lopdf::Stream::new(dictionary! {}, b"".to_vec()));
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });
    let mut kids = Vec::new();
    for _ in 0..pages {
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => mid_id,
            "Contents" => content_id,
        });
        kids.push(Object::Reference(page_id));
    }
    doc.objects.insert(
        mid_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Parent" => root_id,
            "Count" => pages,
            "Kids" => kids,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Resources" => resources_id,
            "Rotate" => 90,
        }),
    );
    doc.objects.insert(
        root_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => pages,
            "Kids" => vec![Object::Reference(mid_id)],
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => root_id });
    doc.trailer.set("Root", catalog_id);
    doc.save(&path).expect("保存嵌套页面树 PDF");
    path
}

// ===== 合并 =====

/// 合并后页数 = 各输入页数之和(规划书验证点)
#[test]
fn merge_pages_sum_correctly() {
    let a = make_pdf(2);
    let b = make_pdf(3);
    let out = a.parent().unwrap().join("merged.pdf");

    let stats = merge(&[a.clone(), b.clone()], &out).expect("合并应成功");
    assert_eq!(stats.pages, 5, "合并统计页数应为 5");
    assert_eq!(page_count(&out), 5, "2 页 + 3 页应合并为 5 页");
}

/// 需要打开密码的 PDF 不能静默合并:否则加密流会被原样写进新文档,产出乱码
#[test]
fn merge_locked_pdf_fails_clearly() {
    let src = make_pdf(1);
    let locked = src.parent().unwrap().join("locked.pdf");
    let mut doc = lopdf::Document::load(&src).expect("加载测试 PDF");
    // 伪造带 /Encrypt 的文档:空密码无法通过密钥校验,用于验证拒绝逻辑
    let enc = doc.add_object(dictionary! {
        "Filter" => "Standard",
        "V" => 1,
    });
    doc.trailer.set("Encrypt", enc);
    doc.save(&locked).expect("写出加密占位 PDF");

    let out = src.parent().unwrap().join("merged-locked.pdf");
    let err = merge(&[locked], &out).expect_err("加密文件应被拒绝");
    assert!(
        format!("{err}").contains("已加密"),
        "错误信息应提示先解密,实际为: {err}"
    );
}

// ===== 拆分 =====

/// 按范围拆分:拆出的每个文件可重新 load 且页数正确
#[test]
fn split_by_ranges_reloads() {
    let src = make_pdf(3);
    let out_dir = src.parent().unwrap().join("split-ranges");

    let stats = split(
        &src,
        &SplitMode::ByRanges {
            ranges: vec![
                pdf_docvault_core::pdf::PageRange { start: 1, end: 2 },
                pdf_docvault_core::pdf::PageRange { start: 3, end: 3 },
            ],
        },
        &out_dir,
    )
    .expect("按范围拆分应成功");
    assert_eq!(stats.outputs, 2);
    assert_eq!(page_count(&stats.paths[0]), 2);
    assert_eq!(page_count(&stats.paths[1]), 1);
}

/// 每 N 页拆分:3 页按每 2 页拆 → 2 个文件(2 页 + 1 页)
#[test]
fn split_every_n_pages() {
    let src = make_pdf(3);
    let out_dir = src.parent().unwrap().join("split-every-n");

    let stats = split(&src, &SplitMode::EveryNPages { n: 2 }, &out_dir).expect("拆分应成功");
    assert_eq!(stats.outputs, 2);
    assert_eq!(page_count(&stats.paths[0]), 2);
    assert_eq!(page_count(&stats.paths[1]), 1);
}

/// 起始页超出总页数的范围应直接报错,而不是截断后报「范围内没有页面」
#[test]
fn split_range_start_beyond_total_fails() {
    let src = make_pdf(3);
    let out_dir = src.parent().unwrap().join("split-oob");

    let err = split(
        &src,
        &SplitMode::ByRanges {
            ranges: vec![pdf_docvault_core::pdf::PageRange { start: 4, end: 6 }],
        },
        &out_dir,
    )
    .expect_err("起始页越界应报错");
    assert!(
        format!("{err}").contains("超出文档总页数"),
        "错误信息应说明页码越界,实际为: {err}"
    );
}

/// 同一目录重复拆分不能覆盖上一次的成果:文件名撞车时应追加序号
#[test]
fn split_twice_keeps_previous_outputs() {
    let src = make_pdf(3);
    let out_dir = src.parent().unwrap().join("split-twice");

    let first = split(&src, &SplitMode::EveryNPages { n: 2 }, &out_dir).expect("第一次拆分应成功");
    let second = split(&src, &SplitMode::EveryNPages { n: 2 }, &out_dir).expect("第二次拆分应成功");
    assert_eq!(second.outputs, first.outputs);
    for (a, b) in first.paths.iter().zip(second.paths.iter()) {
        assert_ne!(a, b, "第二次拆分不应写回同一路径");
        assert!(a.is_file() && b.is_file(), "两次拆分的文件都应保留");
    }
}

/// 同一批里的重复范围应各写一个独立文件,否则两个并行任务会写同一路径
#[test]
fn split_duplicate_ranges_write_distinct_files() {
    let src = make_pdf(3);
    let out_dir = src.parent().unwrap().join("split-dup");

    let stats = split(
        &src,
        &SplitMode::ByRanges {
            ranges: vec![
                pdf_docvault_core::pdf::PageRange { start: 1, end: 2 },
                pdf_docvault_core::pdf::PageRange { start: 1, end: 2 },
            ],
        },
        &out_dir,
    )
    .expect("重复范围拆分应成功");
    assert_eq!(stats.outputs, 2);
    assert_ne!(stats.paths[0], stats.paths[1], "重复范围不应写到同一个文件");
    for p in &stats.paths {
        assert!(p.is_file(), "输出文件应存在: {}", p.display());
    }
}

// ===== 页面操作 =====

/// 旋转后 /Rotate 字段正确(规划书验证点)
#[test]
fn rotate_sets_rotate_field() {
    let src = make_pdf(3);
    let out = src.parent().unwrap().join("rotated.pdf");

    rotate_pages(&src, &[1, 3], 90, &out).expect("旋转应成功");
    let doc = lopdf::Document::load(&out).expect("旋转结果可重新加载");
    assert_eq!(doc.get_pages().len(), 3, "页数不应变化");
    for (idx, id) in doc.get_pages() {
        let rotate = doc
            .get_object(id)
            .expect("页对象存在")
            .as_dict()
            .expect("页为字典")
            .get(b"Rotate")
            .and_then(|o| o.as_i64())
            .unwrap_or(0);
        if idx == 1 || idx == 3 {
            assert_eq!(rotate, 90, "第 {idx} 页应旋转 90 度");
        } else {
            assert_eq!(rotate, 0, "第 {idx} 页不应被旋转");
        }
    }
}

/// 页码重复时只旋转一次,避免 "1,1" 被累加成 180°
#[test]
fn rotate_duplicate_pages_once() {
    let src = make_pdf(3);
    let out = src.parent().unwrap().join("rotated-dup.pdf");

    rotate_pages(&src, &[1, 1], 90, &out).expect("重复页码旋转应成功");
    let doc = lopdf::Document::load(&out).expect("旋转结果可重新加载");
    let rotate = doc
        .get_object(*doc.get_pages().get(&1).unwrap())
        .expect("页对象存在")
        .as_dict()
        .expect("页为字典")
        .get(b"Rotate")
        .and_then(|o| o.as_i64())
        .unwrap_or(0);
    assert_eq!(rotate, 90, "重复页码不应重复累加角度");
}

/// 页码留空时旋转应作用于全部页面(与界面「留空表示全部」一致)
#[test]
fn rotate_all_pages_when_empty() {
    let src = make_pdf(3);
    let out = src.parent().unwrap().join("rotated-all.pdf");

    rotate_pages(&src, &[], 90, &out).expect("空页码旋转应成功");
    let doc = lopdf::Document::load(&out).expect("旋转结果可重新加载");
    for (idx, id) in doc.get_pages() {
        let rotate = doc
            .get_object(id)
            .expect("页对象存在")
            .as_dict()
            .expect("页为字典")
            .get(b"Rotate")
            .and_then(|o| o.as_i64())
            .unwrap_or(0);
        assert_eq!(rotate, 90, "第 {idx} 页应旋转 90 度");
    }
}

/// 未指定页码时删除应报错,而不是静默输出未删除任何页的副本
#[test]
fn delete_without_pages_fails() {
    let src = make_pdf(3);
    let out = src.parent().unwrap().join("delete-empty.pdf");
    assert!(delete_pages(&src, &[], &out).is_err());
    assert!(!out.exists());
}

/// 删除超出总页数的页码应报错
#[test]
fn delete_out_of_range_fails() {
    let src = make_pdf(3);
    let out = src.parent().unwrap().join("delete-oob.pdf");
    assert!(delete_pages(&src, &[4], &out).is_err());
    assert!(!out.exists());
}

/// 删除全部页面应被拒绝:0 页 PDF 无法正常打开,且会让文件变成无用的空壳
#[test]
fn delete_all_pages_fails() {
    let src = make_pdf(2);
    let out = src.parent().unwrap().join("delete-all.pdf");
    let err = delete_pages(&src, &[1, 2], &out).expect_err("删除全部页面应报错");
    assert!(
        format!("{err}").contains("至少保留"),
        "错误信息应说明至少保留 1 页,实际: {err}"
    );
    assert!(!out.exists(), "失败时不应留下 0 页 PDF");
}

/// 删除/提取/插入/重排后页数正确
#[test]
fn delete_extract_insert_reorder() {
    let src = make_pdf(4);
    let dir = src.parent().unwrap();

    // 删除第 2 页 → 3 页
    let out = dir.join("deleted.pdf");
    delete_pages(&src, &[2], &out).expect("删除应成功");
    assert_eq!(page_count(&out), 3);

    // 提取第 1、4 页 → 2 页
    let out = dir.join("extracted.pdf");
    extract_pages(&src, &[1, 4], &out).expect("提取应成功");
    assert_eq!(page_count(&out), 2);

    // 在第 1 页后插入 extracted.pdf(2 页) → 2 + 2 = ... 基于当前 4 页源再插
    let out = dir.join("inserted.pdf");
    insert_pages(&src, &dir.join("extracted.pdf"), 1, &out).expect("插入应成功");
    assert_eq!(page_count(&out), 6, "4 页 + 插入 2 页 = 6 页");

    // 重排为 3,1,2,4 → 4 页
    let out = dir.join("reordered.pdf");
    reorder_pages(&src, &[3, 1, 2, 4], &out).expect("重排应成功");
    assert_eq!(page_count(&out), 4);
}

// ===== 水印与压缩 =====

/// 构造一个 Resources 里已占用 F1 / GS1 / Im1 的页面,用于验证水印不会覆盖原有资源
fn make_resource_rich_pdf() -> std::path::PathBuf {
    let dir = make_pdf(1).parent().unwrap().to_path_buf();
    let path = dir.join("resource-rich.pdf");

    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let content_id = doc.add_object(lopdf::Stream::new(
        dictionary! {},
        b"BT /F1 12 Tf 72 720 Td (Origin) Tj ET".to_vec(),
    ));
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Times-Roman",
    });
    let img_id = doc.add_object(lopdf::Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => 10,
            "Height" => 10,
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
        },
        vec![0u8; 10 * 10 * 3],
    ));
    let gs_id = doc.add_object(dictionary! {
        "Type" => "ExtGState",
        "CA" => 1.0,
        "ca" => 1.0,
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
        "XObject" => dictionary! { "Im1" => img_id },
        "ExtGState" => dictionary! { "GS1" => gs_id },
    });
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "Resources" => resources_id,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => 1,
            "Kids" => vec![Object::Reference(page_id)],
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    doc.save(&path).expect("保存含既有资源的 PDF");
    path
}

fn deref_dict<'a>(doc: &'a lopdf::Document, obj: &'a Object) -> &'a lopdf::Dictionary {
    let obj = match obj {
        Object::Reference(id) => doc.get_object(*id).unwrap(),
        other => other,
    };
    match obj {
        // XObject 是带字典的流对象,资源条目同样要取其字典
        Object::Stream(stream) => &stream.dict,
        other => other.as_dict().unwrap(),
    }
}

/// 取第 1 页 Resources 下的指定子字典(Font / XObject / ExtGState)
fn page1_resource<'a>(doc: &'a lopdf::Document, category: &[u8]) -> &'a lopdf::Dictionary {
    let page_id = *doc.get_pages().get(&1).expect("第 1 页存在");
    let page = doc.get_object(page_id).unwrap().as_dict().unwrap();
    let resources = deref_dict(doc, page.get(b"Resources").unwrap());
    deref_dict(doc, resources.get(category).unwrap())
}

/// 水印与原文共用 Resources:固定资源名 /F1、/GS1、/Im1 会覆盖原资源,
/// 导致正文换字体、原图被水印图替换。这里验证原有映射保持不动,水印另起新名。
#[test]
fn watermark_does_not_shadow_existing_resources() {
    let src = make_resource_rich_pdf();
    let dir = src.parent().unwrap().to_path_buf();

    // 文字水印:Font/F1 与 ExtGState/GS1 应保持原样
    let text_out = dir.join("rich-text-watermarked.pdf");
    add_text_watermark(&src, "WATERMARK", &WatermarkOptions::default(), &text_out)
        .expect("文字水印应成功");
    let doc = lopdf::Document::load(&text_out).expect("加载文字水印结果");
    let fonts = page1_resource(&doc, b"Font");
    let f1 = deref_dict(&doc, fonts.get(b"F1").unwrap());
    let base_font = f1.get(b"BaseFont").unwrap().as_name().unwrap();
    assert_eq!(
        String::from_utf8_lossy(base_font),
        "Times-Roman",
        "原文 /F1 字体不应被水印字体覆盖"
    );
    assert!(fonts.len() >= 2, "水印字体应注册在独立的新资源名下");
    let ext = page1_resource(&doc, b"ExtGState");
    let gs1 = deref_dict(&doc, ext.get(b"GS1").unwrap());
    let ca = gs1.get(b"CA").unwrap().as_float().unwrap();
    assert!(
        (ca - 1.0).abs() < 1e-6,
        "原文 /GS1 的 CA 不应被水印透明度覆盖,实际 {ca}"
    );

    // 图片水印:原 XObject/Im1 不应被水印图替换,且 opacity 必须落到 ExtGState
    let logo_path = dir.join("rich-logo.png");
    let mut logo = image::RgbaImage::new(8, 8);
    for p in logo.pixels_mut() {
        *p = image::Rgba([0, 120, 220, 255]);
    }
    logo.save(&logo_path).expect("保存水印图");
    let img_out = dir.join("rich-image-watermarked.pdf");
    let opts = WatermarkOptions {
        opacity: 0.4,
        ..WatermarkOptions::default()
    };
    add_image_watermark(&src, &logo_path, &opts, &img_out).expect("图片水印应成功");
    let doc = lopdf::Document::load(&img_out).expect("加载图片水印结果");
    let xobjects = page1_resource(&doc, b"XObject");
    let im1 = deref_dict(&doc, xobjects.get(b"Im1").unwrap());
    assert_eq!(
        im1.get(b"Width").unwrap().as_i64().unwrap(),
        10,
        "原文 /Im1 不应被水印图替换"
    );
    let ext = page1_resource(&doc, b"ExtGState");
    let gs1 = deref_dict(&doc, ext.get(b"GS1").unwrap());
    let original_ca = gs1.get(b"CA").unwrap().as_float().unwrap();
    assert!(
        (original_ca - 1.0).abs() < 1e-6,
        "原 /GS1 透明度应保持 1.0,实际 {original_ca}"
    );
    let has_opacity = ext.iter().any(|(_, obj)| {
        deref_dict(&doc, obj)
            .get(b"CA")
            .ok()
            .and_then(|o| o.as_float().ok())
            .is_some_and(|v| (v - 0.4).abs() < 1e-6)
    });
    assert!(has_opacity, "图片水印的 opacity 应注册为 ExtGState");
}

/// 文字水印:处理后可重新加载且页数不变
#[test]
fn text_watermark_keeps_pages() {
    let src = make_pdf(2);
    let out = src.parent().unwrap().join("watermarked.pdf");

    add_text_watermark(&src, "密档大师 测试", &WatermarkOptions::default(), &out)
        .expect("添加水印应成功");
    assert_eq!(page_count(&out), 2, "水印不应改变页数");
}

/// 不透明度需注册为 ExtGState:否则 content 里的 /GS1 gs 引用不存在,滑块无效
#[test]
fn text_watermark_registers_opacity() {
    let src = make_pdf(1);
    let out = src.parent().unwrap().join("watermarked-opacity.pdf");
    let opts = WatermarkOptions {
        opacity: 0.25,
        ..WatermarkOptions::default()
    };
    add_text_watermark(&src, "测试", &opts, &out).expect("添加水印应成功");

    let doc = lopdf::Document::load(&out).expect("结果可重新加载");
    let page_id = *doc.get_pages().get(&1).expect("第 1 页存在");
    let page = doc.get_object(page_id).expect("页对象存在");
    let resources = match page.as_dict().unwrap().get(b"Resources").unwrap() {
        lopdf::Object::Reference(id) => doc.get_object(*id).expect("Resources 对象存在"),
        other => other,
    };
    let ext = match resources.as_dict().unwrap().get(b"ExtGState").unwrap() {
        lopdf::Object::Reference(id) => doc.get_object(*id).expect("ExtGState 对象存在"),
        other => other,
    };
    let gs1 = match ext.as_dict().unwrap().get(b"GS1").unwrap() {
        lopdf::Object::Reference(id) => doc.get_object(*id).expect("GS1 对象存在"),
        other => other,
    };
    let ca = gs1
        .as_dict()
        .unwrap()
        .get(b"CA")
        .unwrap()
        .as_float()
        .unwrap();
    assert!((ca - 0.25).abs() < 1e-6, "CA 应为 0.25,实际 {ca}");
}

/// 压缩:处理后可重新加载
#[test]
fn compress_reloads() {
    let src = make_pdf(2);
    let out = src.parent().unwrap().join("compressed.pdf");

    let stats = compress(&src, &out, 6).expect("压缩应成功");
    let _ = stats;
    assert_eq!(page_count(&out), 2);
}

// ===== 加密/解密(pikepdf) =====

/// AES-256 加密:无密码打不开、正确密码可读、页数不变、权限位真的落盘
#[test]
fn encrypt_aes256_locks_and_applies_permissions() {
    if !pycrypt_available() {
        eprintln!("跳过:pycore/pikepdf 不可用");
        return;
    }
    let src = make_pdf(3);
    let enc = src.parent().unwrap().join("encrypted.pdf");

    let report = encrypt_aes256(
        &src,
        &enc,
        &EncryptOptions {
            user_password: "测试密码".into(),
            owner_password: Some("owner-pw".into()),
            permissions: Permissions {
                allow_print: true,
                allow_copy: false,
                allow_modify: false,
                allow_annotate: false,
            },
            bits: 256,
            input_password: None,
        },
    )
    .expect("加密应成功");

    assert!(Path::new(&enc).is_file(), "输出文件应存在");
    assert_eq!(report.revision, 6, "AES-256 对应 PDF 加密修订号 R6");
    assert_eq!(report.pages, 3, "加密不应改变页数");
    assert!(!report.permissions.allow_copy, "复制权限应确实被禁止");

    // 无密码 → 锁死
    let locked = probe_encryption(&enc, "").expect("探测应成功");
    assert!(locked.encrypted, "输出应被标记为已加密");
    assert!(locked.locked, "不给密码必须读不到内容");

    // 正确密码 → 可读,页数与权限位一致
    let opened = probe_encryption(&enc, "测试密码").expect("探测应成功");
    assert!(!opened.locked, "正确密码应能打开");
    assert_eq!(opened.pages, 3);
    assert!(!opened.permissions.allow_copy);
    assert!(opened.permissions.allow_print);
}

/// 解密回原页数,且输出不再需要密码
#[test]
fn decrypt_restores_plaintext() {
    if !pycrypt_available() {
        eprintln!("跳过:pycore/pikepdf 不可用");
        return;
    }
    let src = make_pdf(3);
    let enc = src.parent().unwrap().join("encrypted.pdf");
    let dec = src.parent().unwrap().join("decrypted.pdf");

    encrypt_aes256(
        &src,
        &enc,
        &EncryptOptions {
            user_password: "测试密码".into(),
            owner_password: Some("owner".into()),
            ..Default::default()
        },
    )
    .expect("加密应成功");

    let report = decrypt(&enc, &dec, "测试密码").expect("解密应成功");
    assert!(report.was_encrypted, "源文件应确实是加密的");
    assert_eq!(page_count(&dec), 3, "解密后页数应与原文一致");

    let plain = probe_encryption(&dec, "").expect("探测应成功");
    assert!(!plain.encrypted, "解密后不应残留加密");
    assert!(!plain.locked);
}

/// 错误密码必须被明确拒绝,且不留下半成品文件
#[test]
fn decrypt_with_wrong_password_fails_cleanly() {
    if !pycrypt_available() {
        eprintln!("跳过:pycore/pikepdf 不可用");
        return;
    }
    let src = make_pdf(2);
    let enc = src.parent().unwrap().join("encrypted.pdf");
    let dec = src.parent().unwrap().join("wrong-dec.pdf");

    encrypt_aes256(
        &src,
        &enc,
        &EncryptOptions {
            user_password: "right-pw".into(),
            ..Default::default()
        },
    )
    .expect("加密应成功");

    let err = decrypt(&enc, &dec, "wrong-pw").expect_err("错误密码必须失败");
    let msg = format!("{err}");
    assert!(msg.contains("密码"), "错误信息应指向密码问题,实际: {msg}");
    assert!(!Path::new(&dec).exists(), "失败时不应留下输出文件");
}

// ===== 渲染(pdfium) =====

/// 全页渲染:输出文件数与页数一致
#[test]
fn render_all_pages_count() {
    if !ensure_pdfium() {
        eprintln!("跳过:未找到 pdfium 动态库(可设置 PDFIUM_PATH)");
        return;
    }
    let src = make_pdf(3);
    let out_dir = src.parent().unwrap().join("rendered");

    let pages = pdf_docvault_core::pdf::render_all_pages(
        &src,
        &out_dir,
        96,
        image::ImageFormat::Png,
        None,
        |_, _| {},
    )
    .expect("渲染应成功");
    assert_eq!(pages.len(), 3);
    assert!(pages.iter().all(|p| p.is_file()));
}

/// 按页列表渲染:输出文件数 == 去重后页数,文件名保留原始页码
#[test]
fn render_pages_by_list() {
    if !ensure_pdfium() {
        eprintln!("跳过:未找到 pdfium 动态库(可设置 PDFIUM_PATH)");
        return;
    }
    let src = make_pdf(5);
    let out_dir = src.parent().unwrap().join("rendered-by-list");

    // 乱序 + 重复传入,应被去重排序后按原始页码命名(不重新编号)
    let pages = pdf_docvault_core::pdf::render_all_pages(
        &src,
        &out_dir,
        96,
        image::ImageFormat::Png,
        Some(&[3, 1, 3]),
        |_, _| {},
    )
    .expect("按页列表渲染应成功");
    assert_eq!(pages.len(), 2, "去重后应输出 2 张图片");
    assert_eq!(
        pages[0],
        out_dir.join("fixture-5p_page-0001.png"),
        "首张图片文件名应为源文件名 + 原始页码 1"
    );
    assert_eq!(
        pages[1],
        out_dir.join("fixture-5p_page-0003.png"),
        "次张图片文件名应为源文件名 + 原始页码 3"
    );
}

/// 同一输出目录导出两个 PDF 时不能互相覆盖:文件名必须带源文件名
#[test]
fn render_different_pdfs_do_not_overwrite() {
    if !ensure_pdfium() {
        eprintln!("跳过:未找到 pdfium 动态库(可设置 PDFIUM_PATH)");
        return;
    }
    let a = make_pdf(1);
    let b = make_pdf(2);
    // 复制成与 a 同名(fixture-1p.pdf)的文档,验证「同名不同内容」也不会串
    let b_same_name = b.parent().unwrap().join("fixture-1p.pdf");
    std::fs::copy(&b, &b_same_name).expect("复制 fixture");
    let out_dir = a.parent().unwrap().join("rendered-shared");

    let first = pdf_docvault_core::pdf::render_all_pages(
        &a,
        &out_dir,
        96,
        image::ImageFormat::Png,
        None,
        |_, _| {},
    )
    .expect("第一次导出应成功");
    let second = pdf_docvault_core::pdf::render_all_pages(
        &b_same_name,
        &out_dir,
        96,
        image::ImageFormat::Png,
        None,
        |_, _| {},
    )
    .expect("第二次导出应成功");

    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 2);
    for p in first.iter().chain(second.iter()) {
        assert!(p.is_file(), "所有导出图片都应存在: {}", p.display());
    }
    assert_ne!(first[0], second[0], "两个 PDF 的第 1 页不能写到同一个文件");
}

/// 先按页导出、再整本导出:此前只看第 1 页判定撞名,会把已导出的第 3 页覆盖
#[test]
fn render_partial_then_full_keeps_previous() {
    if !ensure_pdfium() {
        eprintln!("跳过:未找到 pdfium 动态库(可设置 PDFIUM_PATH)");
        return;
    }
    let src = make_pdf(3);
    let out_dir = src.parent().unwrap().join("rendered-partial-then-full");

    let partial = pdf_docvault_core::pdf::render_all_pages(
        &src,
        &out_dir,
        96,
        image::ImageFormat::Png,
        Some(&[3]),
        |_, _| {},
    )
    .expect("单页导出应成功");
    assert_eq!(partial.len(), 1);

    let full = pdf_docvault_core::pdf::render_all_pages(
        &src,
        &out_dir,
        96,
        image::ImageFormat::Png,
        None,
        |_, _| {},
    )
    .expect("整本导出应成功");
    assert_eq!(full.len(), 3);
    assert!(
        !full.contains(&partial[0]),
        "整本导出不应覆盖此前单独导出的第 3 页"
    );
    assert!(partial[0].is_file(), "此前的单页导出应仍保留");
}

/// 按页列表渲染:页码越界应返回明确错误
#[test]
fn render_pages_out_of_range_rejected() {
    if !ensure_pdfium() {
        eprintln!("跳过:未找到 pdfium 动态库(可设置 PDFIUM_PATH)");
        return;
    }
    let src = make_pdf(3);
    let out_dir = src.parent().unwrap().join("rendered-oob");

    let result = pdf_docvault_core::pdf::render_all_pages(
        &src,
        &out_dir,
        96,
        image::ImageFormat::Png,
        Some(&[4]),
        |_, _| {},
    );
    assert!(result.is_err(), "页码 4 超出共 3 页应报错");
}

// ===== 页面树结构 =====

/// 提取:继承自中间 Pages 节点的属性要落到页面自身,/Parent 指向新的根,
/// 否则旧页面树仍被页面引用,未选页无法被 prune 清理
#[test]
fn extract_flattens_nested_page_tree() {
    let src = make_nested_pdf(4);
    let out = src.parent().unwrap().join("nested-extract.pdf");
    extract_pages(&src, &[2, 3], &out).expect("提取应成功");
    assert_eq!(page_count(&out), 2);

    let doc = lopdf::Document::load(&out).expect("加载提取结果");
    let root = doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    for page_id in doc.get_pages().values() {
        let dict = doc.get_object(*page_id).unwrap().as_dict().unwrap();
        assert_eq!(
            dict.get(b"Parent").unwrap().as_reference().unwrap(),
            root,
            "页面 /Parent 应指向新页面树根"
        );
        assert!(dict.get(b"MediaBox").is_ok(), "页面应继承到 MediaBox");
        assert!(dict.get(b"Resources").is_ok(), "页面应继承到 Resources");
    }
}

/// 提取时重复页码只应输出一次:同一页面对象在 Kids 里出现两次是非法结构
#[test]
fn extract_duplicate_pages_once() {
    let src = make_pdf(3);
    let out = src.parent().unwrap().join("extracted-dedup.pdf");
    extract_pages(&src, &[1, 1, 2], &out).expect("提取应成功");
    assert_eq!(page_count(&out), 2, "重复页码只应提取一次");
}

/// 旋转要把继承来的 /Rotate 一并累加,不能当成 0 覆盖掉原有旋转
#[test]
fn rotate_keeps_inherited_rotation() {
    let src = make_nested_pdf(2);
    let out = src.parent().unwrap().join("nested-rotated.pdf");
    rotate_pages(&src, &[1], 90, &out).expect("旋转应成功");

    let doc = lopdf::Document::load(&out).expect("加载旋转结果");
    let page_id = doc.get_pages()[&1];
    let page = doc.get_object(page_id).unwrap().as_dict().unwrap();
    let rotate = page.get(b"Rotate").unwrap().as_i64().unwrap();
    assert_eq!(rotate, 180, "继承的 90° 应与本次 90° 累加");
}

/// 插入:doc1 与 doc2 的合并页面都要挂到 doc1 的根下,且保留各自尺寸/资源
#[test]
fn insert_flattens_nested_page_tree() {
    let base = make_pdf(2);
    let nested = make_nested_pdf(2);
    let out = base.parent().unwrap().join("insert-nested.pdf");
    insert_pages(&base, &nested, 1, &out).expect("插入应成功");
    assert_eq!(page_count(&out), 4);

    let doc = lopdf::Document::load(&out).expect("加载插入结果");
    let root = doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    for page_id in doc.get_pages().values() {
        let dict = doc.get_object(*page_id).unwrap().as_dict().unwrap();
        assert_eq!(
            dict.get(b"Parent").unwrap().as_reference().unwrap(),
            root,
            "插入后所有页面都应挂到同一根节点"
        );
        assert!(dict.get(b"MediaBox").is_ok(), "插入页应保留 MediaBox");
    }
}

/// 合并嵌套页面树:所有页面同样要拿到自身属性并挂到合并后的根
#[test]
fn merge_flattens_nested_page_tree() {
    let a = make_nested_pdf(2);
    let b = make_nested_pdf(3);
    let out = a.parent().unwrap().join("merge-nested.pdf");
    let stats = merge(&[a, b], &out).expect("合并应成功");
    assert_eq!(stats.pages, 5);

    let doc = lopdf::Document::load(&out).expect("加载合并结果");
    let root = doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    for page_id in doc.get_pages().values() {
        let dict = doc.get_object(*page_id).unwrap().as_dict().unwrap();
        assert_eq!(
            dict.get(b"Parent").unwrap().as_reference().unwrap(),
            root,
            "合并后所有页面都应挂到新根节点"
        );
        assert!(dict.get(b"MediaBox").is_ok(), "合并页应保留 MediaBox");
    }
}

/// 拆分只应保留选中的页面对象:此前旧页面树仍可经 /Parent 到达,每份输出都塞进整本
#[test]
fn split_drops_unselected_page_objects() {
    let src = make_pdf(5);
    let out_dir = src.parent().unwrap().join("split-prune");
    let stats = split(
        &src,
        &SplitMode::ByRanges {
            ranges: vec![PageRange { start: 1, end: 2 }],
        },
        &out_dir,
    )
    .expect("拆分应成功");

    let src_objects = lopdf::Document::load(&src).unwrap().objects.len();
    let out_objects = lopdf::Document::load(&stats.paths[0])
        .unwrap()
        .objects
        .len();
    assert!(
        out_objects < src_objects,
        "拆分结果不应带上未选页面的对象: 输出 {} 个,源 {} 个",
        out_objects,
        src_objects
    );
}

// ===== 水印 =====

/// 页面把 /Resources 写在中间 Pages 节点上由叶子继承时,水印也要能正常写入
#[test]
fn text_watermark_handles_inherited_resources() {
    let src = make_nested_pdf(2);
    let out = src.parent().unwrap().join("nested-watermark.pdf");
    add_text_watermark(&src, "PDF-DocVault", &WatermarkOptions::default(), &out)
        .expect("继承 Resources 的页面也应能加水印");
    assert_eq!(page_count(&out), 2);
}

/// 透明背景的水印图必须合成到白底:此前直接丢 alpha,透明区域会变成黑块
#[test]
fn image_watermark_composites_transparency_on_white() {
    let src = make_pdf(1);
    let dir = src.parent().unwrap();
    let logo_path = dir.join("logo.png");
    let mut logo = image::RgbaImage::new(16, 16);
    for (x, y, p) in logo.enumerate_pixels_mut() {
        *p = if x < 8 && y < 8 {
            image::Rgba([220, 0, 0, 255])
        } else {
            image::Rgba([0, 0, 0, 0]) // 全透明
        };
    }
    logo.save(&logo_path).expect("保存透明 PNG");

    let out = dir.join("logo-watermarked.pdf");
    add_image_watermark(&src, &logo_path, &WatermarkOptions::default(), &out)
        .expect("图片水印应成功");

    // 取出嵌入的 JPEG 解码检查像素
    let doc = lopdf::Document::load(&out).expect("加载水印结果");
    let stream = doc
        .objects
        .values()
        .find_map(|o| {
            let s = o.as_stream().ok()?;
            // 源 PDF 本身也是图片页(DCTDecode),按水印图的尺寸挑选
            if s.dict.get(b"Width").ok()?.as_i64().ok()? != 16
                || s.dict.get(b"Height").ok()?.as_i64().ok()? != 16
            {
                return None;
            }
            let filter = s.dict.get(b"Filter").ok()?.as_name().ok()?;
            if filter == b"DCTDecode" {
                Some(s)
            } else {
                None
            }
        })
        .expect("应嵌入 JPEG 水印图片");
    let decoded = image::load_from_memory(&stream.content)
        .expect("解码水印 JPEG")
        .to_rgb8();
    let transparent = decoded.get_pixel(15, 15);
    assert!(
        transparent[0] > 200 && transparent[1] > 200 && transparent[2] > 200,
        "透明区域应合成成白色,而不是黑色: {:?}",
        transparent
    );
    // JPEG 会做色度抽样,取红色区域中心避免边缘混色
    let opaque = decoded.get_pixel(3, 3);
    assert!(
        opaque[0] > 120 && opaque[1] < 120 && opaque[2] < 120,
        "不透明红色区域应保留: {:?}",
        opaque
    );
}

/// 空白水印文本不能静默产出「无效果但提示成功」的文件
#[test]
fn empty_text_watermark_fails() {
    let src = make_pdf(1);
    let out = src.parent().unwrap().join("watermarked-empty.pdf");
    let err = add_text_watermark(&src, "   ", &WatermarkOptions::default(), &out)
        .expect_err("空白水印文本应报错");
    assert!(
        format!("{err}").contains("水印文本"),
        "错误信息应指向水印文本,实际: {err}"
    );
    assert!(!out.exists(), "失败时不应留下输出文件");
}

/// 压缩同样必须先解密:prune/renumber 会改变对象号,而加密流按对象号派生密钥,
/// 不解密直接压缩会产出打不开的成品
#[test]
fn compress_locked_pdf_fails_clearly() {
    let src = make_pdf(1);
    let locked = src.parent().unwrap().join("locked-compress.pdf");
    let mut doc = lopdf::Document::load(&src).expect("加载测试 PDF");
    // 伪造带 /Encrypt 的文档:空密码无法通过密钥校验,用于验证拒绝逻辑
    let enc = doc.add_object(dictionary! {
        "Filter" => "Standard",
        "V" => 1,
    });
    doc.trailer.set("Encrypt", enc);
    doc.save(&locked).expect("写出加密占位 PDF");

    let out = src.parent().unwrap().join("compressed-locked.pdf");
    let err = compress(&locked, &out, 6).expect_err("加密文件应被拒绝");
    assert!(
        format!("{err}").contains("已加密"),
        "错误信息应提示先解密,实际为: {err}"
    );
    assert!(!out.exists(), "失败时不应留下输出文件");
}

/// 删除页同样会 prune/renumber,加密文档必须先解密,否则对象号变化会让内容全部无法解密
#[test]
fn delete_locked_pdf_fails_clearly() {
    let src = make_pdf(2);
    let locked = src.parent().unwrap().join("locked-delete.pdf");
    let mut doc = lopdf::Document::load(&src).expect("加载测试 PDF");
    let enc = doc.add_object(dictionary! {
        "Filter" => "Standard",
        "V" => 1,
    });
    doc.trailer.set("Encrypt", enc);
    doc.save(&locked).expect("写出加密占位 PDF");

    let out = src.parent().unwrap().join("deleted-locked.pdf");
    let err = delete_pages(&locked, &[1], &out).expect_err("加密文件应被拒绝");
    assert!(
        format!("{err}").contains("已加密"),
        "错误信息应提示先解密,实际为: {err}"
    );
    assert!(!out.exists(), "失败时不应留下输出文件");
}

/// /Contents 是可选键:没有内容的空白页也属于合法 PDF,不能因此让整份水印失败
#[test]
fn watermark_page_without_contents_succeeds() {
    let src = make_pdf(1);
    let blank = src.parent().unwrap().join("blank-page.pdf");
    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => 1,
            "Kids" => vec![Object::Reference(page_id)],
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    doc.save(&blank).expect("写出无 Contents 的页面");

    let out = src.parent().unwrap().join("watermarked-blank.pdf");
    add_text_watermark(&blank, "Test", &WatermarkOptions::default(), &out)
        .expect("空白页也应能加水印");
    let doc = lopdf::Document::load(&out).expect("加载水印结果");
    let page_id = *doc.get_pages().get(&1).expect("第 1 页存在");
    assert!(
        doc.get_object(page_id)
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"Contents")
            .is_ok(),
        "水印内容应挂到页面 /Contents"
    );
    assert_eq!(page_count(&out), 1);
}

/// 负值 /Rotate(-180 是合法且部分工具会写出的值)与本次旋转相加后应归一化到 0..360
#[test]
fn rotate_negative_angle_normalized() {
    let src = make_pdf(1);
    let neg = src.parent().unwrap().join("negative-rotate.pdf");
    let mut doc = lopdf::Document::load(&src).expect("加载测试 PDF");
    let page_id = *doc.get_pages().get(&1).expect("第 1 页存在");
    doc.get_object_mut(page_id)
        .expect("页对象存在")
        .as_dict_mut()
        .expect("页为字典")
        .set("Rotate", Object::Integer(-180));
    doc.save(&neg).expect("写出负角度 PDF");

    let out = src.parent().unwrap().join("rotated-negative.pdf");
    rotate_pages(&neg, &[1], 90, &out).expect("旋转应成功");
    let doc = lopdf::Document::load(&out).expect("加载旋转结果");
    let page_id = *doc.get_pages().get(&1).expect("第 1 页存在");
    let rotate = doc
        .get_object(page_id)
        .expect("页对象存在")
        .as_dict()
        .expect("页为字典")
        .get(b"Rotate")
        .and_then(|o| o.as_i64())
        .unwrap_or(0);
    assert_eq!(rotate, 270, "负角度应归一化为 270,而不是 -90");
}
