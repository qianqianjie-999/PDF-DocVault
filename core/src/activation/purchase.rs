//! 购买与激活引导
//!
//! 收款码图片由前端从 `public/pay-qr.png` 加载(放进去即生效,不放则显示文字引导),
//! 不在 Rust 里硬编码 base64 —— 换收款码不需要重新编译。

use crate::errors::Result;

/// 售价(元)。一次性买断,永久授权。
pub const PRICE_YUAN: u32 = 1;

/// 默认店铺地址。上架后可通过环境变量 `PDF_DOCVAULT_PURCHASE_URL` 覆盖,
/// 也可以在打包时编译期注入。
const DEFAULT_PURCHASE_URL: &str = "https://afdian.com/a/pdf-docvault";

/// 购买引导信息
#[derive(Debug, Clone, serde::Serialize)]
pub struct PurchaseInfo {
    /// 售价(元)
    pub price_yuan: u32,
    /// 展示用文案
    pub price_label: String,
    /// 卡密购买地址(发卡平台 / 发电主页)
    pub purchase_url: String,
    /// 本机设备指纹(联系客服换机时需要报给它)
    pub device_id: String,
}

/// 生成购买引导信息
pub fn purchase_info(device_id: &str) -> Result<PurchaseInfo> {
    let purchase_url = std::env::var("PDF_DOCVAULT_PURCHASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            option_env!("PDF_DOCVAULT_PURCHASE_URL")
                .map(str::to_string)
                .filter(|s| !s.trim().is_empty())
        })
        .unwrap_or_else(|| DEFAULT_PURCHASE_URL.to_string());

    Ok(PurchaseInfo {
        price_yuan: PRICE_YUAN,
        price_label: format!("{PRICE_YUAN} 元永久激活"),
        purchase_url,
        device_id: device_id.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_label_is_perpetual() {
        let info = purchase_info("dev123").unwrap();
        assert_eq!(info.price_yuan, 1);
        assert!(info.price_label.contains("永久"), "买断制文案必须写明永久");
        assert!(!info.purchase_url.is_empty());
        assert_eq!(info.device_id, "dev123");
    }
}
