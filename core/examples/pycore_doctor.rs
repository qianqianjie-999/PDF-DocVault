//! 诊断工具:确认加密/OCR/Office 赖以工作的 Python 链路是否就绪。
//!
//! 打包版最常见的一类故障是「解释器找到了,但找的是系统 Python 或绑死路径的
//! 开发 venv」,表现是模块缺失、或换台机器就崩。这个工具把实际选中的路径打印
//! 出来,并真跑一次 pycore 入口。
//!
//! 用法:
//!   cargo run -p pdf_docvault_core --example pycore_doctor
//!   PYCORE_DIR=<安装包 resources/pycore> cargo run -p pdf_docvault_core --example pycore_doctor

use pdf_docvault_core::pybridge;

fn main() {
    println!("=== pycore 链路诊断 ===\n");

    let dir = match pybridge::resolve_pycore_dir() {
        Ok(d) => d,
        Err(e) => {
            println!("[FAIL] 定位 pycore 目录失败: {e}");
            println!("\n安装包内应存在 <resources>/pycore/main.py");
            println!("可设 PYCORE_DIR 指过去再跑一次。");
            std::process::exit(1);
        }
    };
    println!("[ OK ] pycore 目录: {}", dir.display());

    let py = match pybridge::resolve_python(&dir) {
        Ok(p) => p,
        Err(e) => {
            println!("[FAIL] 定位解释器失败: {e}");
            std::process::exit(1);
        }
    };
    println!("[ OK ] 解释器:    {}", py.display());

    // 判断解释器来源,这是排障时最想先知道的一条
    let kind = if py.starts_with(&dir) {
        let rel = py.strip_prefix(&dir).unwrap_or(&py);
        if rel.starts_with("python") {
            "随包内置运行时"
        } else {
            "开发 venv(仅本机有效,发布版不应命中)"
        }
    } else {
        "系统 PATH 中的 python(发布版不应命中)"
    };
    println!("[INFO] 来源:      {kind}");

    let py_ver = std::process::Command::new(&py)
        .arg("-c")
        .arg("import sys;print(sys.version.split()[0])")
        .output();
    match py_ver {
        Ok(o) if o.status.success() => {
            println!(
                "[ OK ] 版本:      {}",
                String::from_utf8_lossy(&o.stdout).trim()
            );
        }
        _ => println!("[FAIL] 解释器无法执行"),
    }

    // 按依赖分组报告,哪条链路坏了立刻能看出来
    let groups: &[(&str, &[&str])] = &[
        ("加密", &["pikepdf"]),
        ("Office", &["lxml", "docx", "openpyxl"]),
        ("OCR", &["cv2", "numpy", "onnxruntime", "rapidocr"]),
    ];
    println!();
    for (name, mods) in groups {
        let list = mods.join(" ");
        let out = std::process::Command::new(&py)
            .arg("-c")
            .arg(format!(
                "import sys
bad=[]
for m in '{list}'.split():
    try: __import__(m)
    except Exception as e: bad.append(f'{{m}}({{type(e).__name__}})')
print(','.join(bad))"
            ))
            .output();
        match out {
            Ok(o) if o.status.success() => {
                let missing = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if missing.is_empty() {
                    println!("[ OK ] {name:<6} 依赖齐全");
                } else {
                    println!("[FAIL] {name:<6} 缺失: {missing}");
                }
            }
            _ => println!("[FAIL] {name:<6} 自检执行失败"),
        }
    }

    // 真跑入口:模块能 import 不代表 main.py 能装配
    println!();
    let run = std::process::Command::new(&py)
        .arg(dir.join("main.py"))
        .arg("--help")
        .output();
    match run {
        Ok(o) if o.status.success() => println!("[ OK ] pycore 入口可运行"),
        Ok(o) => {
            println!("[FAIL] pycore 入口返回 {}", o.status);
            let err = String::from_utf8_lossy(&o.stderr);
            let lines: Vec<&str> = err.lines().collect();
            for line in lines.iter().skip(lines.len().saturating_sub(5)) {
                println!("       {line}");
            }
        }
        Err(e) => println!("[FAIL] 无法启动 pycore 入口: {e}"),
    }
}
