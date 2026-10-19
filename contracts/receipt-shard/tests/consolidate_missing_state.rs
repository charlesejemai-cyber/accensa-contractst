#![cfg(test)]

//! Missing-state coverage for `ReceiptShard::consolidate` (issue #675).
//!
//! The source batch read used to be
//! `.unwrap_or_else(|| panic!("batch not found"))`, so a request for a batch
//! the shard never held aborted the host call with a `WasmRuntimeError`
//! instead of naming the missing state. The read now answers the crate's
//! typed `Error::BatchNotFound`, and these tests pin that arm for a batch
//! that was never anchored and for one whose key was removed after the fact.
//!
//! The router gate that opens `consolidate` is owned by a separate issue
//! (#677); these tests keep it satisfied so they measure the batch read
//! itself rather than the preceding gate.

use accensa_common::Error;
use receipt_shard::{DataKey, ReceiptShard, ReceiptShardClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env, Vec};

fn registered_shard(env: &Env) -> ReceiptShardClient<'static> {
    env.mock_all_auths();
    let router = Address::generate(env);
    let id = env.register(ReceiptShard, (router, 1u64, 1001u64));
    ReceiptShardClient::new(env, &id)
}

fn registered_destination(env: &Env) -> Address {
    let router = Address::generate(env);
    env.register(ReceiptShard, (router, 1u64, 1001u64))
}

/// A `consolidate` call that reaches the source read with no
/// `DataKey::Batch` in persistent storage — the batch was never anchored —
/// yields the crate's typed error, not a host abort.
#[test]
fn consolidate_never_anchored_batch_returns_typed_error() {
    let env = Env::default();
    let source = registered_shard(&env);
    let destination = registered_destination(&env);

    let batch_ids: Vec<u64> = Vec::from_array(&env, [42u64]);
    assert_eq!(
        source.try_consolidate(&destination, &1u64, &2u64, &batch_ids),
        Err(Ok(Error::BatchNotFound)),
        "an absent batch record must surface as BatchNotFound, not a host abort"
    );
    assert!(source.is_active());
}

/// The same typed arm answers when a batch that had been anchored is no
/// longer present (for example after it was pruned): the caller can tell
/// "no such batch" apart from a trap.
#[test]
fn consolidate_removed_batch_returns_typed_error() {
    let env = Env::default();
    let source = registered_shard(&env);
    let destination = registered_destination(&env);

    let root = BytesN::from_array(&env, &[7u8; 32]);
    source.anchor_batch(&1, &root, &3, &10, &20);
    assert_eq!(source.get_batch(&1).root, root);

    // Model archival/pruning by dropping the record's storage key.
    let source_id = source.address.clone();
    env.as_contract(&source_id, || {
        env.storage().persistent().remove(&DataKey::Batch(1));
    });

    let batch_ids: Vec<u64> = Vec::from_array(&env, [1u64]);
    assert_eq!(
        source.try_consolidate(&destination, &1u64, &2u64, &batch_ids),
        Err(Ok(Error::BatchNotFound)),
        "a removed batch record must surface as BatchNotFound, not a host abort"
    );
}
