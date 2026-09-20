//! 办公文档格式转换(当前:批量 Word → PDF)。
//!
//! 引擎链见 `pycore/office/word_to_pdf.py`:Windows Word COM → WPS COM →
//! LibreOffice;Linux/macOS LibreOffice → WPS CLI。进度经 pycore stderr 行实时回调。

use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::{CoreError, Result};

// ===== 质量等级 =====

/// 输出 PDF 质量等级(对应不同图片 DPI 上限与 JPEG 质量)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordQuality {
    /// 原版图片分辨率,JPEG 90%
    High,
    /// 图片 ≤150 DPI,JPEG 80%
    Medium,
    /// 图片 ≤96 DPI,JPEG 60%
    Low,
}

impl WordQuality {
    pub fn as_str(self) -> &'static str {
        match self {
            WordQuality::High => "high",
            WordQuality::Medium => "medium",
            WordQuality::Low => "low",
        }
    }

    /// 由 pycore 协议字符串解析
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "high" => Ok(WordQuality::High),
            "medium" => Ok(WordQuality::Medium),
            "low" => Ok(WordQuality::Low),
            other => Err(CoreError::InvalidArgument(format!(
                "未知质量等级: {other}(应为 high/medium/low)"
            ))),
        }
    }
}

// ===== 结果报告 =====

/// 单个成功/失败条目
#[derive(Clone, Deserialize)]
struct Item {
    input: String,
    output: Option<String>,
    reason: Option<String>,
}

#[derive(Deserialize)]
struct BatchReport {
    engine: String,
    succeeded: Vec<Item>,
    failed: Vec<Item>,
}

/// 对前端的转换报告(成功/失败清单 + 失败原因)
#[derive(Clone)]
pub struct WordConvertReport {
    pub engine: String,
    pub succeeded: Vec<(String, String)>,
    pub failed: Vec<(String, String)>,
    pub elapsed_ms: u128,
    pub output_dir: PathBuf,
}

impl WordConvertReport {
    pub fn success_count(&self) -> usize {
        self.succeeded.len()
    }
    pub fn fail_count(&self) -> usize {
        self.failed.len()
    }
}

// ===== 自检 =====

#[derive(Deserialize)]
struct EnginesReport {
    libreoffice: Option<String>,
    word_com: bool,
    #[serde(default)]
    wps_com: bool,
    #[serde(default)]
    wps_cli: Option<String>,
}

/// 引擎自检结果
pub struct OfficeSelftest {
    pub libreoffice: Option<String>,
    pub word_com: bool,
    pub wps_com: bool,
    pub wps_cli: Option<String>,
}

pub fn resolve_pycore_dir() -> Result<PathBuf> {
    crate::pybridge::resolve_pycore_dir()
}

/// 探测 LibreOffice / Word COM / WPS(COM 与 CLI)可用性
pub fn office_selftest(pycore_dir: &Path) -> Result<OfficeSelftest> {
    let resp = crate::pybridge::run_pycore(pycore_dir, "office_selftest", [], 60, || {})?;
    let value = resp
        .report
        .ok_or_else(|| CoreError::Internal("office_selftest 未返回报告".into()))?;
    let parsed: EnginesReport = serde_json::from_value(value)?;
    Ok(OfficeSelftest {
        libreoffice: parsed.libreoffice,
        word_com: parsed.word_com,
        wps_com: parsed.wps_com,
        wps_cli: parsed.wps_cli,
    })
}

// ===== 批量转换 =====

/// 解析 pycore 进度行:`[PROGRESS] <current> <total> <文件名>`
fn parse_progress_line(line: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 3 && parts[0] == "[PROGRESS]" {
        if let (Ok(current), Ok(total)) = (parts[1].parse::<u32>(), parts[2].parse::<u32>()) {
            return Some((current, total));
        }
    }
    None
}

/// 批量 Word(.doc/.docx)转 PDF。
///
/// - `output_dir`:PDF 统一输出目录,同名自动加 `~N` 后缀
/// - `on_progress`:文件级实时进度(当前序号,总文件数),无需进度可传 None
pub fn word_to_pdf<C>(
    inputs: &[PathBuf],
    output_dir: &Path,
    quality: WordQuality,
    timeout_secs: u64,
    on_progress: Option<C>,
) -> Result<WordConvertReport>
where
    // owned 回调:move 进函数后可再移入子进程读线程(满足 'static)
    C: Fn(u32, u32) + Send + Sync + 'static,
{
    if inputs.is_empty() {
        return Err(CoreError::InvalidArgument("未选择任何 Word 文件".into()));
    }
    let pycore_dir = resolve_pycore_dir()?;

    // pycore 参数:每文件一个 --input,加输出目录与质量等级
    let mut args: Vec<(String, String)> = inputs
        .iter()
        .map(|p| ("input".to_string(), p.to_string_lossy().into_owned()))
        .collect();
    args.push((
        "output-dir".to_string(),
        output_dir.to_string_lossy().into_owned(),
    ));
    args.push(("quality".to_string(), quality.as_str().to_string()));

    // 把文件级回调包装成 stderr 行监听器(读线程内调用,需 Send + 'static)
    let watcher: Option<Box<dyn FnMut(&str) + Send>> = on_progress.map(|cb| {
        Box::new(move |line: &str| {
            if let Some((current, total)) = parse_progress_line(line) {
                cb(current, total);
            }
        }) as Box<dyn FnMut(&str) + Send>
    });

    let resp = match watcher {
        Some(cb) => crate::pybridge::run_pycore_watched(
            &pycore_dir,
            "word_to_pdf",
            args,
            timeout_secs,
            || {},
            cb,
        )?,
        None => crate::pybridge::run_pycore(&pycore_dir, "word_to_pdf", args, timeout_secs, || {})?,
    };

    let value = resp
        .report
        .ok_or_else(|| CoreError::Internal("word_to_pdf 未返回报告".into()))?;
    let report: BatchReport = serde_json::from_value(value)?;

    let succeeded = report
        .succeeded
        .into_iter()
        .map(|i| (i.input, i.output.unwrap_or_default()))
        .collect();
    let failed = report
        .failed
        .into_iter()
        .map(|i| (i.input, i.reason.unwrap_or_else(|| "未知错误".into())))
        .collect();

    Ok(WordConvertReport {
        engine: report.engine,
        succeeded,
        failed,
        elapsed_ms: resp.elapsed_ms,
        output_dir: output_dir.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_progress_line() {
        assert_eq!(parse_progress_line("[PROGRESS] 2 5 报告.docx"), Some((2, 5)));
        assert_eq!(parse_progress_line("其他日志"), None);
        assert_eq!(parse_progress_line("[PROGRESS] x 5"), None);
    }

    #[test]
    fn test_quality_roundtrip() {
        for q in [WordQuality::High, WordQuality::Medium, WordQuality::Low] {
            assert_eq!(WordQuality::parse(q.as_str()).unwrap(), q);
        }
        assert!(WordQuality::parse("ultra").is_err());
    }
}
