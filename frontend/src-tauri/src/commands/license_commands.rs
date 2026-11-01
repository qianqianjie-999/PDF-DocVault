//! 卡密兑换命令
//!
//! 客户端只负责把「卡密 + 设备指纹」发给兑换服务,拿回签名后的许可证。
//! 私钥在服务端,**客户端不持有任何签发能力** —— 因此本地改代码也伪造不出许可证。
//!
//! 兑换成功后许可证落到本地库,之后 `check_activation` 走纯离线验签,不再联网。

use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::user_commands::persist_license;
use crate::error::{AppError, CmdResult};
use crate::state::AppState;
use pdf_docvault_core::activation::{fingerprint, license::License, voucher};

/// 默认兑换服务地址。打包时可用 `PDF_DOCVAULT_REDEEM_URL` 覆盖。
const DEFAULT_REDEEM_URL: &str = "https://docvault-redeem.example.workers.dev";

/// 兑换结果
#[derive(Debug, Serialize)]
pub struct RedeemResult {
    pub plan: String,
    pub expiry: i64,
    pub perpetual: bool,
    /// 剩余换机次数(0 表示再换机需要人工处理)
    pub rebind_left: u32,
    /// 本次是否为新绑定(false = 同一设备重新激活)
    pub newly_bound: bool,
    /// 卡密尾号,用于让用户确认自己用的是哪张卡
    pub code_suffix: String,
}

#[derive(Debug, Deserialize)]
struct RedeemResponse {
    license: String,
    plan: String,
    expiry: i64,
    #[serde(default)]
    perpetual: bool,
    #[serde(default)]
    rebind_left: u32,
    #[serde(default)]
    newly_bound: bool,
    #[serde(default)]
    code_suffix: String,
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    #[serde(default)]
    error: String,
    message: String,
}

/// 用卡密激活(首次绑定本设备)
#[tauri::command]
pub async fn redeem_voucher(state: State<'_, AppState>, code: String) -> CmdResult<RedeemResult> {
    do_redeem(&state, &code, "redeem").await
}

/// 换机/重装:把已绑定卡密改绑到本设备
#[tauri::command]
pub async fn rebind_voucher(state: State<'_, AppState>, code: String) -> CmdResult<RedeemResult> {
    do_redeem(&state, &code, "rebind").await
}

/// 当前兑换服务地址(前端展示 / 排障用)
#[tauri::command]
pub async fn redeem_endpoint() -> CmdResult<String> {
    Ok(endpoint())
}

async fn do_redeem(state: &AppState, raw_code: &str, action: &str) -> CmdResult<RedeemResult> {
    let code = voucher::normalize(raw_code);
    if !voucher::is_valid_format(&code) {
        return Err(AppError {
            code: "VOUCHER_FORMAT".into(),
            message: if code.is_empty() {
                "请输入卡密".into()
            } else if code.len() != voucher::CODE_CHARS {
                format!(
                    "卡密应为 {} 位字符(不含分隔符),当前 {} 位,请检查是否漏抄",
                    voucher::CODE_CHARS,
                    code.len()
                )
            } else {
                "卡密校验不通过,请逐位核对(注意 0/O、1/I/L 容易看错)".into()
            },
        });
    }

    let device_id = fingerprint::device_id()?;
    let url = format!("{}/{}", endpoint(), action);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(8))
        .user_agent(concat!("PDF-DocVault/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| AppError {
            code: "NETWORK".into(),
            message: format!("初始化网络请求失败: {e}"),
        })?;

    let resp = client
        .post(&url)
        .json(&serde_json::json!({ "code": code, "device_id": device_id }))
        .send()
        .await
        .map_err(|e| AppError {
            code: "NETWORK".into(),
            message: if e.is_timeout() {
                "连接兑换服务超时,请检查网络后重试".to_string()
            } else if e.is_connect() {
                "无法连接兑换服务,请检查网络或稍后重试".to_string()
            } else {
                format!("请求兑换服务失败: {e}")
            },
        })?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        let (code_str, message) = match serde_json::from_str::<ErrorResponse>(&body) {
            Ok(err) => (err.error, err.message),
            Err(_) => ("HTTP".to_string(), format!("兑换服务返回异常状态 {status}")),
        };
        return Err(AppError {
            code: format!("REDEEM_{}", code_str.to_uppercase()),
            message,
        });
    }

    let parsed: RedeemResponse = serde_json::from_str(&body).map_err(|e| AppError {
        code: "REDEEM_PARSE".into(),
        message: format!("兑换服务返回内容无法解析: {e}"),
    })?;

    // 服务端签的东西必须自己再验一遍:防止 DNS 劫持/中间人塞个假许可证
    let lic = License::from_blob(&parsed.license).map_err(|e| AppError {
        code: "ACTIVATION".into(),
        message: format!("返回的许可证格式错误: {e}"),
    })?;
    lic.verify(&device_id).map_err(|e| AppError {
        code: "ACTIVATION".into(),
        message: format!("许可证验证失败: {e}"),
    })?;

    persist_license(&state.db, &device_id, &parsed.license, &lic)?;
    tracing::info!(
        "设备 {} 通过卡密尾号 {} 激活成功,套餐 {}",
        device_id,
        parsed.code_suffix,
        parsed.plan
    );

    Ok(RedeemResult {
        plan: parsed.plan,
        expiry: parsed.expiry,
        perpetual: parsed.perpetual || parsed.expiry == 0,
        rebind_left: parsed.rebind_left,
        newly_bound: parsed.newly_bound,
        code_suffix: parsed.code_suffix,
    })
}

fn endpoint() -> String {
    std::env::var("PDF_DOCVAULT_REDEEM_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            option_env!("PDF_DOCVAULT_REDEEM_URL")
                .map(str::to_string)
                .filter(|s| !s.trim().is_empty())
        })
        .unwrap_or_else(|| DEFAULT_REDEEM_URL.to_string())
        .trim_end_matches('/')
        .to_string()
}
