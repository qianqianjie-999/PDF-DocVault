//! PDF 操作性能基准
//!
//! 使用 criterion 对核心 PDF 操作进行微基准测试。
//! 目标指标(规划书):合并 <200ms、拆分 <100ms、加密 <50ms。
//!
//! 运行:`cargo bench -p pdf_docvault_core --bench pdf_ops`

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use lopdf::{dictionary, Document, Object, ObjectId};
use pdf_docvault_core::pdf;
use std::path::PathBuf;

/// 生成 N 页的测试 PDF
fn make_test_pdf(path: &std::path::Path, pages: u32) {
    let mut doc = Document::with_version("1.5");
    let pages_id: ObjectId = doc.new_object_id();
    let mut kids: Vec<Object> = Vec::new();

    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });

    for i in 0..pages {
        let text = format!(
            "BT /F1 12 Tf 72 700 Td (Page {} - PDF-DocVault Benchmark) Tj ET",
            i + 1
        );
        let content = text.into_bytes();
        let stream = lopdf::Stream::new(dictionary! {}, content);
        let content_id = doc.add_object(stream);
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Resources" => resources_id,
            "Contents" => content_id,
        });
        kids.push(page_id.into());
    }

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => pages,
            "Kids" => kids,
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    doc.save(path).unwrap();
}

fn bench_merge(c: &mut Criterion) {
    let tmp = tempfile::tempdir().unwrap();
    let inputs: Vec<PathBuf> = (0..3)
        .map(|i| {
            let p = tmp.path().join(format!("input-{}.pdf", i));
            make_test_pdf(&p, 100);
            p
        })
        .collect();
    let output = tmp.path().join("merged.pdf");

    c.bench_with_input(
        BenchmarkId::new("merge", "3x100p"),
        &(&inputs, &output),
        |b, (inputs, output)| {
            b.iter(|| {
                pdf::merge::merge(inputs, output).unwrap();
            })
        },
    );
}

fn bench_split(c: &mut Criterion) {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("src-300p.pdf");
    make_test_pdf(&input, 300);
    let out_dir = tmp.path().join("split");
    std::fs::create_dir_all(&out_dir).unwrap();

    c.bench_with_input(
        BenchmarkId::new("split", "300p-every50"),
        &(&input, &out_dir),
        |b, (input, out_dir)| {
            b.iter(|| {
                pdf::split::split(
                    input,
                    &pdf::split::SplitMode::EveryNPages { n: 50 },
                    out_dir,
                )
                .unwrap();
            })
        },
    );
}

fn bench_encrypt(c: &mut Criterion) {
    // 加密走 pycore(pikepdf),解释器子进程必须可定位
    if pdf_docvault_core::pybridge::resolve_pycore_dir().is_err() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("src-50p.pdf");
    make_test_pdf(&input, 50);
    let output = tmp.path().join("encrypted.pdf");

    c.bench_with_input(
        BenchmarkId::new("encrypt", "aes256-50p"),
        &(&input, &output),
        |b, (input, output)| {
            b.iter(|| {
                pdf::encrypt::encrypt_aes256(
                    input,
                    output,
                    &pdf::encrypt::EncryptOptions {
                        user_password: "test".into(),
                        ..Default::default()
                    },
                )
                .unwrap();
            })
        },
    );
}

criterion_group!(benches, bench_merge, bench_split, bench_encrypt);
criterion_main!(benches);
