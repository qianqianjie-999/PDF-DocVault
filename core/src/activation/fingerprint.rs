//! 设备指纹
//!
//! 基于 `machine-uid` 读取跨平台机器 ID(Linux /etc/machine-id、Windows MachineGuid、
//! macOS IOPlatformUUID),加盐 SHA256 后作为设备指纹。

use sha2::{Digest, Sha256};

use crate::errors::{CoreError, Result};

/// 设备信息
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceInfo {
    pub device_id: String,
    pub os_name: String,
    pub os_version: String,
    pub hostname: String,
    pub cpu_brand: String,
    pub arch: String,
}

/// 生成设备指纹(SHA256 哈希,16 进制)
pub fn device_id() -> Result<String> {
    let raw = machine_uid::get().map_err(|e| CoreError::Activation(format!("读取机器 ID 失败: {e}")))?;
    let mut hasher = Sha256::new();
    hasher.update(b"pdf-docvault::v1::");
    hasher.update(raw.as_bytes());
    Ok(format!("{:x}", hasher.finalize()))
}

/// 获取设备完整信息
pub fn device_info() -> Result<DeviceInfo> {
    Ok(DeviceInfo {
        device_id: device_id()?,
        os_name: std::env::consts::OS.to_string(),
        os_version: os_version(),
        hostname: hostname(),
        cpu_brand: cpu_brand(),
        arch: std::env::consts::ARCH.to_string(),
    })
}

fn os_version() -> String {
    #[cfg(target_os = "linux")]
    {
        if let Ok(s) = std::fs::read_to_string("/etc/os-release") {
            for line in s.lines() {
                if let Some(v) = line.strip_prefix("PRETTY_NAME=") {
                    return v.trim_matches('"').to_string();
                }
            }
        }
        "unknown".to_string()
    }
    #[cfg(not(target_os = "linux"))]
    {
        std::env::consts::OS.to_string()
    }
}

fn hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| {
            #[cfg(target_os = "linux")]
            {
                std::fs::read_to_string("/etc/hostname").map(|s| s.trim().to_string())
            }
            #[cfg(not(target_os = "linux"))]
            {
                Ok("unknown".to_string())
            }
        })
        .unwrap_or_else(|_| "unknown".to_string())
}

fn cpu_brand() -> String {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/cpuinfo")
            .map(|s| {
                s.lines()
                    .find_map(|l| {
                        l.strip_prefix("model name")
                            .and_then(|r| r.split(':').nth(1))
                            .map(|v| v.trim().to_string())
                    })
                    .unwrap_or_else(|| "unknown".to_string())
            })
            .unwrap_or_else(|_| "unknown".to_string())
    }
    #[cfg(not(target_os = "linux"))]
    {
        "unknown".to_string()
    }
}
