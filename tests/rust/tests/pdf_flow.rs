//! PDF 全流程集成测试:merge → split → pages → watermark → compress → 加密 → 渲染。
//!
//! 覆盖规划书阶段 I 要求:合并后页数正确、拆分后可重新 load、
//! 旋转后 /Rotate 正确、加密后能解密回原页数。
//! 外部依赖缺失(pdfium / pycore-pikepdf)时对应测试自动跳过。

mod common;

use common::{ensure_pdfium, make_pdf, page_count, pycrypt_available};
use lopdf::dictionary;
use pdf_docvault_core::pdf::{
    add_text_watermark, compress, decrypt, delete_pages, encrypt_aes256, extract_pages,
    insert_pages, merge, probe_encryption, reorder_pages, rotate_pages, split, EncryptOptions,
    Permissions, SplitMode, WatermarkOptions,
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
