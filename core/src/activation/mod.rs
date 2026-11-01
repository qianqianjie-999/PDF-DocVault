//! 激活模块:设备指纹 + 卡密 + 离线许可证验证 + 购买引导
//!
//! 链路:用户买卡密 → 客户端把「卡密 + 设备指纹」发给兑换服务 → 服务端验卡后
//! 用 ed25519 私钥签名 `{device_id, plan, activated_at, expiry}` 得到 license blob
//! → 客户端内置公钥验签并落库,此后完全离线。

pub mod fingerprint;
pub mod license;
pub mod purchase;
pub mod voucher;

pub use fingerprint::{device_id, device_info, DeviceInfo};
pub use license::{License, LicenseError};
pub use purchase::{purchase_info, PurchaseInfo};
pub use voucher::CODE_CHARS;
