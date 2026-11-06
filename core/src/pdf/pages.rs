//! PDF 页面操作:旋转/删除/插入/提取/重排

use lopdf::{dictionary, Document, Object, ObjectId};
use std::path::Path;

use crate::errors::{CoreError, Result};

/// 旋转指定页面。`degrees` 应为 90/180/270 之一(累加到已有 /Rotate)。
pub fn rotate_pages(input: &Path, page_nums: &[u32], degrees: u32, output: &Path) -> Result<()> {
    if !degrees.is_multiple_of(90) {
        return Err(CoreError::InvalidArgument(
            "旋转角度必须是 90 的倍数".into(),
        ));
    }
    let mut doc = Document::load(input)?;
    let pages = doc.get_pages();
    // 页码留空时作用于全部页面,与界面「留空表示全部」的提示一致
    // 去重:同一页号出现两次会重复累加角度(如 "1,1" 把第 1 页转成 180°)
    let mut targets: Vec<u32> = if page_nums.is_empty() {
        pages.keys().copied().collect()
    } else {
        page_nums.to_vec()
    };
    targets.sort_unstable();
    targets.dedup();
    // 先把继承自中间 Pages 节点的 /Rotate 落到页面自身,再累加;
    // 否则读到的是 0,原有旋转会被本次操作覆盖
    let target_ids: Vec<ObjectId> = targets
        .iter()
        .map(|&pn| pages.get(&pn).copied().ok_or(CoreError::PageNotFound(pn)))
        .collect::<Result<_>>()?;
    super::inherit_page_attributes(&mut doc, &target_ids);
    for pn in targets {
        let page_id = *pages.get(&pn).ok_or(CoreError::PageNotFound(pn))?;
        let page_obj = doc.get_object_mut(page_id)?;
        let dict = page_obj.as_dict_mut()?;
        let cur = dict
            .get(b"Rotate")
            .ok()
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(0);
        // 负值 /Rotate(部分工具会写 -90/-180)用 % 会留下负角度,
        // 阅读器行为不一致;统一归一化到 0..360
        dict.set(
            "Rotate",
            Object::Integer((cur + degrees as i64).rem_euclid(360)),
        );
    }
    doc.save(output)?;
    Ok(())
}

/// 删除指定页面
pub fn delete_pages(input: &Path, page_nums: &[u32], output: &Path) -> Result<()> {
    let mut doc = Document::load(input)?;
    // 删除后会 prune/renumber,对象号变化会让加密流无法解密,
    // 必须先解密,不能把一份内容全乱的加密壳交给用户
    super::decrypt_if_needed(&mut doc)?;
    if page_nums.is_empty() {
        return Err(CoreError::InvalidArgument("未指定要删除的页面".into()));
    }
    let total = doc.get_pages().len() as u32;
    // 提前校验页号,避免超出范围时静默生成一份未删除任何页面的副本
    if let Some(&bad) = page_nums.iter().find(|&&pn| pn == 0 || pn > total) {
        return Err(CoreError::PageNotFound(bad));
    }
    // 去重后再判断:删光全部页面会产出 0 页 PDF,多数阅读器直接报错,
    // 与其生成一个打不开的空壳,不如明确要求至少保留 1 页。
    let mut targets = page_nums.to_vec();
    targets.sort_unstable();
    targets.dedup();
    if targets.len() as u32 >= total {
        return Err(CoreError::InvalidArgument(
            "不能删除全部页面,至少保留 1 页".into(),
        ));
    }
    // lopdf delete_pages 接受页号切片(1-based)
    doc.delete_pages(&targets);
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
    super::decrypt_if_needed(&mut doc)?;
    let pages_map = doc.get_pages();

    let mut new_doc = Document::with_version("1.5");
    for (id, obj) in doc.objects.iter() {
        new_doc.objects.insert(*id, obj.clone());
    }
    new_doc.max_id = doc.max_id;

    let mut kids: Vec<Object> = Vec::new();
    let mut page_ids: Vec<ObjectId> = Vec::new();
    // 去重:同一页只提取一次,重复引用会让页面树出现两个指向同一页面对象的叶子
    let mut seen = std::collections::HashSet::new();
    for &pn in page_nums {
        if !seen.insert(pn) {
            continue;
        }
        let &page_id = pages_map.get(&pn).ok_or(CoreError::PageNotFound(pn))?;
        kids.push(Object::Reference(page_id));
        page_ids.push(page_id);
    }
    if kids.is_empty() {
        return Err(CoreError::InvalidArgument("未指定要提取的页面".into()));
    }

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
    new_doc.save(output)?;
    Ok(())
}

/// 将 `input2` 的所有页面插入到 `input1` 的 `after_page` 之后
pub fn insert_pages(input1: &Path, input2: &Path, after_page: u32, output: &Path) -> Result<()> {
    let mut doc1 = Document::load(input1)?;
    let mut doc2 = Document::load(input2)?;
    // 任一输入需要打开密码时都无法安全重建页面树
    super::decrypt_if_needed(&mut doc1)?;
    super::decrypt_if_needed(&mut doc2)?;
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
    let mut all_ids: Vec<ObjectId> = Vec::new();
    // 修正插入页的 /Parent:原本指向 doc2 的 Pages 根(prune 后会消失),改指 doc1 的 Pages 根;
    // doc1 自身若是嵌套页面树,叶子同样要提升到根下,所以统一处理两边的页面
    for p in 1..=after_page {
        if let Some(&id) = pages1.get(&p) {
            new_kids.push(Object::Reference(id));
            all_ids.push(id);
        }
    }
    for id in pages2.values() {
        new_kids.push(Object::Reference(*id));
        all_ids.push(*id);
    }
    for p in (after_page + 1)..=total1 {
        if let Some(&id) = pages1.get(&p) {
            new_kids.push(Object::Reference(id));
            all_ids.push(id);
        }
    }

    // 顺序很重要:先在原页面树里取继承属性,再把 /Parent 全部改指 doc1 根
    super::inherit_page_attributes(&mut doc1, &all_ids);
    super::reparent_pages(&mut doc1, pages_id, &all_ids);

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
    super::decrypt_if_needed(&mut doc)?;
    let pages_map = doc.get_pages();

    // 重排必须是完整排列:漏页会静默丢页面,重复页会让页面树里出现两个
    // 指向同一页面对象的叶子,阅读器行为不一致甚至报错
    let total = pages_map.len() as u32;
    if new_order.is_empty() {
        return Err(CoreError::InvalidArgument("未指定任何页面".into()));
    }
    let mut seen = vec![false; total as usize + 1];
    for &pn in new_order {
        if pn == 0 || pn > total {
            return Err(CoreError::PageNotFound(pn));
        }
        if seen[pn as usize] {
            return Err(CoreError::InvalidArgument(format!(
                "页面顺序中第 {pn} 页重复出现"
            )));
        }
        seen[pn as usize] = true;
    }
    if let Some(idx) = seen.iter().skip(1).position(|&v| !v) {
        return Err(CoreError::InvalidArgument(format!(
            "页面顺序缺少第 {} 页,重排必须包含全部页面",
            idx + 1
        )));
    }

    let mut kids: Vec<Object> = Vec::new();
    let mut page_ids: Vec<ObjectId> = Vec::new();
    for &pn in new_order {
        let &page_id = pages_map.get(&pn).ok_or(CoreError::PageNotFound(pn))?;
        kids.push(Object::Reference(page_id));
        page_ids.push(page_id);
    }
    if kids.is_empty() {
        return Err(CoreError::InvalidArgument("未指定任何页面".into()));
    }

    let mut new_doc = Document::with_version("1.5");
    for (id, obj) in doc.objects.iter() {
        new_doc.objects.insert(*id, obj.clone());
    }
    new_doc.max_id = doc.max_id;

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
    new_doc.save(output)?;
    Ok(())
}
