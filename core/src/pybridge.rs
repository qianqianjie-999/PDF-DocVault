//! Rust-Python 进程桥 —— pycore 子进程调用的公共基建。
//!
//! 协议见 `pycore/runtime/PROTOCOL.md`:单任务模式,stdout 恰好一行 JSON,
//! stderr 承载日志;超时 kill 进程。OCR(ocr.rs)、图像修复(image/handwriting.rs)
//! 与办公文档转换(office.rs)共用本模块。

use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::{CoreError, Result};

/// stderr 逐行回调(在读取线程内调用,需 Send + 'static)
pub(crate) type StderrLineHandler = Box<dyn FnMut(&str) + Send>;

/// 中止检查回调:返回 true 表示任务已取消,应立即终止子进程。
/// 由命令层注入(读 TaskManager::is_cancelled),在主线程轮询中被同步调用。
pub(crate) type AbortCheck = dyn Fn() -> bool + Send + Sync;

// ===== 协议反序列化(pycore/main.py 输出) =====

#[derive(Clone, Deserialize)]
pub(crate) struct PyError {
    pub code: String,
    pub message: String,
}

/// pycore 协议响应(字段全部宽松可选,兼容 extract/to_docx/to_xlsx/selftest/remove_handwriting)
#[derive(Deserialize)]
pub(crate) struct PyResponse {
    pub ok: bool,
    #[serde(default)]
    pub engine: String,
    #[serde(default)]
    pub pages: Vec<crate::ocr::OcrPage>,
    #[serde(default)]
    pub elapsed_ms: u128,
    #[serde(default)]
    pub output: Option<PathBuf>,
    /// remove_handwriting:清理后的图片路径列表
    #[serde(default)]
    pub outputs: Option<Vec<PathBuf>>,
    #[serde(default)]
    pub images: Option<usize>,
    #[serde(default)]
    pub lines: Option<usize>,
    #[serde(default)]
    pub rows: Option<usize>,
    #[serde(default)]
    pub sheets: Option<serde_json::Value>,
    /// remove_handwriting:笔迹像素占比(0-1)
    #[serde(default)]
    pub mask_ratio: Option<f32>,
    /// selftest:环境可用引擎列表
    #[serde(default)]
    pub available: Option<Vec<String>>,
    /// 通用结构化报告(Word→PDF 的成功/失败清单等)
    #[serde(default)]
    pub report: Option<serde_json::Value>,
    pub error: Option<PyError>,
}

impl PyResponse {
    /// 把失败响应转为 CoreError(成功返回 None)。
    pub fn error_or_none(&self) -> Option<CoreError> {
        if self.ok {
            return None;
        }
        let e = self.error.clone().unwrap_or(PyError {
            code: "PYTHON_CRASH".into(),
            message: "未知错误".into(),
        });
        Some(CoreError::Ocr(format!("[{}] {}", e.code, e.message)))
    }
}

/// 从 stdout 提取协议 JSON:取最后一个能解析的 `{...}` 行(容忍引擎日志混入)。
pub(crate) fn parse_response(stdout: &str) -> Option<PyResponse> {
    stdout
        .lines()
        .rev()
        .filter_map(|l| {
            let t = l.trim();
            if t.starts_with('{') && t.ends_with('}') {
                serde_json::from_str::<PyResponse>(t).ok()
            } else {
                None
            }
        })
        .next()
}

// ===== Python 解释器探测 =====

/// 依序探测:环境变量 `PYCORE_PYTHON` → 随包内置解释器 → `pycore/.venv/bin/python`
/// → PATH 中的 python3。
///
/// 内置解释器排在 venv 之前:打包版目录里永远带 `python/`,而用户机上不会有 venv;
/// 开发机上两者都可能存在,此时以「能独立运行」的内置运行时为准,避免误用
/// 绑定了系统路径的 venv。
///
/// 公开导出以便集成测试与诊断工具按同一规则定位解释器。
pub fn resolve_python(pycore_dir: &Path) -> Result<PathBuf> {
    let main_py = pycore_dir.join("main.py");
    if !main_py.is_file() {
        return Err(CoreError::Ocr(format!(
            "未找到 pycore 入口: {}",
            main_py.display()
        )));
    }
    if let Ok(p) = std::env::var("PYCORE_PYTHON") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok(p);
        }
    }
    for rel in bundled_python_candidates() {
        let p = pycore_dir.join(rel);
        if p.is_file() {
            return Ok(p);
        }
    }
    let venv = pycore_dir.join(".venv/bin/python");
    if venv.is_file() {
        return Ok(venv);
    }
    find_in_path("python3").ok_or_else(|| {
        CoreError::Ocr(
            "未找到 Python 解释器:安装包缺少内置运行时,或设置 PYCORE_PYTHON 环境变量".into(),
        )
    })
}

/// 随包内置解释器的候选相对路径(不同平台/构建方式布局不同)。
///
/// 由 `scripts/pack-pycore-runtime.sh` 产出,统一放在 `pycore/python/` 下。
fn bundled_python_candidates() -> &'static [&'static str] {
    &[
        "python/bin/python3",
        "python/python.exe",
        "python/bin/python",
    ]
}

/// 在 PATH 中查找可执行文件
fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

/// 定位 pycore 目录:环境变量 `PYCORE_DIR` → 可执行文件同级 `resources/pycore` → 开发期回退。
pub fn resolve_pycore_dir() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("PYCORE_DIR") {
        let d = PathBuf::from(p);
        if d.join("main.py").is_file() {
            return Ok(d);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        let candidates = [
            exe.parent().unwrap().join("resources/pycore"),
            exe.parent().unwrap().join("pycore"),
        ];
        for d in candidates {
            if d.join("main.py").is_file() {
                return Ok(d);
            }
        }
    }
    // 开发期回退:cargo workspace 根(编译期路径,仅本地开发有效)
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../pycore");
    if dev.join("main.py").is_file() {
        return Ok(dev.canonicalize().unwrap_or(dev));
    }
    Err(CoreError::Ocr(
        "未找到 pycore 目录:请设置 PYCORE_DIR 环境变量".into(),
    ))
}

// ===== 子进程执行(超时 + stderr 实时监听) =====

/// spawn 子进程并等待(带超时),stderr 每产生一行即实时回调,
/// 同时收集 stdout 全文与 stderr 全文。
///
/// 回调为 None 时等价于普通执行(读线程在进程退出/被杀后因 EOF 返回,
/// 避免管道死锁);超时 kill 进程。
///
/// `stdin_payload` 为 Some 时把内容写入子进程 stdin 并关闭管道。
/// 用于传递密码这类不能进命令行参数的敏感值。
pub(crate) fn spawn_with_timeout_watched(
    mut cmd: Command,
    timeout_secs: u64,
    stdin_payload: Option<String>,
    on_stderr_line: Option<StderrLineHandler>,
    abort: Option<&AbortCheck>,
) -> Result<(std::process::ExitStatus, String, String)> {
    use std::io::{BufRead, BufReader, Write};
    use std::thread;

    let mut child = cmd
        .spawn()
        .map_err(|e| CoreError::Ocr(format!("无法启动 Python 进程: {e}")))?;

    if let Some(payload) = stdin_payload {
        if let Some(mut sin) = child.stdin.take() {
            // 写失败多半是子进程提前退出(参数非法等),错误留给退出码与 stdout 判定,
            // 这里不抢先行报错,避免掩盖真正的失败原因。
            let _ = sin.write_all(payload.as_bytes());
            // sin 在此作用域结束被 drop → 关闭管道,子进程读到 EOF
        }
    }

    let mut out_pipe = child.stdout.take().expect("stdout 已设为 piped");
    let err_pipe = child.stderr.take().expect("stderr 已设为 piped");

    let out_handle = thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut out_pipe, &mut s);
        s
    });

    // stderr 逐行读取并实时触发回调,同时累积全文供错误报告使用
    let err_handle = thread::spawn(move || {
        let mut reader = BufReader::new(err_pipe);
        let mut all = String::new();
        let mut cb = on_stderr_line;
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    all.push_str(&line);
                    if let Some(f) = cb.as_deref_mut() {
                        f(line.trim_end_matches(['\r', '\n']));
                    }
                }
                Err(_) => break,
            }
        }
        all
    });

    // wait 放独立线程,主线程超时后 kill
    let child = Arc::new(Mutex::new(child));
    let (tx, rx) = mpsc::channel();
    let waiter_child = Arc::clone(&child);
    thread::spawn(move || {
        let status = waiter_child.lock().unwrap().wait();
        let _ = tx.send(status);
    });

    let timeout = Duration::from_secs(timeout_secs.max(1));
    let poll = Duration::from_millis(200);
    let mut waited = Duration::ZERO;
    let status = loop {
        // 每轮先查取消标志(命令层经 TaskManager 注入),命中即终止子进程
        if abort.map(|a| a()).unwrap_or(false) {
            let _ = child.lock().unwrap().kill();
            let _ = child.lock().unwrap().wait();
            return Err(CoreError::TaskCancelled);
        }
        match rx.recv_timeout(poll) {
            Ok(st) => break st.map_err(|e| CoreError::Ocr(format!("等待进程失败: {e}")))?,
            Err(RecvTimeoutError::Timeout) => {
                waited += poll;
                if waited >= timeout {
                    let _ = child.lock().unwrap().kill();
                    let _ = child.lock().unwrap().wait();
                    return Err(CoreError::Ocr(format!(
                        "OCR_TIMEOUT:Python 进程超过 {timeout_secs}s 未完成,已终止"
                    )));
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(CoreError::Ocr("等待进程失败:通道断开".into()));
            }
        }
    };
    let stdout = out_handle.join().unwrap_or_default();
    let stderr = err_handle.join().unwrap_or_default();
    Ok((status, stdout, stderr))
}

// ===== pycore 子命令组装 =====

/// 组装并执行 pycore 子命令,统一处理超时/解析/清理。
///
/// `cleanup` 在所有错误路径上调用(成功路径由调用方决定保留策略)。
pub(crate) fn run_pycore(
    pycore_dir: &Path,
    subcommand: &str,
    input_args: impl IntoIterator<Item = (String, String)>,
    timeout_secs: u64,
    cleanup: impl Fn(),
) -> Result<PyResponse> {
    run_pycore_inner(
        pycore_dir,
        subcommand,
        input_args,
        timeout_secs,
        cleanup,
        None,
        None,
        None,
    )
}

/// 与 [`run_pycore`] 相同,但把 `stdin_payload` 写进子进程 stdin。
///
/// 用于传递密码:命令行参数对同用户的其他进程可见(`/proc/<pid>/cmdline`),
/// 而 stdin 走管道,进程结束后不留痕。payload 约定为一整行 JSON。
pub(crate) fn run_pycore_stdin(
    pycore_dir: &Path,
    subcommand: &str,
    input_args: impl IntoIterator<Item = (String, String)>,
    stdin_payload: String,
    timeout_secs: u64,
    cleanup: impl Fn(),
) -> Result<PyResponse> {
    run_pycore_inner(
        pycore_dir,
        subcommand,
        input_args,
        timeout_secs,
        cleanup,
        Some(stdin_payload),
        None,
        None,
    )
}

/// 与 [`run_pycore`] 相同,但 stderr 每行实时回调(批量任务进度)。
pub(crate) fn run_pycore_watched(
    pycore_dir: &Path,
    subcommand: &str,
    input_args: impl IntoIterator<Item = (String, String)>,
    timeout_secs: u64,
    cleanup: impl Fn(),
    on_stderr_line: Box<dyn FnMut(&str) + Send>,
) -> Result<PyResponse> {
    run_pycore_inner(
        pycore_dir,
        subcommand,
        input_args,
        timeout_secs,
        cleanup,
        None,
        Some(on_stderr_line),
        None,
    )
}

/// 与 [`run_pycore`] 相同,但支持通过 `abort` 回调随时中断子进程(任务取消)。
/// `abort` 为 None 时退化为普通超时执行。
pub(crate) fn run_pycore_cancellable(
    pycore_dir: &Path,
    subcommand: &str,
    input_args: impl IntoIterator<Item = (String, String)>,
    timeout_secs: u64,
    cleanup: impl Fn(),
    abort: Option<&AbortCheck>,
) -> Result<PyResponse> {
    run_pycore_inner(
        pycore_dir,
        subcommand,
        input_args,
        timeout_secs,
        cleanup,
        None,
        None,
        abort,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_pycore_inner(
    pycore_dir: &Path,
    subcommand: &str,
    input_args: impl IntoIterator<Item = (String, String)>,
    timeout_secs: u64,
    cleanup: impl Fn(),
    stdin_payload: Option<String>,
    on_stderr_line: Option<StderrLineHandler>,
    abort: Option<&AbortCheck>,
) -> Result<PyResponse> {
    let python = resolve_python(pycore_dir)?;
    let mut cmd = Command::new(&python);
    cmd.arg(pycore_dir.join("main.py"))
        .arg(subcommand)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(if stdin_payload.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
    for (k, v) in input_args {
        cmd.arg(format!("--{k}")).arg(v);
    }

    let (status, stdout, stderr) =
        match spawn_with_timeout_watched(cmd, timeout_secs, stdin_payload, on_stderr_line, abort) {
            Ok(r) => r,
            Err(e) => {
                cleanup();
                return Err(e);
            }
        };

    match parse_response(&stdout) {
        Some(r) => {
            if !r.ok {
                cleanup();
            }
            Ok(r)
        }
        None => {
            cleanup();
            let tail: String = stderr
                .lines()
                .rev()
                .take(8)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");
            if status.success() {
                return Err(CoreError::Ocr(format!(
                    "pycore 输出无法解析(stdout 前 200 字: {:.200})",
                    stdout.trim()
                )));
            }
            Err(CoreError::Ocr(format!(
                "Python 进程失败(exit={status}):{tail}"
            )))
        }
    }
}
