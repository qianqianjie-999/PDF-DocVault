# pycore 运行时打包

## 为什么需要这个

加密 / OCR / 图片转 Word·Excel / Word→PDF 四条链路都经 `pycore` 调 Python。
但 `tauri.conf.json` 的 `bundle.resources` 长期只写了 `resources/pdfium/*`，
**pycore 从来没进过安装包**。也就是说：从源码构建的开发版一切正常，
用户装上发布版后点这些按钮只会看到「未找到 Python 解释器」。

更麻烦的是这类问题**在构建期完全静默** —— 编译通过、测试通过、打包成功、
软件能启动，只有用户点到那个按钮才炸。

所以这里做了两件事：

1. `scripts/pack-pycore-runtime.sh` 产出一个自包含运行时（自带解释器，不依赖用户机器上的 Python）
2. `frontend/src-tauri/build.rs` 在 release 打包时检查它，缺失直接中止打包

## 用起来

```bash
# 完整版:加密 + Office 转换 + OCR(约 411MB)
scripts/pack-pycore-runtime.sh

# 精简版:加密 + Office 转换,不含 OCR(约 94MB)
scripts/pack-pycore-runtime.sh --core-only

# 之后正常打包
pnpm tauri build
```

首次运行会从 GitHub 下载 Python 解释器（约 33MB），之后走缓存
`~/.cache/pdf-docvault/python-runtime/`。缓存丢了重下一次即可。

产物在 `frontend/src-tauri/resources/pycore/`，已加入 `.gitignore` —— 别提交。

## 体积构成

| 组成 | 完整版 | 精简版 | 说明 |
|---|---:|---:|---|
| Python 解释器 + 标准库 | 47MB | 47MB | python-build-standalone，静态链接 |
| pikepdf（加密） | 8MB | 8MB | 内含静态 libqpdf |
| lxml / python-docx / openpyxl | 19MB | 19MB | Office 转换 |
| cv2 + opencv 依赖库 | 153MB | — | OCR 图像预处理 |
| onnxruntime | 58MB | — | OCR 推理 |
| numpy | 70MB | — | OCR |
| rapidocr（含 3 个 ONNX 模型） | 33MB | — | 模型已内置，离线可用 |
| 其他 | 23MB | 20MB | |
| **合计** | **411MB** | **94MB** | 磁盘占用；deb/appimage 压缩后约 35–45% |

OCR 链占了 314MB，是体积的全部来源。如果安装包体积是硬约束，用 `--core-only`，
代价是用户机器上 OCR 功能点不动。

## 裁剪做了什么

`python-build-standalone` 的 stripped 版解压 104MB，裁剪到 47MB：

- 解压时即排除（不是先解压再 `rm`，几千文件的批量删除又慢又容易撞上各种删除保护）
- 去掉 C 头文件 `include/`、`share/`、Tcl/Tk 库（GUI 用不到）
- 去掉 `test/`、`idlelib/`、`lib2to3/`、`ensurepip/`、`turtle`、`tkinter`
- 去掉 32MB 的 `libpython3.12.so` —— 解释器是静态链接的，运行期不需要
- site-packages 里去掉 `pip`/`setuptools`/`wheel`、`*.dist-info`、`__pycache__`

opencv 有个坑：`opencv-python` 和 `opencv-python-headless` 同时装着时，
`cv2/cv2.abi3.so` 的 RPATH 指向 **headless** 那份，`opencv_python.libs`（116MB）
是纯死重量。脚本删的是后者。

## 排障

装好后功能不对，先跑诊断：

```bash
cargo run -p pdf_docvault_core --example pycore_doctor
```

它会打印**实际选中**的目录和解释器路径，并分组报告加密/Office/OCR 各自缺什么。
发布版最怕「解释器找到了，但找的是系统 Python 或绑死路径的开发 venv」，
这个工具把来源直接标出来。

常见输出含义：

| 输出 | 含义 |
|---|---|
| `来源: 随包内置运行时` | 正常，发布版应该长这样 |
| `来源: 开发 venv` | 打包时没带内置运行时，发布版会坏 |
| `来源: 系统 PATH 中的 python` | 同上，且用户机上多半没有 |

## 解释器查找顺序

`core/src/pybridge.rs` 的 `resolve_python`：

1. 环境变量 `PYCORE_PYTHON`（人工指定，排障用）
2. `pycore/python/bin/python3`（随包内置，**打包版走这条**）
3. `pycore/.venv/bin/python`（开发期）
4. PATH 里的 `python3`

内置排在 venv 之前：打包目录里永远带 `python/`，用户机上不会有 venv。
开发机上两者都可能存在，此时以能独立运行的内置运行时为准。

## pycore 目录怎么定位

`lib.rs` 在启动时把 Tauri 解析好的 `$RESOURCE/resources/pycore` 写进
`PYCORE_DIR` 环境变量。不能让 core 自己用 `current_exe()` 猜 ——
Linux 下可执行文件在 `/usr/bin`、资源在 `/usr/lib/<app>/` 下，猜不中。

## 已知限制

- **只支持 Linux/macOS 构建**。Windows 需要 `pack-pycore-runtime.ps1`（尚未编写）；
  `build.rs` 的检查逻辑已经兼容 `python/python.exe` 路径。
- **产物与构建机绑定**：Linux x86_64 上产出的运行时不能拿到 aarch64 或 Windows 用，
  每个平台各自构建。
- **glibc 版本**：manylinux wheel 与解释器都要求构建机与目标机的 glibc 兼容，
  在旧发行版（如 CentOS 7）上构建、给新系统用没问题，反过来不行。
- **macOS 签名**：Apple Silicon 上内置的解释器和 .so 若未签名，用户首次运行时
  可能被 Gatekeeper 拦。打包后需要 `codesign --deep` 处理，这一步尚未接入构建流程。

## 本机踩过的坑

**`/tmp` 只有 10MB 时测试会随机失败。** 测试用 `tempfile::tempdir()`，
默认写到 `/tmp`；渲染几张 PDF 就 `StorageFull`，表现为
`pdf::compress::tests::*` 偶发失败或 SIGTRAP。跑测试前先指到大分区：

```bash
TMPDIR=/path/to/big/disk cargo test -p pdf_docvault_core -p pdf-docvault-tests
```

**沙箱环境下 `rm` 依赖 `$HOME`。** 没有 HOME 时会走回收站失败，
`SAFE_DELETE_FAIL_CLOSED`。脚本因此尽量避免 `rm`，改为改名保留旧产物。

## 和许可证公钥防呆的关系

同一个 `build.rs` 里还有一道公钥检查（见 `docs/activation-setup.md`）。
两者思路一致：**发布包里缺了某个东西，用户现场只会看到一句看不懂的报错，
而开发机上永远复现不了** —— 所以把检查放在打包环节，让它根本发不出去。

逃生开关（仅内部测试包，产物不能对外发布）：

```bash
PDF_DOCVAULT_SKIP_PYCORE_CHECK=1 pnpm tauri build
```
