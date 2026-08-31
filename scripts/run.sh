#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
export JAVA_HOME="${JAVA_HOME:-/usr/lib/jvm/java-21-openjdk-amd64}"
export CLOJUREGPUI_CLOJURE_DIR="${CLOJUREGPUI_CLOJURE_DIR:-$root/clojure}"
if [[ -z "${VK_ICD_FILENAMES:-}" && -f /usr/share/vulkan/icd.d/lvp_icd.json ]]; then
  export VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json
fi
cd "$root/rust"
exec cargo run --release -- "$@"
