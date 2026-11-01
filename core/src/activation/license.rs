//! 离线许可证验证(ed25519)
//!
//! 客户端只内置**公钥**,私钥保存在签发端(兑换 Worker 的 secret / 本地签发 CLI),
//! 验签完全离线,激活后不再联网。
//!
//! 公钥来源见 `build.rs`:release 构建必须注入正式公钥,否则编译失败;
//! debug 构建未注入时回落到开发种子派生的公钥,便于本地联调。

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

use crate::errors::Result;

/// 开发期种子(仅在 debug 构建、且未注入正式公钥时使用)。
/// **绝不允许进入发布包** —— build.rs 已保证 release 构建必须提供正式公钥。
#[cfg(debug_assertions)]
const DEV_SEED: [u8; 32] = [
    0x0b, 0x18, 0x3e, 0x55, 0xa1, 0xc4, 0x2f, 0x88, 0x6e, 0xa7, 0xd5, 0x39, 0x2c, 0x77, 0xb1, 0x4e,
    0x90, 0x3a, 0x6f, 0xe8, 0x15, 0x2a, 0xb7, 0xc3, 0x4d, 0x6e, 0x8a, 0x1f, 0x5b, 0x9d, 0x2e, 0x07,
];

/// 许可证错误
#[derive(Debug, Clone, thiserror::Error)]
pub enum LicenseError {
    #[error("许可证签名无效")]
    InvalidSignature,
    #[error("许可证与本设备不匹配,请使用换机功能重新绑定")]
    DeviceMismatch,
    #[error("许可证已过期")]
    Expired,
    #[error("许可证格式错误: {0}")]
    Malformed(String),
    #[error("客户端公钥配置错误: {0}")]
    PublicKey(String),
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

    /// 是否永久授权
    pub fn is_perpetual(&self) -> bool {
        self.expiry == 0
    }

    /// 验证许可证:验签 + 比对 device_id + 检查过期
    pub fn verify(&self, expected_device_id: &str) -> std::result::Result<(), LicenseError> {
        let pk_bytes = public_key()?;
        self.verify_with_key(&pk_bytes, expected_device_id)
    }

    /// 用指定公钥验证。与内置公钥解耦,便于测试与多密钥过渡。
    ///
    /// 顺序有意为之:**先验签,再看设备与有效期**。
    /// 没通过签名的字段一个都不该被信任 —— 否则篡改 expiry 的人会拿到
    /// "已过期"这种看似正常的反馈,而不是"签名被改过"。
    pub fn verify_with_key(
        &self,
        pk_bytes: &[u8; 32],
        expected_device_id: &str,
    ) -> std::result::Result<(), LicenseError> {
        let pk = VerifyingKey::from_bytes(pk_bytes)
            .map_err(|e| LicenseError::PublicKey(format!("公钥不是合法的 ed25519 点: {e}")))?;
        let sig_bytes = B64
            .decode(&self.signature)
            .map_err(|e| LicenseError::Malformed(format!("签名 base64 解码失败: {e}")))?;
        let sig = Signature::from_slice(&sig_bytes)
            .map_err(|e| LicenseError::Malformed(format!("签名格式错误: {e}")))?;
        let msg = self.sign_message();
        pk.verify(msg.as_bytes(), &sig)
            .map_err(|_| LicenseError::InvalidSignature)?;

        if self.device_id != expected_device_id {
            return Err(LicenseError::DeviceMismatch);
        }
        if self.expiry != 0 && chrono::Utc::now().timestamp() > self.expiry {
            return Err(LicenseError::Expired);
        }
        Ok(())
    }

    /// 从 base64 JSON blob 解析
    pub fn from_blob(blob: &str) -> std::result::Result<Self, LicenseError> {
        let json = B64
            .decode(blob.trim())
            .map_err(|e| LicenseError::Malformed(format!("blob base64 解码失败: {e}")))?;
        serde_json::from_slice(&json).map_err(|e| LicenseError::Malformed(e.to_string()))
    }

    /// 序列化为 base64 JSON blob(便于存储/传输)
    pub fn to_blob(&self) -> String {
        let json = serde_json::to_vec(self).unwrap_or_default();
        B64.encode(json)
    }
}

/// 构建期注入的公钥(hex)。release 构建由 build.rs 保证存在。
fn injected_pubkey_hex() -> Option<&'static str> {
    option_env!("PDF_DOCVAULT_LICENSE_PUBKEY_RESOLVED")
}

/// 客户端内置公钥
pub fn public_key() -> std::result::Result<[u8; 32], LicenseError> {
    if let Some(hex_str) = injected_pubkey_hex() {
        let bytes = hex::decode(hex_str.trim())
            .map_err(|e| LicenseError::PublicKey(format!("公钥 hex 解码失败: {e}")))?;
        let arr: [u8; 32] = bytes.try_into().map_err(|v: Vec<u8>| {
            LicenseError::PublicKey(format!("公钥长度应为 32 字节,实际 {}", v.len()))
        })?;
        return Ok(arr);
    }

    #[cfg(debug_assertions)]
    {
        use ed25519_dalek::SigningKey;
        Ok(SigningKey::from_bytes(&DEV_SEED).verifying_key().to_bytes())
    }
    #[cfg(not(debug_assertions))]
    {
        Err(LicenseError::PublicKey(
            "构建时未注入 license 公钥(不应发生:build.rs 会阻止 release 构建)".into(),
        ))
    }
}

/// 当前内置公钥的 hex 表示(诊断用,不含任何私密信息)
pub fn public_key_hex() -> String {
    public_key()
        .map(hex::encode)
        .unwrap_or_else(|_| "未配置".into())
}

/// 当前公钥是否来自构建期注入(而非 debug 开发种子)
pub fn is_production_key() -> bool {
    injected_pubkey_hex().is_some()
}

/// 用给定私钥签发许可证。仅签发端(本地 CLI)使用,客户端构建不启用 `signing` feature。
///
/// `expiry` 传 0 表示永久。
#[cfg(any(debug_assertions, feature = "signing"))]
pub fn sign_license(seed: &[u8; 32], device_id: &str, plan: &str, expiry: i64) -> Result<String> {
    use ed25519_dalek::{Signer, SigningKey};

    let signing = SigningKey::from_bytes(seed);
    let now = chrono::Utc::now().timestamp();
    let msg = format!("{device_id}|{plan}|{now}|{expiry}");
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

/// 开发期:签发测试 license。仅 debug 构建可用。
///
/// 两种情况:
/// - 构建未注入正式公钥(纯本地开发):用内置开发种子签,与自己派生的公钥配对;
/// - 构建已注入正式公钥(本机已生成 `.license`):只有持有私钥才签得出能验过的许可证,
///   此时从环境变量 `PDF_DOCVAULT_LICENSE_KEY` 读取私钥 hex。
///
/// 私钥永远不会被编译进二进制,所以第二种情况必须由显式环境变量提供。
#[cfg(debug_assertions)]
pub fn dev_sign_license(device_id: &str, plan: &str, expiry: i64) -> Result<String> {
    use crate::errors::CoreError;

    if is_production_key() {
        let key = std::env::var("PDF_DOCVAULT_LICENSE_KEY").map_err(|_| {
            CoreError::Activation(
                "当前构建内置了正式公钥,调试签发需要环境变量 PDF_DOCVAULT_LICENSE_KEY(私钥 hex)。\
                 也可改用 `license-gen sign --device <设备指纹>` 离线签发。"
                    .into(),
            )
        })?;
        let seed = crate::activation::voucher::key_from_hex(&key).map_err(CoreError::Activation)?;
        return sign_license(&seed, device_id, plan, expiry);
    }
    sign_license(&DEV_SEED, device_id, plan, expiry)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用独立密钥对,不依赖构建期注入的公钥 —— 这样无论本机有没有生成
    /// `.license`,这些测试测的都是真逻辑,而不是被静默跳过。
    fn test_keypair() -> ([u8; 32], [u8; 32]) {
        use ed25519_dalek::SigningKey;
        let seed = [42u8; 32];
        let pk = SigningKey::from_bytes(&seed).verifying_key().to_bytes();
        (seed, pk)
    }

    fn sign_with_test_key(device: &str, expiry: i64) -> (License, [u8; 32]) {
        let (seed, pk) = test_keypair();
        let blob = sign_license(&seed, device, "full", expiry).unwrap();
        (License::from_blob(&blob).unwrap(), pk)
    }

    #[test]
    fn license_roundtrip() {
        let (lic, pk) = sign_with_test_key("test-device-123", 0);
        lic.verify_with_key(&pk, "test-device-123")
            .expect("自己签的许可证应验签通过");
        assert!(lic.is_perpetual());
    }

    #[test]
    fn device_mismatch_rejected() {
        let (lic, pk) = sign_with_test_key("device-a", 0);
        assert!(matches!(
            lic.verify_with_key(&pk, "device-b"),
            Err(LicenseError::DeviceMismatch)
        ));
    }

    #[test]
    fn tampered_license_rejected() {
        let (mut lic, pk) = sign_with_test_key("device-a", 0);
        lic.plan = "trial".into(); // 改字段但签名不动
        assert!(matches!(
            lic.verify_with_key(&pk, "device-a"),
            Err(LicenseError::InvalidSignature)
        ));
    }

    #[test]
    fn tampered_expiry_rejected() {
        let (mut lic, pk) = sign_with_test_key("device-a", 0);
        lic.expiry = 99; // 想把永久授权改成别的,同样得过签名这关
        assert!(matches!(
            lic.verify_with_key(&pk, "device-a"),
            Err(LicenseError::InvalidSignature)
        ));
    }

    #[test]
    fn expired_license_rejected() {
        let past = chrono::Utc::now().timestamp() - 3600;
        let (lic, pk) = sign_with_test_key("device-a", past);
        assert!(matches!(
            lic.verify_with_key(&pk, "device-a"),
            Err(LicenseError::Expired)
        ));
    }

    #[test]
    fn forged_signature_rejected() {
        // 换一对密钥签,原来的公钥必须验不过
        let (_, pk) = test_keypair();
        let other_seed = [9u8; 32];
        let blob = sign_license(&other_seed, "device-a", "full", 0).unwrap();
        let lic = License::from_blob(&blob).unwrap();
        assert!(matches!(
            lic.verify_with_key(&pk, "device-a"),
            Err(LicenseError::InvalidSignature)
        ));
    }

    #[test]
    fn blob_decode_rejects_garbage() {
        assert!(License::from_blob("not-base64!!!").is_err());
        assert!(License::from_blob(B64.encode(b"{}").as_str()).is_err());
    }

    #[test]
    fn public_key_is_never_all_zero() {
        let pk = public_key().expect("公钥应可解析");
        assert!(pk.iter().any(|&b| b != 0), "全零公钥绝不能作为内置公钥");
    }

    /// 仅在未注入正式公钥时成立:开发种子签的东西要能被内置公钥验过。
    #[test]
    #[cfg(debug_assertions)]
    fn dev_seed_matches_builtin_key() {
        if is_production_key() {
            return; // 内置的是正式公钥,开发种子自然验不过,该场景不适用
        }
        let blob = dev_sign_license("dev-device", "full", 0).unwrap();
        let lic = License::from_blob(&blob).unwrap();
        lic.verify("dev-device")
            .expect("开发种子与内置公钥必须配对");
    }

    /// 内置正式公钥时,没有私钥就不能签发 —— 这正是安全性所在。
    #[test]
    #[cfg(debug_assertions)]
    fn prod_build_refuses_dev_sign_without_key() {
        if !is_production_key() {
            return;
        }
        if std::env::var("PDF_DOCVAULT_LICENSE_KEY").is_ok() {
            return; // 本地显式提供了私钥,不算这个用例
        }
        assert!(dev_sign_license("dev-device", "full", 0).is_err());
    }
}
