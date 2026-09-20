#!/usr/bin/env bash
# Distrobox 容器内 Tauri 开发环境初始化脚本
# 在容器内执行: bash /home/qianqianjie/PDF-DocVault/scripts/tauri-distrobox-setup.sh
# 支持 Ubuntu/Debian(apt) 与 Fedora/RHEL(dnf)
set -euo pipefail

echo "=== 0. 检测发行版 ==="
if command -v apt &>/dev/null; then
  PKG="apt"
elif command -v dnf &>/dev/null; then
  PKG="dnf"
else
  echo "仅支持 apt / dnf 系发行版" && exit 1
fi
echo "包管理器: $PKG"

echo "=== 1. 安装 Tauri 2.x 系统依赖 ==="
if [ "$PKG" = "apt" ]; then
  sudo apt update
  sudo apt install -y \
    libwebkit2gtk-4.1-dev \
    build-essential \
    curl \
    wget \
    file \
    libxdo-dev \
    libssl-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev \
    libglib2.0-dev \
    libgtk-3-dev \
    libsoup-3.0-dev \
    libjavascriptcoregtk-4.1-dev \
    pkg-config
else
  # Fedora 39+: webkit2gtk4.1-devel 提供 webkit2gtk-4.1.pc
  sudo dnf install -y \
    gcc \
    gcc-c++ \
    make \
    webkit2gtk4.1-devel \
    gtk3-devel \
    libxdo-devel \
    libayatana-appindicator3-devel \
    librsvg2-devel \
    patchelf \
    pkgconf
fi

echo "=== 2. 安装 Rust 工具链(如未安装) ==="
if ! command -v rustc &>/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
  source "$HOME/.cargo/env"
fi

echo "=== 3. 安装 pnpm(如未安装) ==="
if ! command -v pnpm &>/dev/null; then
  npm install -g pnpm
fi

echo "=== 4. 验证关键依赖 ==="
echo "gio-2.0: $(pkg-config --exists gio-2.0 && echo '✓' || echo '✗')"
echo "webkit2gtk-4.1: $(pkg-config --exists webkit2gtk-4.1 && echo '✓' || echo '✗')"
echo "rustc: $(rustc --version)"
echo "pnpm: $(pnpm --version)"

echo ""
echo "=== ✅ 环境就绪! ==="
echo "在容器内运行以下命令启动 Tauri 开发:"
echo "  cd /home/qianqianjie/PDF-DocVault"
echo "  pnpm install"
echo "  pnpm tauri dev"
