#!/usr/bin/env bash
# check_wasm_size.sh - verify each contract wasm file is under size limit (default 64KB)
set -euo pipefail
MAX_KB=${1:-64}
MAX_BYTES=$((MAX_KB * 1024))
echo "Checking wasm size limit: ${MAX_KB}KB (${MAX_BYTES} bytes)"
# Find all wasm files in target directory for workspace contracts
shopt -s nullglob
easy_wasm_files=(target/wasm32-unknown-unknown/release/*.wasm)
if [ ${#easy_wasm_files[@]} -eq 0 ]; then
  echo "[ERROR] No wasm files found in target/wasm32-unknown-unknown/release/"
  exit 1
fi
for wasm in "${easy_wasm_files[@]}"; do
  size=$(wc -c < "$wasm")
  if (( size > MAX_BYTES )); then
    echo "[FAIL] $wasm size $(($size/1024))KB exceeds limit of ${MAX_KB}KB"
    exit 1
  else
    echo "[PASS] $wasm size $(($size/1024))KB within limit"
  fi
done
exit 0
