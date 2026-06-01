use credence_errors::ContractError;
use soroban_sdk::{contracttype, panic_with_error, Address, Env, Symbol, Vec};

use crate::DataKey;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum PauseAction {
    Pause = 1,
    Unpause = 2,
}

/// Read-only aggregated snapshot of a single pause proposal, for operator
/// monitoring dashboards.
///
/// This struct is the typed result of [`get_pause_proposal_state`], which
/// **aggregates four distinct storage entries** into one read:
/// * [`DataKey::PauseProposalCounter`] — to tell an allocated id from one that
///   was never issued (and so derive `executed`).
/// * [`DataKey::PauseProposal`] — the proposed action payload.
/// * [`DataKey::PauseApproval`] — the per-signer approval flags.
/// * [`DataKey::PauseApprovalCount`] — the running approval tally.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PauseProposalView {
    /// The proposal id this view describes (echoes the query argument).
    pub proposal_id: u64,
    /// Proposed action: `1` = Pause, `2` = Unpause, `0` = no live payload
    /// (the proposal was never allocated, or has already been executed/cleared).
    pub action: u32,
    /// Number of distinct signer approvals recorded for the proposal.
    pub approvals: u32,
    /// The subset of the caller-supplied `signers` that have approved. See
    /// [`get_pause_proposal_state`] for why the candidate set must be supplied.
    pub approved_by: Vec<Address>,
    /// `true` when the id was allocated by the counter but its payload is gone —
    /// i.e. the proposal has been executed (or otherwise cleared).
    pub executed: bool,
}

fn require_admin_auth(e: &Env, admin: &Address) {
    let stored_admin: Address = e
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
    if stored_admin != *admin {
        panic_with_error!(e, ContractError::NotAdmin);
    }
    admin.require_auth();
}

pub fn is_paused(e: &Env) -> bool {
    e.storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false)
}

pub fn require_not_paused(e: &Env) {
    if is_paused(e) {
        panic_with_error!(e, ContractError::ContractPaused);
    }
}

/// Add or remove a pause signer.
///
/// Invariant: the stored `PauseSignerCount` MUST always equal the number
/// of `PauseSigner(Address)` entries set to `true` in contract storage.
///
/// Implementations must ensure `PauseSignerCount` is only incremented when
/// a previously-false entry is set to `true`, and only decremented when a
/// previously-true entry is removed. Tests should assert this invariant after
/// every `set_pause_signer` call.
pub fn set_pause_signer(e: &Env, admin: &Address, signer: &Address, enabled: bool) {
    require_admin_auth(e, admin);

    // No-lockout invariants:
    // 1. If there are active signers (count > 0), threshold MUST be > 0.
    // 2. Threshold MUST be <= signer count.
    // 3. Unpause MUST ALWAYS be reachable (admin override is available).

    let key = DataKey::PauseSigner(signer.clone());
    let existing: bool = e.storage().instance().get(&key).unwrap_or(false);

    if enabled {
        if !existing {
            e.storage().instance().set(&key, &true);
            let count: u32 = e
                .storage()
                .instance()
                .get(&DataKey::PauseSignerCount)
                .unwrap_or(0);
            e.storage()
                .instance()
                .set(&DataKey::PauseSignerCount, &count.saturating_add(1));

            // Auto-adjust threshold to 1 if it is currently 0, to maintain no-lockout invariant
            let threshold: u32 = e
                .storage()
                .instance()
                .get(&DataKey::PauseThreshold)
                .unwrap_or(0);
            if threshold == 0 {
                e.storage().instance().set(&DataKey::PauseThreshold, &1_u32);
            }
        }
    } else if existing {
        e.storage().instance().remove(&key);
        let count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::PauseSignerCount)
            .unwrap_or(0);
        e.storage()
            .instance()
            .set(&DataKey::PauseSignerCount, &count.saturating_sub(1));

        let threshold: u32 = e
            .storage()
            .instance()
            .get(&DataKey::PauseThreshold)
            .unwrap_or(0);
        let new_count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::PauseSignerCount)
            .unwrap_or(0);
        if threshold > new_count {
            e.storage()
                .instance()
                .set(&DataKey::PauseThreshold, &new_count);
        }
    }

    e.events().publish(
        (Symbol::new(e, "pause_signer_set"), signer.clone()),
        enabled,
    );
}

pub fn set_pause_threshold(e: &Env, admin: &Address, threshold: u32) {
    require_admin_auth(e, admin);
    let count: u32 = e
        .storage()
        .instance()
        .get(&DataKey::PauseSignerCount)
        .unwrap_or(0);
    if threshold > count {
        panic_with_error!(e, ContractError::ThresholdExceedsSigners);
    }
    if threshold == 0 && count > 0 {
        panic_with_error!(e, ContractError::InvalidPauseAction);
    }
    e.storage()
        .instance()
        .set(&DataKey::PauseThreshold, &threshold);
    e.events()
        .publish((Symbol::new(e, "pause_threshold_set"),), threshold);
}

fn require_pause_signer(e: &Env, signer: &Address) {
    signer.require_auth();
    let ok: bool = e
        .storage()
        .instance()
        .get(&DataKey::PauseSigner(signer.clone()))
        .unwrap_or(false);
    if !ok {
        panic_with_error!(e, ContractError::NotSigner);
    }
}

/// Allocate a unique pause proposal id and advance the counter.
///
/// `PauseProposalCounter` stores the next unused proposal id.
/// The allocated id is returned, and the counter is incremented to prevent reuse.
fn next_proposal_id(e: &Env) -> u64 {
    let id: u64 = e
        .storage()
        .instance()
        .get(&DataKey::PauseProposalCounter)
        .unwrap_or(0);
    let next = id
        .checked_add(1)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow));
    e.storage()
        .instance()
        .set(&DataKey::PauseProposalCounter, &next);
    id
}

fn record_approval(e: &Env, proposal_id: u64, signer: &Address) {
    let approval_key = DataKey::PauseApproval(proposal_id, signer.clone());
    if e.storage().instance().has(&approval_key) {
        return;
    }
    e.storage().instance().set(&approval_key, &true);
    let count: u32 = e
        .storage()
        .instance()
        .get(&DataKey::PauseApprovalCount(proposal_id))
        .unwrap_or(0);
    let new_count = count
        .checked_add(1)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow));
    e.storage()
        .instance()
        .set(&DataKey::PauseApprovalCount(proposal_id), &new_count);
}

pub fn pause(e: &Env, caller: &Address) -> Option<u64> {
    let threshold: u32 = e
        .storage()
        .instance()
        .get(&DataKey::PauseThreshold)
        .unwrap_or(0);
    if threshold == 0 {
        require_admin_auth(e, caller);
        do_pause(e, None);
        None
    } else {
        propose_action(e, caller, PauseAction::Pause)
    }
}

pub fn unpause(e: &Env, caller: &Address) -> Option<u64> {
    let threshold: u32 = e
        .storage()
        .instance()
        .get(&DataKey::PauseThreshold)
        .unwrap_or(0);
    if threshold == 0 {
        require_admin_auth(e, caller);
        do_unpause(e, None);
        None
    } else {
        // Admin override: Admin can always unpause without a proposal.
        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

        if *caller == stored_admin {
            caller.require_auth();
            do_unpause(e, None);
            return None;
        }

        propose_action(e, caller, PauseAction::Unpause)
    }
}

fn propose_action(e: &Env, caller: &Address, action: PauseAction) -> Option<u64> {
    require_pause_signer(e, caller);

    let id = next_proposal_id(e);
    e.storage()
        .instance()
        .set(&DataKey::PauseProposal(id), &(action as u32));
    e.storage()
        .instance()
        .set(&DataKey::PauseApprovalCount(id), &0_u32);

    record_approval(e, id, caller);

    e.events()
        .publish((Symbol::new(e, "pause_proposed"), id), action as u32);

    Some(id)
}

pub fn approve_pause_proposal(e: &Env, signer: &Address, proposal_id: u64) {
    require_pause_signer(e, signer);

    let _action: u32 = e
        .storage()
        .instance()
        .get(&DataKey::PauseProposal(proposal_id))
        .unwrap_or_else(|| panic_with_error!(e, ContractError::ProposalNotFound));

    record_approval(e, proposal_id, signer);

    e.events().publish(
        (Symbol::new(e, "pause_approved"), proposal_id),
        signer.clone(),
    );
}

pub fn execute_pause_proposal(e: &Env, proposal_id: u64) {
    let action: u32 = e
        .storage()
        .instance()
        .get(&DataKey::PauseProposal(proposal_id))
        .unwrap_or_else(|| panic_with_error!(e, ContractError::ProposalNotFound));

    let threshold: u32 = e
        .storage()
        .instance()
        .get(&DataKey::PauseThreshold)
        .unwrap_or(0);
    let approvals: u32 = e
        .storage()
        .instance()
        .get(&DataKey::PauseApprovalCount(proposal_id))
        .unwrap_or(0);

    if approvals < threshold {
        panic_with_error!(e, ContractError::InsufficientApprovals);
    }

    match action {
        1 => do_pause(e, Some(proposal_id)),
        2 => do_unpause(e, Some(proposal_id)),
        _ => panic_with_error!(e, ContractError::InvalidPauseAction),
    }

    e.storage()
        .instance()
        .remove(&DataKey::PauseProposal(proposal_id));
    e.storage()
        .instance()
        .remove(&DataKey::PauseApprovalCount(proposal_id));
}

fn do_pause(e: &Env, proposal_id: Option<u64>) {
    e.storage().instance().set(&DataKey::Paused, &true);
    e.events().publish((Symbol::new(e, "paused"),), proposal_id);
}

fn do_unpause(e: &Env, proposal_id: Option<u64>) {
    e.storage().instance().set(&DataKey::Paused, &false);
    e.events()
        .publish((Symbol::new(e, "unpaused"),), proposal_id);
}

/// Aggregate the full state of a pause proposal into a single typed view.
///
/// This is **read-only**: it performs no `require_auth` and never mutates
/// storage, so it is safe to expose as a public entrypoint. It combines the
/// four proposal-related storage entries (see [`PauseProposalView`]).
///
/// `signers` is the candidate set used to populate `approved_by`. Soroban
/// instance storage is a key/value map with no key enumeration, and the
/// contract keeps no list of approvers — only per-`(proposal, signer)` flags.
/// The view therefore cannot discover approvers on its own; the caller passes
/// the addresses it wants resolved (operators already track their signer set).
/// Passing an empty vector yields an empty `approved_by` while still returning
/// the action/approvals/executed fields, which do not depend on `signers`.
///
/// Field derivation:
/// * `action` is `0` when no live payload exists for `proposal_id`.
/// * `executed` is `true` when the id is below the counter (it was allocated)
///   yet has no live payload (it was executed/cleared). A never-allocated id
///   (`proposal_id >= PauseProposalCounter`) reports `action = 0, executed =
///   false`.
pub fn get_pause_proposal_state(
    e: &Env,
    proposal_id: u64,
    signers: &Vec<Address>,
) -> PauseProposalView {
    let store = e.storage().instance();

    // Read 1: the counter, to distinguish allocated-then-cleared ids from
    // ids that were never issued.
    let counter: u64 = store.get(&DataKey::PauseProposalCounter).unwrap_or(0);

    // Read 2: the action payload. Absent (0) once executed or if never created.
    let action: u32 = store.get(&DataKey::PauseProposal(proposal_id)).unwrap_or(0);
    let has_payload = action != 0;

    // Read 3: the approval count.
    let approvals: u32 = store
        .get(&DataKey::PauseApprovalCount(proposal_id))
        .unwrap_or(0);

    // Read 4: per-signer approval flags, resolved across the supplied set.
    let mut approved_by = Vec::new(e);
    for signer in signers.iter() {
        let approved: bool = store
            .get(&DataKey::PauseApproval(proposal_id, signer.clone()))
            .unwrap_or(false);
        if approved {
            approved_by.push_back(signer);
        }
    }

    let executed = proposal_id < counter && !has_payload;

    PauseProposalView {
        proposal_id,
        action,
        approvals,
        approved_by,
        executed,
    }
}
