# Wasm Size Budget

Soroban contracts have a hard on‑chain size limit of ~64 KB per contract Wasm. Exceeding this limit prevents upgrades and can cause runtime failures.

## Enforced Limit
- Default ceiling: **64 KB** (65 536 bytes).
- The limit can be overridden by passing a different value to the script, e.g. `./scripts/check_wasm_size.sh 60` for a 60 KB ceiling.

## How It Works
1. The CI workflow runs `scripts/check_wasm_size.sh` after building all contracts in release mode.
2. The script scans `target/wasm32-unknown-unknown/release/*.wasm` and fails the job if any artifact exceeds the configured ceiling.
3. Debug symbols are stripped via the workspace `profile.release.strip = "symbols"` setting, ensuring only the actual code size is measured.

## Local Validation
Developers can run the check locally:
```bash
chmod +x scripts/check_wasm_size.sh
./scripts/check_wasm_size.sh 64
```
The script will output pass/fail messages for each contract.

## Adjusting the Budget
If a particular contract legitimately needs more space, adjust the limit in the CI step or modify the script invocation accordingly.

---
*This document was added as part of the `feature/wasm-size-budget` implementation.*
