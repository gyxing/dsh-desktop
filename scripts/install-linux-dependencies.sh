#!/usr/bin/env bash
set -euo pipefail

# 避开 GitHub Ubuntu Runner 默认的 Azure 镜像超时，固定使用 Ubuntu 官方 HTTPS 软件源。
readonly apt_sources_file="$(mktemp)"
trap 'rm -f "$apt_sources_file"' EXIT

cat >"$apt_sources_file" <<'EOF'
deb https://archive.ubuntu.com/ubuntu jammy main restricted universe multiverse
deb https://archive.ubuntu.com/ubuntu jammy-updates main restricted universe multiverse
deb https://archive.ubuntu.com/ubuntu jammy-backports main restricted universe multiverse
deb https://security.ubuntu.com/ubuntu jammy-security main restricted universe multiverse
EOF

# 限制单次网络等待和整体执行时间，避免 Linux 矩阵长期占用发布队列。
apt_options=(
  -o "Dir::Etc::sourcelist=$apt_sources_file"
  -o "Dir::Etc::sourceparts=-"
  -o "Acquire::Retries=3"
  -o "Acquire::http::Timeout=30"
  -o "Acquire::https::Timeout=30"
)
packages=(
  libwebkit2gtk-4.1-dev
  build-essential
  curl
  wget
  file
  libxdo-dev
  libssl-dev
  libayatana-appindicator3-dev
  librsvg2-dev
  patchelf
)

timeout --foreground 5m sudo apt-get "${apt_options[@]}" update
timeout --foreground 15m sudo env DEBIAN_FRONTEND=noninteractive \
  apt-get "${apt_options[@]}" install -y "${packages[@]}"
