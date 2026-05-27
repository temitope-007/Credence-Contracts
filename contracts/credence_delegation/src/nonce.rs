//! Per-identity nonce tracking for the delegation contract.
//!
//! Provides monotonically increasing nonces that bind each delegated-action
//! signature to a single use.  The same pattern used by `credence_bond::nonce`
//! is replicated here so the delegation contract remains self-contained.

// ── Storage TTL Policy ────────────────────────────────────────────────────────
//
// Delegation entries: TTL is tied to the delegation's `expires_at` + BUFFER.
// Nonce entries: TTL is the maximum `expires_at` across all active
//                delegations for `owner`, never less than MIN_NONCE_TTL.
//
// Restore invariant: If a Nonce entry is archived (which must not happen in
//                    normal operation), restoring it MUST preserve the stored
//                    value. The system must never re-initialise a missing nonce
//                    to 0 without first verifying no delegation was ever issued
//                    for that owner. Add an explicit panic/error if a nonce
//                    key is missing but delegation keys still exist.
// ─────────────────────────────────────────────────────────────────────────────

/// Safety buffer added on top of the delegation's `expires_at` TTL.
/// ~1 day at 5 s/ledger.
pub const LEDGER_BUMP_BUFFER: u32 = 17_280;

/// Minimum TTL for a Nonce entry regardless of delegation expiry.
/// ~30 days at 5 s/ledger.
pub const MIN_NONCE_TTL: u32 = 518_400;

/// Maximum persistent TTL allowed by the Soroban network.
/// ~6 months at 5 s/ledger.
pub const MAX_TTL: u32 = 3_110_400;

use credence_errors::ContractError;
use soroban_sdk::panic_with_error;
use soroban_sdk::{Address, Env};

use crate::DataKey;

// ── TTL helpers ───────────────────────────────────────────────────────────────

/// Compute the ledger-relative TTL for a given Unix-timestamp expiry.
///
/// Converts `expires_at` (seconds) to a ledger offset using the current
/// ledger timestamp and the network's `seconds_per_ledger`, adds
/// `LEDGER_BUMP_BUFFER`, and caps at `MAX_TTL`.
fn ttl_for_expiry(e: &Env, expires_at: u64) -> u32 {
    let now = e.ledger().timestamp();
    // seconds_per_ledger is not directly exposed; use the standard 5 s/ledger constant.
    const SECONDS_PER_LEDGER: u64 = 5;

    let remaining_secs = expires_at.saturating_sub(now);
    let ledgers_until_expiry = (remaining_secs / SECONDS_PER_LEDGER) as u32;
    let desired = ledgers_until_expiry.saturating_add(LEDGER_BUMP_BUFFER);
    desired.min(MAX_TTL)
}

/// Bump the TTL for a `DataKey::Delegation` entry in persistent storage.
///
/// # Guarantees
/// - Called on every read and write of `DataKey::Delegation(owner, delegate, kind)`.
/// - Prevents archival for the duration of the delegation's validity window.
pub fn bump_delegation_ttl(e: &Env, key: &DataKey, expires_at: u64) {
    if !e.storage().persistent().has(key) {
        return;
    }
    let extend_to = ttl_for_expiry(e, expires_at).max(LEDGER_BUMP_BUFFER);
    let threshold = extend_to / 2;
    e.storage()
        .persistent()
        .extend_ttl(key, threshold, extend_to);
}

/// Bump the TTL for a `DataKey::Nonce` entry in persistent storage.
///
/// `expires_at` should be the maximum `expires_at` across all active
/// delegations for the owner, or `0` to use `MIN_NONCE_TTL`.
///
/// # Guarantees
/// - Nonce NEVER resets to 0 after a restore; archival is prevented while any
///   active delegation exists.
/// - Called on every read and write of `DataKey::Nonce(owner)`.
pub fn bump_nonce_ttl(e: &Env, key: &DataKey, expires_at: u64) {
    if !e.storage().persistent().has(key) {
        return;
    }
    let extend_to = ttl_for_expiry(e, expires_at).max(MIN_NONCE_TTL);
    let threshold = extend_to / 2;
    e.storage()
        .persistent()
        .extend_ttl(key, threshold, extend_to);
}

// ── Nonce operations ──────────────────────────────────────────────────────────

/// Returns the current nonce for `identity` (starts at 0).
///
/// Callers must supply this value in the next state-changing delegated call;
/// it is incremented on success.
#[must_use]
pub fn get_nonce(e: &Env, identity: &Address) -> u64 {
    let key = DataKey::Nonce(identity.clone());
    let nonce: u64 = e.storage().persistent().get(&key).unwrap_or(0);
    // Only bump TTL if the key actually exists in storage.
    bump_nonce_ttl(e, &key, 0);
    nonce
}

/// Asserts `expected_nonce` matches the stored nonce for `identity`, then
/// increments.  Panics on mismatch (replay or out-of-order submission).
pub fn consume_nonce(e: &Env, identity: &Address, expected_nonce: u64) {
    let key = DataKey::Nonce(identity.clone());
    let current: u64 = e.storage().persistent().get(&key).unwrap_or(0);
    if current != expected_nonce {
        panic_with_error!(e, ContractError::InvalidNonce);
    }
    let next = current
        .checked_add(1)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow));
    e.storage().persistent().set(&key, &next);
    bump_nonce_ttl(e, &key, 0);
}

/// Advances nonce to `new_nonce`, invalidating the half-open range
/// `[current_nonce, new_nonce)`.
///
/// This allows compromised-key recovery by skipping potentially leaked,
/// pre-signed delegated payloads without submitting each nonce one-by-one.
///
/// # Panics
/// Panics if `new_nonce <= current_nonce` or if the span exceeds `max_span`.
pub fn invalidate_nonce_range(
    e: &Env,
    identity: &Address,
    new_nonce: u64,
    max_span: u64,
) -> (u64, u64) {
    let key = DataKey::Nonce(identity.clone());
    let current: u64 = e.storage().persistent().get(&key).unwrap_or(0);
    if new_nonce <= current {
        panic_with_error!(e, ContractError::InvalidNonce);
    }
    let span = new_nonce
        .checked_sub(current)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::Underflow));
    if span > max_span {
        panic_with_error!(e, ContractError::InvalidNonce);
    }

    e.storage().persistent().set(&key, &new_nonce);
    bump_nonce_ttl(e, &key, 0);
    (current, new_nonce)
}
