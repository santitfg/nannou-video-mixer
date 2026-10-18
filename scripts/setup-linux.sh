#!/usr/bin/env bash
# One-time setup of the build dependencies on Debian/Ubuntu.
set -euo pipefail

sudo apt update
sudo apt install -y git make gcc pkg-config nasm libclang-dev \
  libasound2-dev libudev-dev libv4l-dev
echo "==> Ready: cargo run"
