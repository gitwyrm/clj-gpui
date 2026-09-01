#!/usr/bin/env bash
# Idempotent Cloud Agent setup for clj-gpui.
#
# Prepares a machine that already has Java 21 and a Rust toolchain (the Cursor
# default base image) to build and run native GPUI windows headlessly:
#   * system libraries the GPUI (Rust) host links against
#   * Mesa lavapipe for software Vulkan (no discrete GPU needed)
#   * the Clojure CLI
#   * a pre-built release host binary and warmed dependency caches
#
# Safe to run repeatedly: every step is a no-op when already satisfied.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# 1. System packages: GPUI host build/runtime deps + software Vulkan + tooling.
export DEBIAN_FRONTEND=noninteractive
sudo apt-get update -y
sudo apt-get install -y --no-install-recommends \
  build-essential pkg-config cmake clang curl rlwrap \
  libasound2-dev libfontconfig-dev libwayland-dev libxkbcommon-dev libxkbcommon-x11-dev \
  libssl-dev libzstd-dev libvulkan-dev libvulkan1 mesa-vulkan-drivers vulkan-tools \
  libgit2-dev libgl1-mesa-dev libegl1-mesa-dev \
  x11-utils zenity

# 2. Clojure CLI (skip if already installed).
if ! command -v clojure >/dev/null 2>&1; then
  tmp="$(mktemp -d)"
  curl -fsSL -o "$tmp/linux-install.sh" \
    https://github.com/clojure/brew-install/releases/latest/download/linux-install.sh
  chmod +x "$tmp/linux-install.sh"
  sudo "$tmp/linux-install.sh"
  rm -rf "$tmp"
fi

# 3. Software Vulkan (Mesa lavapipe) for the documented `clj -M:dev` commands.
#    A discrete GPU is absent, so point the Vulkan loader at lavapipe and make
#    sure a display is selected. Only set values that are not already provided.
sudo tee /etc/profile.d/clj-gpui.sh >/dev/null <<'PROFILE'
# clj-gpui: software Vulkan (Mesa lavapipe) for headless GPUI rendering.
if [ -z "${VK_ICD_FILENAMES:-}" ] && [ -f /usr/share/vulkan/icd.d/lvp_icd.json ]; then
  export VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json
fi
export DISPLAY="${DISPLAY:-:1}"
PROFILE

# 4. Build the native GPUI host once (cargo is incremental on re-runs).
( cd host && cargo build --release )

# 5. Warm Clojure dependency caches so tests and examples start quickly.
clojure -P -M:test
clojure -P -M:cljfmt
( cd examples/counter && clojure -P -M:dev )

echo "clj-gpui environment ready."
