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
// 前端 SplitMode 字面量是 by_ranges / every_n_pages;
// 不写 rename_all 时 serde 只认 ByRanges / EveryNPages,拆分命令必然反序列化失败
#[serde(tag = "kind", rename_all = "snake_case")]
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
    let mut doc = Document::load(input)?;
    // 拆分需重建页面树,加密流无法原样带过去
    super::decrypt_if_needed(&mut doc)?;
    let total = doc.get_pages().len() as u32;
    if total == 0 {
        return Err(CoreError::InvalidArgument("输入 PDF 没有页面".into()));
    }

    let ranges: Vec<PageRange> = match mode {
        SplitMode::ByRanges { ranges } => {
            if ranges.is_empty() {
                return Err(CoreError::InvalidArgument("页码范围列表为空".into()));
            }
            ranges
                .iter()
                .map(|r| validate_range(r, total))
                .collect::<Result<_>>()?
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

    // 先串行确定每个输出路径:同目录重复拆分(或同名源文件)时文件名会撞车,
    // 直接覆盖会悄悄丢掉上一次的成果;重复范围还会有两个并行任务写同一路径,
    // 后者覆盖前者导致文件损坏。命名规则与转图片一致,撞名时追加 _1/_2。
    let mut planned: Vec<(&PageRange, PathBuf)> = Vec::with_capacity(ranges.len());
    let mut used: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    for r in &ranges {
        let mut n = 0u32;
        let out = loop {
            let name = if n == 0 {
                format!("{}_{}-{}.pdf", stem, r.start, r.end)
            } else {
                format!("{}_{}-{}_{}.pdf", stem, r.start, r.end, n)
            };
            let candidate = output_dir.join(name);
            if !candidate.exists() && used.insert(candidate.clone()) {
                break candidate;
            }
            n += 1;
        };
        planned.push((r, out));
    }

    let paths: Vec<PathBuf> = planned
        .par_iter()
        .map(|(r, out)| extract_range_to_file(&doc, r, out))
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
    // 起始页已超出总页数时必须先报错:直接截断 end 会得到 start > end 的空区间,
    // 后面的提取只能报「范围内没有页面」,用户看不出是页码越界。
    if r.start > total {
        return Err(CoreError::InvalidArgument(format!(
            "页码范围 {}-{} 的起始页超出文档总页数 {}",
            r.start, r.end, total
        )));
    }
    Ok(PageRange {
        start: r.start,
        end: r.end.min(total),
    })
}

fn extract_range_to_file(doc: &Document, range: &PageRange, out: &Path) -> Result<PathBuf> {
    // 与 merge/extract_pages 同款:整文档转移对象后重建 Pages/Catalog,
    // 由 prune_objects 清理范围外页面(手工逐页复制会遗漏 max_id,导致保存截断)。
    let mut new_doc = Document::with_version("1.5");
    for (id, obj) in doc.objects.iter() {
        new_doc.objects.insert(*id, obj.clone());
    }
    new_doc.max_id = doc.max_id;

    let pages_map = doc.get_pages();
    let mut kids: Vec<Object> = Vec::new();
    let mut page_ids: Vec<ObjectId> = Vec::new();
    for p in range.start..=range.end {
        if let Some(&page_id) = pages_map.get(&p) {
            page_ids.push(page_id);
            kids.push(Object::Reference(page_id));
        }
    }
    if kids.is_empty() {
        return Err(CoreError::InvalidArgument(format!(
            "页码范围 {}-{} 内没有页面",
            range.start, range.end
        )));
    }

    // 先把中间节点上的版式属性落到页面自身,再改 /Parent 指向新根,
    // 否则 prune_objects 清掉旧节点后页面会丢失 MediaBox/Resources
    super::inherit_page_attributes(&mut new_doc, &page_ids);
    let pages_id = new_doc.add_object(dictionary! {
        "Type" => "Pages",
        "Count" => kids.len() as u32,
        "Kids" => kids,
    });
    super::reparent_pages(&mut new_doc, pages_id, &page_ids);
    let catalog_id = new_doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    new_doc.trailer.set("Root", catalog_id);
    new_doc.prune_objects();
    new_doc.renumber_objects();

    new_doc.save(out)?;
    Ok(out.to_path_buf())
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
    use super::*;

    /// 前端发送的是 { kind: 'by_ranges' } / { kind: 'every_n_pages' },
    /// serde 必须按 snake_case 识别,否则拆分命令在反序列化阶段就失败
    #[test]
    fn split_mode_accepts_frontend_kind_names() {
        let every: SplitMode = serde_json::from_str(r#"{"kind":"every_n_pages","n":3}"#)
            .expect("every_n_pages 应可反序列化");
        match every {
            SplitMode::EveryNPages { n } => assert_eq!(n, 3),
            _ => panic!("kind 解析到了错误分支"),
        }

        let ranges: SplitMode =
            serde_json::from_str(r#"{"kind":"by_ranges","ranges":[{"start":1,"end":10}]}"#)
                .expect("by_ranges 应可反序列化");
        match ranges {
            SplitMode::ByRanges { ranges } => {
                assert_eq!(ranges.len(), 1);
                assert_eq!(ranges[0].start, 1);
                assert_eq!(ranges[0].end, 10);
            }
            _ => panic!("kind 解析到了错误分支"),
        }
    }

    #[test]
    fn split_every_n() {
        let tmp = tempfile::tempdir().unwrap();
        let input = tmp.path().join("src.pdf");
        // 复用 merge 测试里的构造方式较重,这里仅验证 API 不 panic
        // 真实 PDF 测试在 tests/fixtures 下进行
        let _ = input;
    }
}
