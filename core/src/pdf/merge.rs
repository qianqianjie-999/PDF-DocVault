//! PDF 合并
//!
//! 将多个 PDF 合并为单个文件。使用 lopdf 加载各文档,重新编号对象后
//! 转移到新文档,统一挂载到新的 Pages 字典下。rayon 并行加载输入。

use lopdf::{dictionary, Document, Object, ObjectId};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use tracing::warn;

use crate::errors::Result;

/// 合并统计信息
#[derive(Debug, Clone, serde::Serialize)]
pub struct MergeStats {
    /// 合并后总页数
    pub pages: u32,
    /// 输入文件数
    pub inputs: usize,
    /// 耗时(毫秒)
    pub elapsed_ms: u128,
}

/// 合并多个 PDF 文件到 `output`。
///
/// 输入顺序即输出页面顺序。各输入的对象会被重新编号以避免冲突。
pub fn merge(input_paths: &[PathBuf], output: &Path) -> Result<MergeStats> {
    let start = std::time::Instant::now();
    if input_paths.is_empty() {
        return Err(crate::errors::CoreError::InvalidArgument(
            "合并至少需要一个输入文件".into(),
        ));
    }

    // 并行加载所有文档
    let docs: Vec<Document> = input_paths
        .par_iter()
        .map(|p| Document::load(p))
        .collect::<std::result::Result<_, _>>()?;

    let mut merged = Document::with_version("1.5");
    let pages_id: ObjectId = merged.new_object_id();
    let mut kids: Vec<Object> = Vec::new();
    let mut max_id = merged.max_id;

    for mut doc in docs {
        // 解密(若加密),使用空密码尝试
        if doc.is_encrypted() {
            let _ = doc.decrypt("");
        }
        // 重新编号对象,避免与已合并对象冲突
        doc.renumber_objects_with(max_id + 1);
        max_id = doc.max_id;

        // 转移所有对象(prune_objects 稍后清理无引用的旧 Catalog/Pages)
        for (id, obj) in doc.objects.iter() {
            merged.objects.insert(*id, obj.clone());
        }

        // 收集页面(按页号顺序)
        let pages = doc.get_pages();
        for (_page_no, page_id) in pages {
            // 页面对象已转移,直接引用
            kids.push(Object::Reference(page_id));
        }
    }

    let count = kids.len() as u32;
    merged.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => count,
            "Kids" => kids,
        }),
    );

    let catalog_id = merged.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    merged.trailer.set("Root", catalog_id);

    // 清理与压缩
    merged.prune_objects();
    merged.renumber_objects();

    if count == 0 {
        warn!("合并结果为 0 页,请检查输入文件是否有效");
    }

    merged.save(output)?;

    Ok(MergeStats {
        pages: count,
        inputs: input_paths.len(),
        elapsed_ms: start.elapsed().as_millis(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_sample_pdf(path: &Path, pages: u32) -> Result<()> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let mut kids = Vec::new();
        for _ in 0..pages {
            let content = "BT /F1 12 Tf 100 700 Td (Hello) Tj ET".as_bytes().to_vec();
            let stream = lopdf::Stream::new(dictionary! {}, content);
            let content_id = doc.add_object(stream);
            let font_id = doc.add_object(dictionary! {
                "Type" => "Font",
                "Subtype" => "Type1",
                "BaseFont" => "Helvetica",
            });
            let resources_id = doc.add_object(dictionary! {
                "Font" => dictionary! { "F1" => font_id },
            });
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
        doc.save(path)?;
        Ok(())
    }

    #[test]
    fn merge_basic() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.pdf");
        let b = tmp.path().join("b.pdf");
        let out = tmp.path().join("merged.pdf");
        make_sample_pdf(&a, 2).unwrap();
        make_sample_pdf(&b, 3).unwrap();
        let stats = merge(&[a, b], &out).unwrap();
        assert_eq!(stats.pages, 5);
        assert!(out.exists());
        let merged = Document::load(&out).unwrap();
        assert_eq!(merged.get_pages().len(), 5);
    }
}
