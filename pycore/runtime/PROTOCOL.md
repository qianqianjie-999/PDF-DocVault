# Rust-Python 进程隔离协议 v1

## 设计原则

- **进程级隔离**:Rust 通过 `std::process::Command` spawn 独立 Python 进程,Python 崩溃不影响主程序
- **单任务模式**:每个任务 spawn 一次进程,完成后退出。无长连接、无状态,实现简单可靠
- **通道分离**:`stdout` 只承载协议 JSON(恰好一行);`stderr` 承载人类可读日志

## 调用方式

```text
<python> <pycore>/main.py <cmd> [args...]
```

`<python>` 由 Rust 侧按顺序探测:

1. `pycore/.venv/bin/python`(打包/开发环境的隔离 venv)
2. `PYCORE_PYTHON` 环境变量
3. 系统 `python3`

## 命令

### selftest

```text
main.py selftest [--engine <name>]
```

返回:`{"ok": true, "engine": "rapidocr", "available": ["rapidocr", ...]}`

### extract

```text
main.py extract --input <img1> [--input <img2> ...] [--engine <name>] [--lang <code>]
```

- 输入只接受图片;PDF 由 Rust 侧 pdfium 先渲染为逐页 PNG
- 返回:

```json
{
  "ok": true,
  "engine": "rapidocr",
  "pages": [
    {
      "index": 0,
      "file": "page-1.png",
      "line_count": 12,
      "lines": [{"text": "…", "score": 0.98, "box": [[x, y], …]}],
      "text": "按阅读顺序拼接的纯文本"
    }
  ],
  "elapsed_ms": 1234
}
```

## 失败格式(统一)

```json
{"ok": false, "error": {"code": "OCR_NO_ENGINE", "message": "…"}}
```

| code | 含义 |
|---|---|
| `OCR_NO_ENGINE` | 环境里没有任何 OCR 引擎 |
| `OCR_ENGINE_UNAVAILABLE` | 指定的引擎不存在 |
| `IO_NOT_FOUND` | 输入文件不存在 |
| `PYTHON_CRASH` | 未预期异常(traceback 已打 stderr) |
| `OCR_TIMEOUT` | Rust 侧等待超时(主动 kill) |

进程退出码:`0` 成功,`1` 失败(Rust 可据此快速判断,JSON 仍以 stdout 为准)。
