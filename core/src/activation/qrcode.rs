//! 捐赠二维码
//!
//! MVP 阶段返回静态微信收款码占位 + 带 device_id 的激活 URL。
//! 生产环境:用户扫码付款后访问 URL,服务端收到回调签发 license。

use crate::errors::Result;

/// 捐赠二维码信息
#[derive(Debug, Clone, serde::Serialize)]
pub struct DonationQrCode {
    /// 微信收款码图片(base64,前端直接渲染)
    pub qr_base64: String,
    /// 激活 URL(付款后访问)
    pub activate_url: String,
    /// 设备 ID
    pub device_id: String,
    /// 捐赠金额说明
    pub amount_label: String,
}

/// 生成捐赠二维码信息
pub fn donation_qrcode(device_id: &str) -> Result<DonationQrCode> {
    // 占位二维码:1x1 透明 PNG base64。生产环境替换为真实微信收款码。
    let placeholder_png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=";
    Ok(DonationQrCode {
        qr_base64: format!("data:image/png;base64,{}", placeholder_png),
        activate_url: format!("https://docvault.app/activate?device={}", device_id),
        device_id: device_id.to_string(),
        amount_label: "1 元捐赠激活全部功能".to_string(),
    })
}
