//! PDF 全流程集成测试:merge → split → pages → watermark → compress → 加密 → 渲染。
//!
//! 覆盖规划书阶段 I 要求:合并后页数正确、拆分后可重新 load、
//! 旋转后 /Rotate 正确、加密后 qpdf 可解密。
//! 外部依赖缺失(pdfium/qpdf)时对应测试自动跳过。

mod common;

use common::{ensure_pdfium, make_pdf, page_count, qpdf_available};
use pdf_docvault_core::pdf::{
    add_text_watermark, compress, delete_pages, encrypt_aes256, extract_pages, insert_pages,
    merge, decrypt, reorder_pages, rotate_pages, split, EncryptOptions, SplitMode, WatermarkOptions,
};
use std::path::Path;

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

/// 文字水印:处理后可重新加载且页数不变
#[test]
fn text_watermark_keeps_pages() {
    let src = make_pdf(2);
    let out = src.parent().unwrap().join("watermarked.pdf");

    add_text_watermark(&src, "密档大师 测试", &WatermarkOptions::default(), &out)
        .expect("添加水印应成功");
    assert_eq!(page_count(&out), 2, "水印不应改变页数");
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

// ===== 加密(qpdf) =====

/// AES-256 加密后 qpdf 解密回原页数(规划书验证点)
#[test]
fn encrypt_then_decrypt_roundtrip() {
    if !qpdf_available() {
        eprintln!("跳过:系统未安装 qpdf");
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
    assert!(Path::new(&enc).is_file());

    // 加密后的文件无法用 lopdf 直接打开(已加密),验证方式:qpdf 解密回明文
    decrypt(&enc, &dec, "测试密码").expect("解密应成功");
    assert_eq!(page_count(&dec), 3, "解密后页数应与原文一致");
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
        out_dir.join("page-0001.png"),
        "首张图片文件名应为原始页码 1"
    );
    assert_eq!(
        pages[1],
        out_dir.join("page-0003.png"),
        "次张图片文件名应为原始页码 3"
    );
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
