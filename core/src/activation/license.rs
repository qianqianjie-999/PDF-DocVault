//! 离线许可证验证(ed25519)
//!
//! 客户端内置公钥,服务端持有私钥签发 license。验签完全离线。
//!
//! 开发期(`feature = "dev-key"` 或 debug 构建)提供 `dev_sign` 辅助生成测试 license。

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

use crate::errors::Result;

/// 开发期种子(对应 dev_sign 的私钥)。生产环境替换为正式公钥。
/// debug 构建下 public_key() 从此种子派生,保证与 dev_sign_license 配对。
#[cfg(debug_assertions)]
const DEV_SEED: [u8; 32] = [
    0x0b, 0x18, 0x3e, 0x55, 0xa1, 0xc4, 0x2f, 0x88, 0x6e, 0xa7, 0xd5, 0x39, 0x2c, 0x77, 0xb1,
    0x4e, 0x90, 0x3a, 0x6f, 0xe8, 0x15, 0x2a, 0xb7, 0xc3, 0x4d, 0x6e, 0x8a, 0x1f, 0x5b, 0x9d,
    0x2e, 0x07,
];

/// 许可证错误
#[derive(Debug, Clone, thiserror::Error)]
pub enum LicenseError {
    #[error("许可证签名无效")]
    InvalidSignature,
    #[error("许可证与本设备不匹配")]
    DeviceMismatch,
    #[error("许可证已过期")]
    Expired,
    #[error("许可证格式错误: {0}")]
    Malformed(String),
}

/// 许可证
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct License {
    pub device_id: String,
    pub plan: String,
    pub activated_at: i64,
    /// 过期时间戳(0 表示永久)
    pub expiry: i64,
    /// base64 编码的 ed25519 签名
    pub signature: String,
}

impl License {
    /// 签名消息(签名/验签的原文)
    fn sign_message(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.device_id, self.plan, self.activated_at, self.expiry
        )
    }

    /// 验证许可证:验签 + 比对 device_id + 检查过期
    pub fn verify(&self, expected_device_id: &str) -> std::result::Result<(), LicenseError> {
        if self.device_id != expected_device_id {
            return Err(LicenseError::DeviceMismatch);
        }
        if self.expiry != 0 {
            let now = chrono::Utc::now().timestamp();
            if now > self.expiry {
                return Err(LicenseError::Expired);
            }
        }
        let pk_bytes = public_key();
        let pk = VerifyingKey::from_bytes(&pk_bytes)
            .map_err(|e| LicenseError::Malformed(format!("公钥错误: {e}")))?;
        let sig_bytes = B64
            .decode(&self.signature)
            .map_err(|e| LicenseError::Malformed(format!("签名 base64 解码失败: {e}")))?;
        let sig = Signature::from_slice(&sig_bytes)
            .map_err(|e| LicenseError::Malformed(format!("签名格式错误: {e}")))?;
        let msg = self.sign_message();
        pk.verify(msg.as_bytes(), &sig).map_err(|_| LicenseError::InvalidSignature)?;
        Ok(())
    }

    /// 从 base64 JSON blob 解析
    pub fn from_blob(blob: &str) -> std::result::Result<Self, LicenseError> {
        let json = B64
            .decode(blob)
            .map_err(|e| LicenseError::Malformed(format!("blob base64 解码失败: {e}")))?;
        serde_json::from_slice(&json).map_err(|e| LicenseError::Malformed(e.to_string()))
    }

    /// 序列化为 base64 JSON blob(便于存储/传输)
    pub fn to_blob(&self) -> String {
        let json = serde_json::to_vec(self).unwrap_or_default();
        B64.encode(json)
    }
}

fn public_key() -> [u8; 32] {
    #[cfg(debug_assertions)]
    {
        use ed25519_dalek::SigningKey;
        let signing = SigningKey::from_bytes(&DEV_SEED);
        signing.verifying_key().to_bytes()
    }
    #[cfg(not(debug_assertions))]
    {
        // 生产环境:替换为正式公钥(构建时注入或常量)
        // 此处占位,生产构建前必须替换
        [0u8; 32]
    }
}

/// 开发期:用私钥签发测试 license。仅 debug 构建可用。
#[cfg(debug_assertions)]
pub fn dev_sign_license(device_id: &str, plan: &str, expiry: i64) -> Result<String> {
    use ed25519_dalek::{Signer, SigningKey};

    // 从 DEV_SEED 派生固定密钥对,保证与 public_key() 配对。
    // 注意:生产环境绝不能在客户端持有私钥。
    let signing = SigningKey::from_bytes(&DEV_SEED);

    let now = chrono::Utc::now().timestamp();
    let msg = format!("{}|{}|{}|{}", device_id, plan, now, expiry);
    let sig = signing.sign(msg.as_bytes());
    let license = License {
        device_id: device_id.to_string(),
        plan: plan.to_string(),
        activated_at: now,
        expiry,
        signature: B64.encode(sig.to_bytes()),
    };
    Ok(license.to_blob())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(debug_assertions)]
    fn dev_license_roundtrip() {
        let dev = "test-device-123";
        let blob = dev_sign_license(dev, "full", 0).unwrap();
        let lic = License::from_blob(&blob).unwrap();
        lic.verify(dev).expect("dev license 应验签通过");
    }
}
