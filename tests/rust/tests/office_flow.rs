//! Word → PDF 批量转换集成测试
//!
//! fixture 动态生成:pycore venv 的 python-docx 造含中文+图片的 docx;
//! 无 LibreOffice 或无 venv 时自动跳过(CI 环境无依赖不报错)。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

mod common;

use common::page_count;
use pdf_docvault_core::office::{self, WordQuality};

/// 定位 pycore venv python(测试 fixture 生成器)
fn venv_python() -> Option<PathBuf> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../pycore/.venv/bin/python");
    let p = p.canonicalize().ok()?;
    p.is_file().then_some(p)
}

/// 系统是否具备 LibreOffice
fn libreoffice_available() -> bool {
    let Ok(path) = std::env::var("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        dir.join("soffice").is_file() || dir.join("libreoffice").is_file()
    })
}

/// 用 python-docx 在 `dir` 生成 `name`,内含中文段落与一张图片
fn make_docx(python: &Path, dir: &Path, name: &str) -> PathBuf {
    let out = dir.join(name);
    // 内联脚本:段落 + PIL 纯色图 + 标题
    let script = format!(
        r#"
import sys
from docx import Document
from docx.shared import Inches
from PIL import Image
img_p = sys.argv[2]
Image.new("RGB", (1000, 700), color=(30, 90, 168)).save(img_p)
doc = Document()
doc.add_heading("集成测试文档", 0)
doc.add_paragraph("中文段落:Word 转 PDF 批量转换验证。")
doc.add_picture(img_p, width=Inches(4))
doc.save(sys.argv[1])
"#
    );
    let status = std::process::Command::new(python)
        .args(["-c", &script])
        .arg(&out)
        .arg(dir.join(format!("{name}.png")))
        .status()
        .expect("运行 fixture 生成脚本失败");
    assert!(status.success(), "fixture 生成失败");
    out
}

/// 前置条件不满足时的统一跳过判定(返回 true = 已跳过)
fn skip_if_unready(python: Option<PathBuf>) -> bool {
    if python.is_none() {
        eprintln!("跳过:未找到 pycore/.venv");
        return true;
    }
    if !libreoffice_available() {
        eprintln!("跳过:未检测到 LibreOffice");
        return true;
    }
    false
}

/// 批量转换:两个 docx 全部成功,产物可 load 且页数为 2
#[test]
fn batch_convert_succeeds() {
    let Some(python) = venv_python() else {
        eprintln!("跳过:未找到 pycore/.venv");
        return;
    };
    if !libreoffice_available() {
        eprintln!("跳过:未检测到 LibreOffice");
        return;
    }

    let dir = tempfile::tempdir().expect("创建临时目录");
    let d1 = make_docx(&python, dir.path(), "报告一.docx");
    let d2 = make_docx(&python, dir.path(), "report-2.docx");
    let out_dir = dir.path().join("pdf");

    let report = office::word_to_pdf(
        &[d1, d2],
        &out_dir,
        WordQuality::High,
        300,
        None::<fn(u32, u32)>,
    )
    .expect("批量转换应成功");

    assert_eq!(report.success_count(), 2, "两个文件均应成功");
    assert_eq!(report.fail_count(), 0);
    for item in &report.succeeded {
        assert!(Path::new(&item.1).is_file(), "PDF 应存在: {}", item.1);
        assert_eq!(page_count(Path::new(&item.1)), 1, "PDF 页数应与源文档一致");
    }
}

/// 三个质量等级均产出 PDF,且低等级图片被降采样(体积应小于高等级)
#[test]
fn quality_levels_produce_pdfs() {
    let Some(python) = venv_python() else {
        eprintln!("跳过:未找到 pycore/.venv");
        return;
    };
    if !libreoffice_available() {
        eprintln!("跳过:未检测到 LibreOffice");
        return;
    }

    let dir = tempfile::tempdir().expect("创建临时目录");
    let src = make_docx(&python, dir.path(), "quality.docx");

    let mut sizes: Vec<u64> = Vec::new();
    for quality in [WordQuality::High, WordQuality::Medium, WordQuality::Low] {
        let out_dir = dir.path().join(format!("{:?}", quality));
        let report =
            office::word_to_pdf(&[src.clone()], &out_dir, quality, 300, None::<fn(u32, u32)>)
                .expect("转换应成功");
        assert_eq!(report.success_count(), 1);
        let pdf = Path::new(&report.succeeded[0].1);
        sizes.push(pdf.metadata().unwrap().len());
    }

    // 低等级(图片 96 DPI)体积应严格小于高等级(原图)
    assert!(
        sizes[2] < sizes[0],
        "低等级 PDF({}B)应小于高等级({}B)",
        sizes[2],
        sizes[0]
    );
}

/// 进度回调:2 文件至少触发 2 次且最终序号等于总数
#[test]
fn progress_callback_fires() {
    let Some(python) = venv_python() else {
        eprintln!("跳过:未找到 pycore/.venv");
        return;
    };
    if !libreoffice_available() {
        eprintln!("跳过:未检测到 LibreOffice");
        return;
    }

    let dir = tempfile::tempdir().expect("创建临时目录");
    let d1 = make_docx(&python, dir.path(), "p1.docx");
    let d2 = make_docx(&python, dir.path(), "p2.docx");
    let out_dir = dir.path().join("out");

    let last = Arc::new(AtomicU32::new(0));
    let last2 = Arc::clone(&last);
    let report = office::word_to_pdf(
        &[d1, d2],
        &out_dir,
        WordQuality::Medium,
        300,
        Some(move |current: u32, _total: u32| {
            last2.store(current, Ordering::SeqCst);
        }),
    )
    .expect("转换应成功");

    assert_eq!(report.success_count(), 2);
    assert_eq!(last.load(Ordering::SeqCst), 2, "最终进度序号应为 2");
}

/// 不存在的文件计入失败但不拖垮整批
#[test]
fn missing_file_reported_not_fatal() {
    if skip_if_unready(venv_python()) {
        return;
    }
    let dir = tempfile::tempdir().expect("创建临时目录");
    let out_dir = dir.path().join("out");

    let report = office::word_to_pdf(
        &[PathBuf::from("/nonexistent/丢失的文件.docx")],
        &out_dir,
        WordQuality::High,
        120,
        None::<fn(u32, u32)>,
    )
    .expect("批量调用本身应正常返回");

    assert_eq!(report.fail_count(), 1);
    assert!(report.failed[0].1.contains("不存在"), "原因为: {}", report.failed[0].1);
}
