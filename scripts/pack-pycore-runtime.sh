#!/usr/bin/env bash
# 把 pycore 打成一个自包含运行时目录,供 Tauri 随包分发。
#
# 解决什么问题:安装包里的加密/OCR/Office 三条链路都经 pycore 调 Python,
# 而原来 tauri.conf.json 只打包了 resources/pdfium,pycore 从来没进过安装包
# ——用户在安装版上点加密/OCR 必然报「未找到 pycore 目录」。
#
# 产物结构(全部相对路径,可整体挪动):
#   <out>/
#     main.py, office/, ocr/, pdfcrypt/, convert/, scan/, runtime/
#     python/bin/python3            自带解释器(python-build-standalone,可重定位)
#     python/lib/python3.12/site-packages/...
#
# 用法:
#   scripts/pack-pycore-runtime.sh                  # 完整包(含 OCR)
#   scripts/pack-pycore-runtime.sh --core-only      # 不含 OCR(体积小约 300MB)
#   scripts/pack-pycore-runtime.sh --force          # 忽略缓存重下解释器
#
# 需要联网(仅首次下载解释器,之后走缓存 ~/.cache/pdf-docvault)。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PYCORE="$ROOT/pycore"
OUT="$ROOT/frontend/src-tauri/resources/pycore"

# 某些精简/CI 环境不设 HOME(实测本机交互式 shell 就没有),set -u 下会直接崩。
# 依次尝试: $HOME → passwd 里的家目录 → 仓库内 .cache。
# 不要退回 /tmp:部分环境对 /tmp 的写入是被拦的,curl 会报
# "Failure writing output to destination" 这种看不出根因的错。
resolve_cache_root() {
  if [[ -n "${XDG_CACHE_HOME:-}" ]]; then printf '%s' "$XDG_CACHE_HOME"; return; fi
  local h="${HOME:-}"
  if [[ -z "$h" ]] && command -v getent >/dev/null 2>&1; then
    h="$(getent passwd "$(id -u)" 2>/dev/null | cut -d: -f6)"
  fi
  if [[ -n "$h" && -d "$h" && -w "$h" ]]; then printf '%s/.cache' "$h"; return; fi
  printf '%s/.cache' "$ROOT"
}
CACHE="$(resolve_cache_root)/pdf-docvault/python-runtime"
PY_VER="${PDF_DOCVAULT_PY_VER:-3.12.16}"
PBS_TAG="${PDF_DOCVAULT_PBS_TAG:-20261026}"

WITH_OCR=1
FORCE=0
KEEP_LIBPYTHON=0
for arg in "$@"; do
  case "$arg" in
    --core-only) WITH_OCR=0 ;;
    --with-ocr) WITH_OCR=1 ;;
    --keep-libpython) KEEP_LIBPYTHON=1 ;;
    --force) FORCE=1 ;;
    -h|--help) sed -n '2,22p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "未知参数: $arg" >&2; exit 2 ;;
  esac
done

log() { printf '\033[36m[pycore-pack]\033[0m %s\n' "$*"; }
die() { printf '\033[31m[pycore-pack] 错误:\033[0m %s\n' "$*" >&2; exit 1; }

# ---------- 平台三元组 ----------
uname_s="$(uname -s)"
uname_m="$(uname -m)"
case "$uname_s/$uname_m" in
  Linux/x86_64)   TRIPLE="x86_64-unknown-linux-gnu";   EXE="python3" ;;
  Linux/aarch64)  TRIPLE="aarch64-unknown-linux-gnu";  EXE="python3" ;;
  Darwin/x86_64)  TRIPLE="x86_64-apple-darwin";        EXE="python3" ;;
  Darwin/arm64)   TRIPLE="aarch64-apple-darwin";       EXE="python3" ;;
  *) die "不支持的平台: $uname_s/$uname_m(Windows 请用 pack-pycore-runtime.ps1)" ;;
esac

TARBALL="cpython-${PY_VER}+${PBS_TAG}-${TRIPLE}-install_only_stripped.tar.gz"
URL="https://github.com/astral-sh/python-build-standalone/releases/download/${PBS_TAG}/${TARBALL//+/%2B}"

mkdir -p "$CACHE"
if [[ $FORCE -eq 1 || ! -f "$CACHE/$TARBALL" ]]; then
  log "下载独立 Python 解释器: $TARBALL"
  log "  来源: $URL"
  # 进度条在 CI 日志里会把有用信息冲掉,失败重试 3 次即可
  if ! curl -fsSL --retry 3 --retry-delay 2 --no-progress-meter \
      -o "$CACHE/$TARBALL.part" "$URL"; then
    rm -f "$CACHE/$TARBALL.part"
    die "下载失败。可手动下载后放到: $CACHE/$TARBALL"
  fi
  mv "$CACHE/$TARBALL.part" "$CACHE/$TARBALL"
else
  log "使用缓存解释器: $CACHE/$TARBALL"
fi

# ---------- 准备干净产物目录 ----------
# 不要用 rm -rf 删旧产物:动辄数万文件,部分环境会拦批量删除或很慢。
# 改为改名留作回滚;但只保留最近一份 —— 完整产物 400MB+,堆几份就是几个 G。
#
# 用管道而不是 `< <(...)` 进程替换:精简环境里 /dev/fd 可能不可用,
# 进程替换会报 "/dev/fd/63: No such file or directory" 并静默跳过整段逻辑。
keep_latest_old() {
  local dir="${OUT%/*}"
  find "$dir" -maxdepth 1 -type d -name 'pycore.old-*' -printf '%T@ %p\n' 2>/dev/null \
    | sort -n \
    | awk 'NR>1{ $1=""; sub(/^ /,""); print }' \
    | while IFS= read -r old; do
        [[ -n "$old" ]] || continue
        if rm -rf "$old" 2>/dev/null; then
          log "清理更早的旧产物: $(basename "$old")"
        else
          log "清理失败,请手动删除: $old"
        fi
      done
}

if [[ -d "$OUT" ]]; then
  keep_latest_old
  trash="$OUT.old-$(date +%s)"
  log "旧产物移到: $trash (确认无误后可删)"
  mv "$OUT" "$trash"
fi
mkdir -p "$OUT"

# ---------- 解压并裁剪解释器 ----------
# 裁剪直接在解压时做:先把不需要的解压出来再 rm,在文件数上千的目录上又慢又
# 容易撞上各种批量删除保护(实测本机沙箱就拦 3000+ 文件的 rm -rf)。
log "解压解释器(解压时即排除 GUI/测试/头文件)"
mkdir -p "$OUT/python"
PYMINOR="${PY_VER%.*}"   # 3.12.16 -> 3.12
tar xzf "$CACHE/$TARBALL" -C "$OUT/python" --strip-components=1 \
  --exclude='python/include/*' \
  --exclude='python/share/*' \
  --exclude='python/lib/libtcl*' \
  --exclude='python/lib/libtk*' \
  --exclude='python/lib/libtcl*' \
  --exclude="python/lib/python${PYMINOR}/tkinter/*" \
  --exclude="python/lib/python${PYMINOR}/turtledemo/*" \
  --exclude="python/lib/python${PYMINOR}/turtle.py" \
  --exclude="python/lib/python${PYMINOR}/idlelib/*" \
  --exclude="python/lib/python${PYMINOR}/lib2to3/*" \
  --exclude="python/lib/python${PYMINOR}/pydoc_data/*" \
  --exclude="python/lib/python${PYMINOR}/ensurepip/*" \
  --exclude="python/lib/python${PYMINOR}/test/*" \
  --exclude="python/lib/python${PYMINOR}/site-packages/*" \
  --exclude='python/lib/libpython*' \
  --exclude='python/bin/pip*' \
  --exclude='python/bin/idle*' \
  --exclude='python/bin/2to3*' \
  --exclude='python/bin/pydoc*' \
  --exclude='python/bin/*-config'

PYLIB="$OUT/python/lib/python${PYMINOR}"
mkdir -p "$PYLIB/site-packages"

# libpython 默认不解压:python-build-standalone 的解释器是静态链接的,运行期不需要
# 那 32MB 的 .so。若个别扩展(ctypes 加载符号等)确实依赖,用 --keep-libpython 重跑。
if [[ "${KEEP_LIBPYTHON:-0}" == "1" ]]; then
  log "补回 libpython(--keep-libpython)"
  tar xzf "$CACHE/$TARBALL" -C "$OUT/python" --strip-components=1 \
    --wildcards 'python/lib/libpython*' 2>/dev/null || true
fi

# 解释器可用性自检:静态链接版没了 libpython.so 也该能跑核心模块。
# 跑不过基本只有一种原因 —— 需要 libpython,提示用 --keep-libpython 重来。
if ! "$OUT/python/bin/$EXE" -c 'import sys,json,sqlite3,ssl,ctypes,hashlib' 2>/dev/null; then
  die "裁剪后的解释器不可用。若确认需要 libpython,用 --keep-libpython 重跑"
fi

# ---------- 装第三方依赖 ----------
log "复制 site-packages"
SRC_SP="$(ls -d "$PYCORE"/.venv/lib/python*/site-packages 2>/dev/null | head -1)"
[[ -n "$SRC_SP" ]] || die "未找到 pycore/.venv。先在 pycore 下创建 venv 并 pip install -r requirements.txt"
DST_SP="$PYLIB/site-packages"

# 排除项:重复/无用/构建工具
EXCLUDES=(
  --exclude='__pycache__/' --exclude='*.pyc'
  --exclude='*.dist-info/' --exclude='*.egg-info/'
  --exclude='pip/' --exclude='setuptools/' --exclude='wheel/'
)
if [[ $WITH_OCR -eq 0 ]]; then
  # core-only:去掉整条 OCR 链
  EXCLUDES+=(
    --exclude='cv2/' --exclude='numpy/' --exclude='numpy.libs/'
    --exclude='onnxruntime/' --exclude='rapidocr/' --exclude='shapely*'
    --exclude='pyclipper*' --exclude='opencv_python*' --exclude='flatbuffers*'
    --exclude='protobuf*' --exclude='omegaconf*' --exclude='antlr4*'
  )
  log "模式: core-only(不含 OCR)"
else
  # opencv 有两份:cv2 的 RPATH 指向 headless 那份,opencv_python.libs 是死重量
  EXCLUDES+=(--exclude='opencv_python.libs/')
  log "模式: 完整(含 OCR)"
fi

rsync -a "${EXCLUDES[@]}" "$SRC_SP/" "$DST_SP/"

# ---------- 复制 pycore 源码 ----------
log "复制 pycore 源码"
rsync -a \
  --exclude='.venv/' --exclude='__pycache__/' --exclude='*.pyc' \
  --exclude='test_*.py' --exclude='*/test_*.py' \
  "$PYCORE/" "$OUT/"

# ---------- 自检 ----------
log "自检"
PY="$OUT/python/bin/$EXE"
"$PY" -c 'import sys; assert sys.version_info[:2] >= (3,11), sys.version' \
  || die "自带解释器不可用"

CHECK_MODS="pikepdf lxml docx openpyxl"
[[ $WITH_OCR -eq 1 ]] && CHECK_MODS="$CHECK_MODS cv2 numpy onnxruntime rapidocr"
# shellcheck disable=SC2086
"$PY" -c "
import sys
sys.path.insert(0, '$OUT')
bad = []
for m in '''$CHECK_MODS'''.split():
    try:
        __import__(m)
    except Exception as e:
        bad.append(f'{m}: {type(e).__name__}: {e}')
if bad:
    print('模块不可用:'); [print(' -', b) for b in bad]; sys.exit(1)
print('模块自检通过: $CHECK_MODS')
" || die "依赖自检失败"

# 真跑一次 pycore 入口,确认能装配到 python 源码
"$PY" "$OUT/main.py" --help >/dev/null 2>&1 || die "pycore 入口无法运行"

SIZE="$(du -sh "$OUT" | cut -f1)"
log "完成: $OUT ($SIZE)"
log "接下来: pnpm tauri build(build.rs 会检查该目录,缺失则中止打包)"
