//! 激活模块:设备指纹 + 离线许可证验证 + 捐赠二维码
//!
//! 设计:1 元捐赠后,服务端用 ed25519 私钥对 `{device_id, plan, activated_at, expiry}` 签名,
//! 生成 license blob。客户端用内置公钥验签,完全离线友好。

pub mod fingerprint;
pub mod license;
pub mod qrcode;

pub use fingerprint::{device_id, device_info, DeviceInfo};
pub use license::{License, LicenseError};
pub use qrcode::{donation_qrcode, DonationQrCode};
