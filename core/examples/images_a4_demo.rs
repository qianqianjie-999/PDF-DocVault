//! 图片转 A4 PDF 端到端验证示例
//!
//! 用 6 张测试图跑 3 种布局组合,产出 PDF 到 /tmp/images2pdf-test/:
//! 1. 4 张/页 + 行优先(→↓)     → 2 页
//! 2. 4 张/页 + 列优先(↓→)     → 2 页(与组合 1 对比排列顺序)
//! 3. 横向 A4 + 2 张/页(左右)   → 3 页
//!
//! 运行:cargo run -p pdf_docvault_core --example images_a4_demo

use pdf_docvault_core::image::{
    images_to_a4_pdf, GridOrder, ImagesToA4Options, ImagesPerA4, PageOrientation, ScanOptions,
};
use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from("/tmp/images2pdf-test");
    let inputs: Vec<PathBuf> = (1..=6)
        .map(|i| dir.join(format!("img{i}.png")))
        .collect();
    for p in &inputs {
        assert!(p.is_file(), "缺少测试图 {},请先生成", p.display());
    }

    let cases: Vec<(&str, ImagesToA4Options, u32)> = vec![
        (
            "a4-four-row.pdf",
            ImagesToA4Options {
                per_page: ImagesPerA4::Four,
                grid_order: GridOrder::RowMajor,
                enhance: ScanOptions::none(),
                ..Default::default()
            },
            2,
        ),
        (
            "a4-four-column.pdf",
            ImagesToA4Options {
                per_page: ImagesPerA4::Four,
                grid_order: GridOrder::ColumnMajor,
                enhance: ScanOptions::none(),
                ..Default::default()
            },
            2,
        ),
        (
            "a4-landscape-two.pdf",
            ImagesToA4Options {
                per_page: ImagesPerA4::Two,
                orientation: PageOrientation::Landscape,
                fit_mode: pdf_docvault_core::image::ImageFitMode::Cover,
                enhance: ScanOptions::none(),
                ..Default::default()
            },
            3,
        ),
    ];

    for (name, opts, expect_pages) in cases {
        let out = dir.join(name);
        let stats = images_to_a4_pdf(&inputs, &out, &opts, None).expect(name);
        assert_eq!(stats.pages, expect_pages, "{name} 页数不符");
        // 重新加载验证 PDF 结构
        let doc = lopdf::Document::load(&out).expect("PDF 应可加载");
        assert_eq!(doc.get_pages().len(), expect_pages as usize);
        println!(
            "OK {:<22} {} 页 / {} ms / {} KB",
            name,
            stats.pages,
            stats.elapsed_ms,
            std::fs::metadata(&out).unwrap().len() / 1024
        );
    }
    println!("\n产物目录:{}/", dir.display());
}
