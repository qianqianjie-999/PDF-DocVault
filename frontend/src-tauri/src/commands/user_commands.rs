//! 用户与激活命令

use rusqlite::params;
use serde::Serialize;
use tauri::State;

use crate::error::CmdResult;
use crate::state::AppState;
use pdf_docvault_core::activation::{self, fingerprint, license::License, purchase};
use pdf_docvault_core::storage::Database;

#[derive(Serialize)]
pub struct ActivationStatus {
    pub activated: bool,
    pub plan: Option<String>,
    pub device_id: String,
    /// 激活时间戳(未激活为 None)
    pub activated_at: Option<i64>,
    /// 过期时间戳(0 = 永久)
    pub expiry: Option<i64>,
    pub perpetual: bool,
}

#[tauri::command]
pub async fn check_activation(state: State<'_, AppState>) -> CmdResult<ActivationStatus> {
    let device_id = fingerprint::device_id()?;
    let blob: Option<String> = state.db.with_conn(|conn| {
        let row = conn.query_row(
            "SELECT license_blob FROM activations WHERE device_id = ?",
            [&device_id],
            |r| r.get::<_, String>(0),
        );
        match row {
            Ok(s) => Ok(Some(s)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    })?;

    if let Some(blob) = blob {
        if let Ok(lic) = License::from_blob(&blob) {
            if lic.verify(&device_id).is_ok() {
                let perpetual = lic.is_perpetual();
                return Ok(ActivationStatus {
                    activated: true,
                    plan: Some(lic.plan),
                    device_id,
                    activated_at: Some(lic.activated_at),
                    expiry: Some(lic.expiry),
                    perpetual,
                });
            }
        }
    }

    Ok(ActivationStatus {
        activated: false,
        plan: None,
        device_id,
        activated_at: None,
        expiry: None,
        perpetual: false,
    })
}

#[tauri::command]
pub async fn activate(state: State<'_, AppState>, license_blob: String) -> CmdResult<bool> {
    let device_id = fingerprint::device_id()?;
    let lic = License::from_blob(&license_blob).map_err(|e| crate::error::AppError {
        code: "ACTIVATION".into(),
        message: format!("许可证解析失败: {}", e),
    })?;
    lic.verify(&device_id).map_err(|e| crate::error::AppError {
        code: "ACTIVATION".into(),
        message: e.to_string(),
    })?;

    persist_license(&state.db, &device_id, &license_blob, &lic)?;
    tracing::info!("设备 {} 已通过离线许可证激活", device_id);
    Ok(true)
}

/// 把许可证写进本地库。卡密兑换和离线粘贴两条路径共用。
pub(crate) fn persist_license(
    db: &Database,
    device_id: &str,
    blob: &str,
    lic: &License,
) -> CmdResult<()> {
    db.with_conn(|conn| {
        conn.execute(
            "INSERT OR REPLACE INTO activations (device_id, license_blob, activated_at, signature, app_version) \
             VALUES (?,?,?,?,?)",
            params![
                device_id,
                blob,
                lic.activated_at,
                lic.signature,
                env!("CARGO_PKG_VERSION")
            ],
        )?;
        Ok(())
    })?;
    Ok(())
}

#[tauri::command]
pub async fn get_purchase_info() -> CmdResult<purchase::PurchaseInfo> {
    let device_id = fingerprint::device_id()?;
    Ok(purchase::purchase_info(&device_id)?)
}

#[tauri::command]
pub async fn get_device_info() -> CmdResult<fingerprint::DeviceInfo> {
    Ok(fingerprint::device_info()?)
}

/// 开发期辅助:生成测试 license(仅 debug 构建可用)
#[tauri::command]
pub async fn dev_generate_license() -> CmdResult<String> {
    #[cfg(debug_assertions)]
    {
        let device_id = fingerprint::device_id()?;
        let blob = activation::license::dev_sign_license(&device_id, "full", 0)
            .map_err(|e| crate::error::AppError {
                code: "ACTIVATION".into(),
                message: e.to_string(),
            })?;
        Ok(blob)
    }
    #[cfg(not(debug_assertions))]
    {
        Err(crate::error::AppError {
            code: "ACTIVATION".into(),
            message: "此命令仅在 debug 构建可用".into(),
        })
    }
}
