#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
export JAVA_HOME="${JAVA_HOME:-/usr/lib/jvm/java-21-openjdk-amd64}"
export CLOJUREGPUI_CLOJURE_DIR="${CLOJUREGPUI_CLOJURE_DIR:-$root/clojure}"
cd "$root/clojure"
clojure -M:test
cd "$root/rust"
exec cargo run --release -- --protocol-test
