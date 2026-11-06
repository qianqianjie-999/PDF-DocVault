//! PDF 处理模块
//!
//! 基于 `lopdf`(读写/合并/拆分/页面操作/水印)与 `pdfium-render`(转图片)实现。
//! 加密因 lopdf 不支持写入,经 pycore 委托 pikepdf(静态链接 libqpdf),不依赖系统 qpdf。

pub mod compress;
pub mod encrypt;
pub mod merge;
pub mod pages;
pub mod render;
pub mod split;
pub mod watermark;

pub use compress::{compress, CompressStats};
pub use encrypt::{
    decrypt, encrypt_aes256, probe_encryption, DecryptReport, EncryptOptions, EncryptReport,
    EncryptionInfo, PermissionReport, Permissions,
};
pub use merge::{merge, MergeStats};
pub use pages::{delete_pages, extract_pages, insert_pages, reorder_pages, rotate_pages};
pub use render::{render_all_pages, render_page_to_bytes, render_page_to_image};
pub use split::{split, PageRange, SplitMode, SplitStats};
pub use watermark::{add_image_watermark, add_text_watermark, WatermarkOptions};

use lopdf::{Document, Object, ObjectId};

use crate::errors::{CoreError, Result};

/// 页面可继承属性:按规范可写在中间 Pages 节点上,由叶子页面继承
const INHERITABLE_PAGE_KEYS: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

fn parent_of(doc: &Document, id: ObjectId) -> Option<ObjectId> {
    doc.get_object(id)
        .ok()?
        .as_dict()
        .ok()?
        .get(b"Parent")
        .ok()?
        .as_reference()
        .ok()
}

/// 把从祖先 Pages 节点继承的属性复制到页面自身。
///
/// 合并/拆分/提取/重排/插入都会重建页面树,中间 Pages 节点随后被 prune 清掉;
/// 若页面自己没有 /MediaBox 或 /Resources,只看继承链的页面会变成空白页。
/// 必须在修改页面的 /Parent 之前调用。
pub(crate) fn inherit_page_attributes(doc: &mut Document, page_ids: &[ObjectId]) {
    for &page_id in page_ids {
        // 收集父链;限制层数并去重,防止异常文件里的环状 /Parent 死循环
        let mut chain: Vec<ObjectId> = Vec::new();
        let mut cur = parent_of(doc, page_id);
        while let Some(id) = cur {
            if chain.len() >= 32 || chain.contains(&id) {
                break;
            }
            chain.push(id);
            cur = parent_of(doc, id);
        }
        if chain.is_empty() {
            continue;
        }

        let mut inherited: Vec<(&[u8], Object)> = Vec::new();
        for &key in INHERITABLE_PAGE_KEYS.iter() {
            let has_own = doc
                .get_object(page_id)
                .ok()
                .and_then(|o| o.as_dict().ok())
                .map(|d| d.get(key).is_ok())
                .unwrap_or(false);
            if has_own {
                continue;
            }
            for &ancestor in &chain {
                let value = doc
                    .get_object(ancestor)
                    .ok()
                    .and_then(|o| o.as_dict().ok())
                    .and_then(|d| d.get(key).ok())
                    .cloned();
                if let Some(value) = value {
                    inherited.push((key, value));
                    break;
                }
            }
        }
        if inherited.is_empty() {
            continue;
        }
        if let Ok(dict) = doc.get_object_mut(page_id).and_then(|o| o.as_dict_mut()) {
            for (key, value) in inherited {
                dict.set(key, value);
            }
        }
    }
}

/// 重建页面树后统一修正 /Parent 指向新的根。
///
/// 只替换根节点的 Kids 而不改页面的 /Parent 时,`prune_objects` 会顺着
/// 「页面 → 旧 Pages → 旧 Kids」把未选中的页面全部保留下来:
/// 拆一个大文档时每份输出都塞进全部页面对象,体积虚涨,父子关系也与实际结构不符。
pub(crate) fn reparent_pages(doc: &mut Document, pages_id: ObjectId, page_ids: &[ObjectId]) {
    for &id in page_ids {
        if let Ok(dict) = doc.get_object_mut(id).and_then(|o| o.as_dict_mut()) {
            dict.set("Parent", Object::Reference(pages_id));
        }
    }
}

/// 页面级操作(合并/拆分/提取/重排/水印)无法安全处理需要打开密码的 PDF:
/// 先用空密码尝试解密,失败时明确提示先解密,避免把加密流原样写进新文档,
/// 生成一份打不开或内容乱码的成品。
pub(crate) fn decrypt_if_needed(doc: &mut lopdf::Document) -> Result<()> {
    if doc.is_encrypted() {
        doc.decrypt("").map_err(|_| {
            CoreError::InvalidArgument("文件已加密,请先用「解密」功能移除打开密码后再操作".into())
        })?;
    }
    Ok(())
}
