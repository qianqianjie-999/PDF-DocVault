//! PDF 页面操作:旋转/删除/插入/提取/重排

use lopdf::{dictionary, Document, Object};
use std::path::Path;

use crate::errors::{CoreError, Result};

/// 旋转指定页面。`degrees` 应为 90/180/270 之一(累加到已有 /Rotate)。
pub fn rotate_pages(input: &Path, page_nums: &[u32], degrees: u32, output: &Path) -> Result<()> {
    if degrees % 90 != 0 {
        return Err(CoreError::InvalidArgument("旋转角度必须是 90 的倍数".into()));
    }
    let mut doc = Document::load(input)?;
    let pages = doc.get_pages();
    for &pn in page_nums {
        let page_id = *pages
            .get(&pn)
            .ok_or(CoreError::PageNotFound(pn))?;
        let page_obj = doc.get_object_mut(page_id)?;
        let dict = page_obj.as_dict_mut()?;
        let cur = dict
            .get(b"Rotate")
            .ok()
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(0);
        dict.set("Rotate", Object::Integer((cur + degrees as i64) % 360));
    }
    doc.save(output)?;
    Ok(())
}

/// 删除指定页面
pub fn delete_pages(input: &Path, page_nums: &[u32], output: &Path) -> Result<()> {
    let mut doc = Document::load(input)?;
    // lopdf delete_pages 接受页号切片(1-based)
    doc.delete_pages(page_nums);
    doc.prune_objects();
    doc.renumber_objects();
    doc.save(output)?;
    Ok(())
}

/// 提取指定页面为单独的 PDF
///
/// 实现与 merge 同款:整文档转移对象后重建 Pages/Catalog,
/// 由 prune_objects 清理未选页(手工逐页复制会遗漏 max_id,导致保存截断)。
pub fn extract_pages(input: &Path, page_nums: &[u32], output: &Path) -> Result<()> {
    let mut doc = Document::load(input)?;
    if doc.is_encrypted() {
        let _ = doc.decrypt("");
    }
    let pages_map = doc.get_pages();

    let mut new_doc = Document::with_version("1.5");
    for (id, obj) in doc.objects.iter() {
        new_doc.objects.insert(*id, obj.clone());
    }
    new_doc.max_id = doc.max_id;

    let mut kids: Vec<Object> = Vec::new();
    for &pn in page_nums {
        let &page_id = pages_map.get(&pn).ok_or(CoreError::PageNotFound(pn))?;
        kids.push(Object::Reference(page_id));
    }
    if kids.is_empty() {
        return Err(CoreError::InvalidArgument("未指定要提取的页面".into()));
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
    new_doc.save(output)?;
    Ok(())
}

/// 将 `input2` 的所有页面插入到 `input1` 的 `after_page` 之后
pub fn insert_pages(
    input1: &Path,
    input2: &Path,
    after_page: u32,
    output: &Path,
) -> Result<()> {
    let mut doc1 = Document::load(input1)?;
    let doc2 = Document::load(input2)?;
    let pages1 = doc1.get_pages();
    let total1 = pages1.len() as u32;
    if after_page > total1 {
        return Err(CoreError::InvalidArgument(format!(
            "after_page {} 超出文档1总页数 {}",
            after_page, total1
        )));
    }

    // 转移 doc2 所有对象
    let max1 = doc1.max_id;
    let mut doc2_renamed = doc2;
    doc2_renamed.renumber_objects_with(max1 + 1);
    for (id, obj) in doc2_renamed.objects.iter() {
        doc1.objects.insert(*id, obj.clone());
    }
    // 关键:同步 max_id,否则 save 时按 max_id 写 xref 会截断 doc2 转移来的对象
    doc1.max_id = doc1.max_id.max(doc2_renamed.max_id);
    let pages2 = doc2_renamed.get_pages();

    // 构造新页面顺序:1..after_page + doc2 pages + (after_page+1)..end
    // catalog() 返回 &Dictionary(Catalog 字典),从中取 Pages 的 ObjectId
    let pages_id = doc1
        .catalog()?
        .get(b"Pages")
        .and_then(Object::as_reference)
        .map_err(|_| CoreError::Internal("无法找到 Pages 根节点".into()))?;

    let mut new_kids: Vec<Object> = Vec::new();
    // 修正插入页的 /Parent:原本指向 doc2 的 Pages 根(prune 后会消失),改指 doc1 的 Pages 根
    for (_, id) in &pages2 {
        if let Some(page) = doc1.objects.get_mut(id) {
            if let Ok(dict) = page.as_dict_mut() {
                dict.set("Parent", Object::Reference(pages_id));
            }
        }
    }
    for p in 1..=after_page {
        if let Some(&id) = pages1.get(&p) {
            new_kids.push(Object::Reference(id));
        }
    }
    for (_pn, id) in pages2 {
        new_kids.push(Object::Reference(id));
    }
    for p in (after_page + 1)..=total1 {
        if let Some(&id) = pages1.get(&p) {
            new_kids.push(Object::Reference(id));
        }
    }

    let pages_dict = doc1.get_object_mut(pages_id)?;
    pages_dict
        .as_dict_mut()?
        .set("Kids", Object::Array(new_kids.clone()));
    pages_dict
        .as_dict_mut()?
        .set("Count", Object::Integer(new_kids.len() as i64));

    doc1.prune_objects();
    doc1.save(output)?;
    Ok(())
}

/// 按新顺序重排页面。`new_order` 为 1-based 原页码序列
///
/// 与 merge/extract_pages 同款:整文档转移对象后按新顺序重建 Pages/Catalog。
pub fn reorder_pages(input: &Path, new_order: &[u32], output: &Path) -> Result<()> {
    let mut doc = Document::load(input)?;
    if doc.is_encrypted() {
        let _ = doc.decrypt("");
    }
    let pages_map = doc.get_pages();

    let mut kids: Vec<Object> = Vec::new();
    for &pn in new_order {
        let &page_id = pages_map.get(&pn).ok_or(CoreError::PageNotFound(pn))?;
        kids.push(Object::Reference(page_id));
    }
    if kids.is_empty() {
        return Err(CoreError::InvalidArgument("未指定任何页面".into()));
    }

    let mut new_doc = Document::with_version("1.5");
    for (id, obj) in doc.objects.iter() {
        new_doc.objects.insert(*id, obj.clone());
    }
    new_doc.max_id = doc.max_id;

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
    new_doc.save(output)?;
    Ok(())
}
