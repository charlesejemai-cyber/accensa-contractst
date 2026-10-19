#![cfg(test)]

//! Before-initialisation coverage for `ReceiptShard::insert_migrated_batch`
//! (issue #676).
//!
//! The destination shard resolved its assigned range from the
//! construction-time `DataKey::StartBatchId` / `DataKey::EndBatchId` keys with
//! `.unwrap()`, so a caller reaching the migration arm with either key absent
//! — before construction, or after the instance entry had aged out — aborted
//! the host call with a `WasmRuntimeError` instead of reporting the missing
//! state the way the rest of the function does. The reads now answer the
//! crate's typed `Error::NotInitialized`.

use accensa_common::Error;
use receipt_shard::{BatchRecord, DataKey, ReceiptShard, ReceiptShardClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env};

/// Registers a shard, runs `remove` against its instance storage, and hands
/// back the client. Removing a construction-time key models exactly the state
/// a caller observes before that key's write has landed (or after the instance
/// entry itself has aged out): the read sees `None`.
fn shard_with_key_removed(
    remove: impl FnOnce(&Env, &Address),
) -> (Env, ReceiptShardClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let router = Address::generate(&env);
    let id = env.register(ReceiptShard, (router, 1u64, 1001u64));
    env.as_contract(&id, || remove(&env, &id));
    let client = ReceiptShardClient::new(&env, &id);
    (env, client)
}

fn record(env: &Env) -> BatchRecord {
    BatchRecord {
        root: BytesN::from_array(env, &[1u8; 32]),
        count: 1,
        period_start: 0,
        period_end: 1,
        anchored_ledger: env.ledger().sequence(),
    }
}

/// A migration with no `DataKey::StartBatchId` in instance storage — a caller
/// reaching the contract before it is initialised — yields the crate's typed
/// error, not a host abort.
#[test]
fn insert_migrated_batch_missing_start_batch_id_returns_typed_error() {
    let (env, client) = shard_with_key_removed(|env, id| {
        env.as_contract(id, || {
            env.storage().instance().remove(&DataKey::StartBatchId);
        });
    });
    let source = Address::generate(&env);
    assert_eq!(
        client.try_insert_migrated_batch(&source, &1u64, &record(&env)),
        Err(Ok(Error::NotInitialized)),
        "a missing StartBatchId must surface as NotInitialized, not a host abort"
    );
}

/// The second read site: with the range's start present but its end gone, the
/// former `EndBatchId` unwrap answers the same typed error.
#[test]
fn insert_migrated_batch_missing_end_batch_id_returns_typed_error() {
    let (env, client) = shard_with_key_removed(|env, id| {
        env.as_contract(id, || {
            env.storage().instance().remove(&DataKey::EndBatchId);
        });
    });
    let source = Address::generate(&env);
    assert_eq!(
        client.try_insert_migrated_batch(&source, &1u64, &record(&env)),
        Err(Ok(Error::NotInitialized)),
        "a missing EndBatchId must surface as NotInitialized, not a host abort"
    );
}
