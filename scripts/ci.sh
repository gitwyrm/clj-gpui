#!/usr/bin/env bash
# Headless checks used by GitHub Actions. Assumes cargo, java, and the Clojure CLI.
# Builds a debug host so protocol-test does not pay for `cargo build --release`.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

echo "==> cargo test"
cargo test --locked --manifest-path host/Cargo.toml

echo "==> cargo build (debug host)"
cargo build --locked --manifest-path host/Cargo.toml

bin="${CLJ_GPUI_BIN:-$root/host/target/debug/clj-gpui}"
if [[ ! -x "$bin" ]]; then
  echo "missing executable host binary: $bin" >&2
  exit 1
fi
export CLJ_GPUI_BIN="$bin"

echo "==> clojure -M:test"
clojure -M:test

echo "==> clojure -M:cljfmt check"
clojure -M:cljfmt check

echo "==> clojure -M:protocol-test"
clojure -M:protocol-test
