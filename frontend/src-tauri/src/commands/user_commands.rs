//! 用户与激活命令

use rusqlite::params;
use serde::Serialize;
use tauri::State;

use crate::error::CmdResult;
use crate::state::AppState;
use pdf_docvault_core::activation::{self, fingerprint, license::License, qrcode};

#[derive(Serialize)]
pub struct ActivationStatus {
    pub activated: bool,
    pub plan: Option<String>,
    pub device_id: String,
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
                return Ok(ActivationStatus {
                    activated: true,
                    plan: Some(lic.plan),
                    device_id,
                });
            }
        }
    }

    Ok(ActivationStatus {
        activated: false,
        plan: None,
        device_id,
    })
}

#[tauri::command]
pub async fn activate(state: State<'_, AppState>, license_blob: String) -> CmdResult<bool> {
    let device_id = fingerprint::device_id()?;
    let lic = License::from_blob(&license_blob)
        .map_err(|e| crate::error::AppError {
            code: "ACTIVATION".into(),
            message: format!("许可证解析失败: {}", e),
        })?;
    lic.verify(&device_id).map_err(|e| crate::error::AppError {
        code: "ACTIVATION".into(),
        message: e.to_string(),
    })?;

    state.db.with_conn(|conn| {
        conn.execute(
            "INSERT OR REPLACE INTO activations (device_id, license_blob, activated_at, signature, app_version) \
             VALUES (?,?,?,?,?)",
            params![
                device_id,
                license_blob,
                lic.activated_at,
                lic.signature,
                env!("CARGO_PKG_VERSION")
            ],
        )?;
        Ok(())
    })?;
    tracing::info!("设备 {} 已激活", device_id);
    Ok(true)
}

#[tauri::command]
pub async fn get_donation_qrcode() -> CmdResult<qrcode::DonationQrCode> {
    let device_id = fingerprint::device_id()?;
    Ok(qrcode::donation_qrcode(&device_id)?)
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
