//! OCR 文字识别 —— Rust-Python 进程级隔离调用层。
//!
//! 架构:Rust 通过 [`crate::pybridge`] spawn `pycore/main.py`(单任务模式),
//! 协议见 `pycore/runtime/PROTOCOL.md`。Python 侧引擎可插拔(默认 rapidocr,
//! 即 PaddleOCR 官方 PP-OCR 模型的 ONNX 版;可选完整版 paddleocr)。
//!
//! PDF 输入先由 pdfium 逐页渲染为 PNG 临时文件,Python 只处理图片,
//! 进程结束自动清理临时目录。

use image::ImageFormat;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::{CoreError, Result};

/// 进度回调类型(与 image::enhance 一致:current, total)
pub type ProgressFn = dyn Fn(u32, u32) + Send + Sync;

/// OCR 任务选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrOptions {
    /// 指定引擎(rapidocr / rapidocr_onnxruntime / paddleocr),None=自动选择
    #[serde(default)]
    pub engine: Option<String>,
    /// 语言代码(paddleocr 有效:ch/en/jp/kor)
    #[serde(default)]
    pub lang: Option<String>,
    /// PDF 输入时的渲染 DPI(默认 200)
    #[serde(default = "default_dpi")]
    pub dpi: u32,
    /// Python 进程超时秒数(默认 300)
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_dpi() -> u32 {
    200
}
fn default_timeout() -> u64 {
    300
}

impl Default for OcrOptions {
    fn default() -> Self {
        Self {
            engine: None,
            lang: None,
            dpi: default_dpi(),
            timeout_secs: default_timeout(),
        }
    }
}

/// 单行识别结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrLine {
    pub text: String,
    pub score: f32,
    /// 四点包围框(左上/右上/右下/左下)
    #[serde(rename = "box")]
    pub box_points: Vec<[f32; 2]>,
}

/// 单页识别结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrPage {
    pub index: u32,
    pub file: String,
    pub line_count: usize,
    pub lines: Vec<OcrLine>,
    /// 按阅读顺序拼接的纯文本
    pub text: String,
}

/// OCR 任务统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrStats {
    pub engine: String,
    pub pages: Vec<OcrPage>,
    pub elapsed_ms: u128,
    /// 输入中是否包含 PDF(内部已渲染并清理临时文件)
    pub from_pdf_pages: u32,
}

pub use crate::pybridge::resolve_pycore_dir;

// ===== 主流程 =====

/// 引擎可用性自检:返回 (当前引擎, 环境可用引擎列表)。
/// 供前端页面加载时探测 Python 环境是否就绪。
pub fn ocr_selftest(pycore_dir: &Path) -> Result<(String, Vec<String>)> {
    let resp = crate::pybridge::run_pycore(pycore_dir, "selftest", [], 120, || {})?;
    if let Some(e) = resp.error_or_none() {
        return Err(e);
    }
    Ok((
        resp.engine.clone(),
        resp.available.clone().unwrap_or_default(),
    ))
}

/// 对图片(或 PDF,先渲染为图片)执行 OCR 文字识别。
///
/// `pycore_dir` 指向 pycore 目录(含 main.py);进度在 PDF 渲染阶段上报。
pub fn ocr_extract(
    inputs: &[PathBuf],
    pycore_dir: &Path,
    opts: &OcrOptions,
    progress: Option<&ProgressFn>,
) -> Result<OcrStats> {
    let notify = |cur: u32, total: u32| {
        if let Some(cb) = progress {
            cb(cur, total);
        }
    };

    // 1. PDF 输入先渲染为临时 PNG
    let (tmp_dir, images, pdf_pages) = prepare_images(inputs, opts.dpi, &notify)?;

    // 2. spawn Python 子进程(extract:多个 --input)
    let mut args: Vec<(String, String)> = Vec::new();
    if let Some(engine) = &opts.engine {
        args.push(("engine".into(), engine.clone()));
    }
    if let Some(lang) = &opts.lang {
        args.push(("lang".into(), lang.clone()));
    }
    for img in &images {
        args.push(("input".into(), img.to_string_lossy().into_owned()));
    }
    let resp = crate::pybridge::run_pycore(pycore_dir, "extract", args, opts.timeout_secs, || {
        let _ = std::fs::remove_dir_all(&tmp_dir);
    });

    let resp = resp?;
    if resp.ok {
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
    if let Some(e) = resp.error_or_none() {
        return Err(e);
    }
    let stats = OcrStats {
        engine: resp.engine,
        pages: resp.pages,
        elapsed_ms: resp.elapsed_ms,
        from_pdf_pages: pdf_pages,
    };
    notify(images.len() as u32, images.len() as u32);
    Ok(stats)
}

/// 输出转换模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConvertMode {
    /// 拍图转 Word(python-docx)
    Docx,
    /// 表格转 Excel(openpyxl)
    Xlsx,
}

impl ConvertMode {
    /// 对应 pycore main.py 的子命令
    pub fn subcommand(self) -> &'static str {
        match self {
            Self::Docx => "to_docx",
            Self::Xlsx => "to_xlsx",
        }
    }
}

/// 转换统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvertStats {
    /// "docx" | "xlsx"
    pub mode: String,
    pub engine: String,
    pub output: PathBuf,
    pub images: usize,
    /// to_docx:识别总行数
    #[serde(default)]
    pub lines: Option<usize>,
    /// to_xlsx:还原总行数
    #[serde(default)]
    pub rows: Option<usize>,
    /// to_xlsx:每张图的工作表统计(原样透传)
    #[serde(default)]
    pub sheets: Option<serde_json::Value>,
    pub elapsed_ms: u128,
}

/// OCR + 生成文档:拍图转 Word / 表格转 Excel。
///
/// PDF 输入同样先渲染为图片;输出路径由调用方(前端保存对话框)决定。
pub fn ocr_convert(
    inputs: &[PathBuf],
    output: &Path,
    mode: ConvertMode,
    pycore_dir: &Path,
    opts: &OcrOptions,
    progress: Option<&ProgressFn>,
) -> Result<ConvertStats> {
    if inputs.is_empty() {
        return Err(CoreError::InvalidArgument("未提供输入文件".into()));
    }
    let notify = |cur: u32, total: u32| {
        if let Some(cb) = progress {
            cb(cur, total);
        }
    };

    let (tmp_dir, images, _pdf_pages) = prepare_images(inputs, opts.dpi, &notify)?;

    let mut args: Vec<(String, String)> = vec![(
        "output".into(),
        output.to_string_lossy().into_owned(),
    )];
    if let Some(engine) = &opts.engine {
        args.push(("engine".into(), engine.clone()));
    }
    if let Some(lang) = &opts.lang {
        args.push(("lang".into(), lang.clone()));
    }
    for img in &images {
        args.push(("input".into(), img.to_string_lossy().into_owned()));
    }
    let resp = crate::pybridge::run_pycore(
        pycore_dir,
        mode.subcommand(),
        args,
        opts.timeout_secs,
        || {
            let _ = std::fs::remove_dir_all(&tmp_dir);
        },
    )?;
    if resp.ok {
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
    if let Some(e) = resp.error_or_none() {
        return Err(e);
    }

    Ok(ConvertStats {
        mode: match mode {
            ConvertMode::Docx => "docx".into(),
            ConvertMode::Xlsx => "xlsx".into(),
        },
        engine: resp.engine,
        // 优先采用 Python 端回报的实际输出路径
        output: resp.output.unwrap_or_else(|| output.to_path_buf()),
        images: resp.images.unwrap_or(images.len()),
        lines: resp.lines,
        rows: resp.rows,
        sheets: resp.sheets,
        elapsed_ms: resp.elapsed_ms,
    })
}

/// 把输入中的 PDF 渲染为临时 PNG,返回 (临时目录, 页面图片列表, PDF 输入数)。
/// 供 OCR 与去手写(image/handwriting.rs)共用。
pub(crate) fn prepare_images(
    inputs: &[PathBuf],
    dpi: u32,
    notify: &impl Fn(u32, u32),
) -> Result<(PathBuf, Vec<PathBuf>, u32)> {
    if inputs.is_empty() {
        return Err(CoreError::InvalidArgument("未提供输入文件".into()));
    }
    let tmp_dir = std::env::temp_dir().join(format!(
        "pdf-docvault-ocr-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let mut images: Vec<PathBuf> = Vec::new();
    let mut pdf_pages = 0u32;
    let total_inputs = inputs.len() as u32;
    for (i, input) in inputs.iter().enumerate() {
        if !input.is_file() {
            return Err(CoreError::Ocr(format!("输入文件不存在: {}", input.display())));
        }
        if is_pdf(input) {
            pdf_pages += 1;
            let out_dir = tmp_dir.join(format!("pdf-{i}"));
            let pages = crate::pdf::render::render_all_pages(
                input,
                &out_dir,
                dpi,
                ImageFormat::Png,
                None,
                |cur, total| notify(cur, total.max(1) * total_inputs),
            )?;
            // 渲染页重命名为 "<源文件名>_p001.png":该文件名会成为
            // Word 节标题(docx_builder)与 Excel 工作表名(xlsx_builder),
            // 保持来源可辨;Python 侧再做非法字符清洗。
            let stem = input
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("page")
                .to_owned();
            for (pi, p) in pages.into_iter().enumerate() {
                let ext = p
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("png")
                    .to_owned();
                let dst = p.with_file_name(format!("{stem}_p{:03}.{ext}", pi + 1));
                if std::fs::rename(&p, &dst).is_ok() {
                    images.push(dst);
                } else {
                    images.push(p);
                }
            }
        } else {
            images.push(input.clone());
        }
        notify((i + 1) as u32, total_inputs);
    }
    if images.is_empty() {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(CoreError::InvalidArgument("没有可识别的页面".into()));
    }
    Ok((tmp_dir, images, pdf_pages))
}

/// 是否 PDF 文件(按扩展名判断,大小写不敏感)
fn is_pdf(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("pdf"))
        .unwrap_or(false)
}

// ===== 单元测试 =====

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_is_pdf() {
        assert!(is_pdf(Path::new("/a/b.PDF")));
        assert!(is_pdf(Path::new("/a/b.pdf")));
        assert!(!is_pdf(Path::new("/a/b.png")));
        assert!(!is_pdf(Path::new("/a/b")));
    }

    /// 端到端真识别:需要 pycore/.venv(rapidocr)与 /tmp/ocr-test-zh.png 测试图。
    /// 运行:`cargo test -p pdf_docvault_core ocr_extract_end_to_end -- --ignored --nocapture`
    #[test]
    #[ignore = "需要 pycore/.venv 与 OCR 模型,端到端验证用"]
    fn test_ocr_extract_end_to_end() {
        let pycore = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../pycore");
        let img = Path::new("/tmp/ocr-test-zh.png");
        if !pycore.join("main.py").is_file() || !img.is_file() {
            println!("跳过:缺少 pycore 或测试图");
            return;
        }
        let stats = ocr_extract(&[img.to_path_buf()], &pycore, &OcrOptions::default(), None)
            .expect("识别应成功");
        println!("engine={}, elapsed_ms={}", stats.engine, stats.elapsed_ms);
        println!("text:\n{}", stats.pages[0].text);
        assert_eq!(stats.pages.len(), 1);
        assert!(stats.pages[0].text.contains("密档大师"), "识别结果应包含'密档大师'");
    }

    /// 端到端转换:表格图 → xlsx。需要 pycore/.venv。
    #[test]
    #[ignore = "需要 pycore/.venv 与 OCR 模型,端到端验证用"]
    fn test_ocr_convert_end_to_end() {
        let pycore = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../pycore");
        let img = Path::new("/tmp/ocr-test-table.png");
        if !pycore.join("main.py").is_file() || !img.is_file() {
            println!("跳过:缺少 pycore 或测试图");
            return;
        }
        let out = std::env::temp_dir().join("pdf-docvault-convert-test.xlsx");
        let stats = ocr_convert(
            &[img.to_path_buf()],
            &out,
            ConvertMode::Xlsx,
            &pycore,
            &OcrOptions::default(),
            None,
        )
        .expect("转换应成功");
        println!("mode={}, rows={:?}, elapsed_ms={}", stats.mode, stats.rows, stats.elapsed_ms);
        assert!(out.is_file(), "xlsx 应已生成");
        assert_eq!(stats.rows, Some(4), "应还原 4 行表格");
        let _ = std::fs::remove_file(&out);
    }

    /// PDF 转 Word/Excel 的页面命名:渲染页应重命名为 "<源文件名>_p001.png"。
    /// 需要两页测试图(合成 PDF)与 pdfium;缺文件自动跳过。
    #[test]
    #[ignore = "需要 pdfium 与测试图,端到端验证用"]
    fn test_prepare_images_pdf_naming() {
        let zh = Path::new("/tmp/ocr-test-zh.png");
        let table = Path::new("/tmp/ocr-test-table.png");
        if !zh.is_file() || !table.is_file() {
            println!("跳过:缺少测试图");
            return;
        }
        // 两张图合成 2 页 PDF
        let pdf = std::env::temp_dir().join("pdf-docvault-naming-test.pdf");
        crate::image::images_to_pdf(&[zh.to_path_buf(), table.to_path_buf()], &pdf, 150.0, &Default::default(), None)
            .expect("合成 PDF 应成功");

        let (tmp_dir, images, pdf_pages) =
            prepare_images(&[pdf.clone()], 150, &|_, _| {}).expect("渲染应成功");
        assert_eq!(pdf_pages, 1);
        assert_eq!(images.len(), 2, "PDF 两页应渲染出两张图");
        assert!(images[0].to_string_lossy().contains("naming-test_p001.png"), "第 1 页命名: {:?}", images[0]);
        assert!(images[1].to_string_lossy().contains("naming-test_p002.png"), "第 2 页命名: {:?}", images[1]);
        assert!(images.iter().all(|p| p.is_file()), "重命名后的文件应存在");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        let _ = std::fs::remove_file(&pdf);
    }
}
