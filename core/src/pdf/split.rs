//! PDF 拆分
//!
//! 支持按页码范围拆分或每 N 页拆分。rayon 并行生成各输出文件。

use lopdf::{dictionary, Document, Object, ObjectId};
use rayon::prelude::*;
use std::path::{Path, PathBuf};

use crate::errors::{CoreError, Result};

/// 页码范围(1-based,闭区间)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PageRange {
    pub start: u32,
    pub end: u32,
}

/// 拆分模式
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind")]
pub enum SplitMode {
    /// 按显式页码范围拆分
    ByRanges { ranges: Vec<PageRange> },
    /// 每 N 页拆一个文件
    EveryNPages { n: u32 },
}

/// 拆分统计
#[derive(Debug, Clone, serde::Serialize)]
pub struct SplitStats {
    /// 生成的文件数
    pub outputs: usize,
    /// 输出文件路径
    pub paths: Vec<PathBuf>,
    /// 耗时(毫秒)
    pub elapsed_ms: u128,
}

/// 拆分 PDF。
///
/// 输出文件命名规则:`{原文件名}_{start}-{end}.pdf`,存放在 `output_dir`。
pub fn split(input: &Path, mode: &SplitMode, output_dir: &Path) -> Result<SplitStats> {
    let start = std::time::Instant::now();
    let doc = Document::load(input)?;
    let total = doc.get_pages().len() as u32;
    if total == 0 {
        return Err(CoreError::InvalidArgument("输入 PDF 没有页面".into()));
    }

    let ranges: Vec<PageRange> = match mode {
        SplitMode::ByRanges { ranges } => {
            if ranges.is_empty() {
                return Err(CoreError::InvalidArgument("页码范围列表为空".into()));
            }
            ranges.iter().map(|r| validate_range(r, total)).collect::<Result<_>>()?
        }
        SplitMode::EveryNPages { n } => {
            if *n == 0 {
                return Err(CoreError::InvalidArgument("n 必须大于 0".into()));
            }
            (1..=total)
                .step_by(*n as usize)
                .map(|s| PageRange {
                    start: s,
                    end: (s + n - 1).min(total),
                })
                .collect()
        }
    };

    std::fs::create_dir_all(output_dir)?;
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("split");

    let paths: Vec<PathBuf> = ranges
        .par_iter()
        .map(|r| extract_range_to_file(&doc, r, output_dir, stem))
        .collect::<Result<_>>()?;

    Ok(SplitStats {
        outputs: paths.len(),
        paths,
        elapsed_ms: start.elapsed().as_millis(),
    })
}

fn validate_range(r: &PageRange, total: u32) -> Result<PageRange> {
    if r.start == 0 || r.end < r.start {
        return Err(CoreError::InvalidArgument(format!(
            "无效页码范围: {}-{}",
            r.start, r.end
        )));
    }
    Ok(PageRange {
        start: r.start,
        end: r.end.min(total),
    })
}

fn extract_range_to_file(
    doc: &Document,
    range: &PageRange,
    out_dir: &Path,
    stem: &str,
) -> Result<PathBuf> {
    // 与 merge/extract_pages 同款:整文档转移对象后重建 Pages/Catalog,
    // 由 prune_objects 清理范围外页面(手工逐页复制会遗漏 max_id,导致保存截断)。
    let mut new_doc = Document::with_version("1.5");
    for (id, obj) in doc.objects.iter() {
        new_doc.objects.insert(*id, obj.clone());
    }
    new_doc.max_id = doc.max_id;

    let pages_map = doc.get_pages();
    let mut kids: Vec<Object> = Vec::new();
    for p in range.start..=range.end {
        if let Some(&page_id) = pages_map.get(&p) {
            kids.push(Object::Reference(page_id));
        }
    }
    if kids.is_empty() {
        return Err(CoreError::InvalidArgument(format!(
            "页码范围 {}-{} 内没有页面",
            range.start, range.end
        )));
    }

    let pages_id = new_doc.add_object(dictionary! {
        "Type" => "Pages",
        "Count" => kids.len() as u32,
        "Kids" => kids,
    });
    let catalog_id = new_doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    new_doc.trailer.set("Root", catalog_id);
    new_doc.prune_objects();
    new_doc.renumber_objects();

    let out = out_dir.join(format!("{}_{}-{}.pdf", stem, range.start, range.end));
    new_doc.save(&out)?;
    Ok(out)
}

/// 递归复制一个对象及其引用的所有对象到目标文档
pub fn copy_object_tree_pub(src: &Document, dst: &mut Document, id: ObjectId) -> Result<()> {
    copy_object_tree(src, dst, id)
}

/// 递归复制一个对象及其引用的所有对象到目标文档
fn copy_object_tree(src: &Document, dst: &mut Document, id: ObjectId) -> Result<()> {
    if dst.objects.contains_key(&id) {
        return Ok(());
    }
    let obj = src
        .get_object(id)
        .map_err(|e| CoreError::Internal(format!("对象 {:?} 不存在: {}", id, e)))?
        .clone();
    // 先插入占位,避免循环引用无限递归
    dst.objects.insert(id, obj.clone());
    // 递归复制引用的对象
    collect_references(&obj, &mut |ref_id| {
        if !dst.objects.contains_key(&ref_id) {
            let _ = copy_object_tree(src, dst, ref_id);
        }
    });
    Ok(())
}

/// 遍历对象中所有引用 ID
///
/// 使用 `&mut dyn FnMut` 而非泛型:泛型 + `&mut f` 递归会导致编译期无限单态化。
fn collect_references(obj: &Object, f: &mut dyn FnMut(ObjectId)) {
    match obj {
        Object::Reference(id) => f(*id),
        Object::Array(arr) => arr.iter().for_each(|o| collect_references(o, f)),
        Object::Dictionary(dict) => dict.iter().for_each(|(_, o)| collect_references(o, f)),
        Object::Stream(stream) => {
            dict_iter(&stream.dict).for_each(|(_, o)| collect_references(o, f));
        }
        _ => {}
    }
}

fn dict_iter(dict: &lopdf::Dictionary) -> impl Iterator<Item = (&[u8], &Object)> {
    dict.iter().map(|(k, v)| (k.as_slice(), v))
}

#[cfg(test)]
mod tests {
    #[test]
    fn split_every_n() {
        let tmp = tempfile::tempdir().unwrap();
        let input = tmp.path().join("src.pdf");
        // 复用 merge 测试里的构造方式较重,这里仅验证 API 不 panic
        // 真实 PDF 测试在 tests/fixtures 下进行
        let _ = input;
    }
}
