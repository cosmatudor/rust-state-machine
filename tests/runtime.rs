use rust_state_machine::{
	maybe_apply_genesis, proof_of_existence, support, types, balances, Runtime, RuntimeCall,
};
use support::keyring::AccountKeyring::{Alice, Bob, Charlie};
use std::sync::OnceLock;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// One fresh DB per test binary (process).
// ---------------------------------------------------------------------------

fn init() {
	static DIR: OnceLock<TempDir> = OnceLock::new();
	DIR.get_or_init(|| {
		let dir = tempfile::tempdir().expect("create temp dir");
		support::init_db_path(dir.path().to_str().expect("utf-8 path"));
		dir
	});
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn signed_transfer(
	from: support::keyring::AccountKeyring,
	nonce: u32,
	to: support::keyring::AccountKeyring,
	amount: types::Balance,
) -> types::Extrinsic {
	let call = RuntimeCall::balances(balances::Call::transfer { to: to.public(), amount });
	support::UncheckedExtrinsic::new_signed(&from.signing_key(), nonce, call)
}

fn signed_claim(
	from: support::keyring::AccountKeyring,
	nonce: u32,
	claim: &str,
) -> types::Extrinsic {
	let call = RuntimeCall::proof_of_existence(proof_of_existence::Call::create_claim {
		claim: claim.to_string(),
	});
	support::UncheckedExtrinsic::new_signed(&from.signing_key(), nonce, call)
}

fn signed_revoke(
	from: support::keyring::AccountKeyring,
	nonce: u32,
	claim: &str,
) -> types::Extrinsic {
	let call = RuntimeCall::proof_of_existence(proof_of_existence::Call::revoke_claim {
		claim: claim.to_string(),
	});
	support::UncheckedExtrinsic::new_signed(&from.signing_key(), nonce, call)
}

/// Build the next valid block for this runtime using its current in-memory block number.
fn next_block(rt: &Runtime, exts: Vec<types::Extrinsic>) -> types::Block {
	types::Block {
		header: support::Header { block_number: rt.system.block_number() + 1 },
		extrinsics: exts,
	}
}

// ---------------------------------------------------------------------------
// Block number / system pallet
// ---------------------------------------------------------------------------

#[test]
fn execute_block_increments_block_number() {
	init();
	let mut rt = Runtime::new();
	let before = rt.system.block_number();
	rt.execute_block(next_block(&rt, vec![])).unwrap();
	assert_eq!(rt.system.block_number(), before + 1);
}

#[test]
fn execute_block_rejects_wrong_header_number() {
	init();
	let mut rt = Runtime::new();
	let bad = types::Block {
		header: support::Header { block_number: rt.system.block_number() + 5 },
		extrinsics: vec![],
	};
	assert!(rt.execute_block(bad).is_err());
}

#[test]
fn multiple_empty_blocks_advance_block_number() {
	init();
	let mut rt = Runtime::new();
	let start = rt.system.block_number();
	for _ in 0..3 {
		rt.execute_block(next_block(&rt, vec![])).unwrap();
	}
	assert_eq!(rt.system.block_number(), start + 3);
}

// ---------------------------------------------------------------------------
// Balance transfers
// ---------------------------------------------------------------------------

#[test]
fn single_transfer_updates_balances() {
	init();
	let mut rt = Runtime::new();
	rt.balances.set_balance(&Alice.public(), 1_000);
	rt.balances.set_balance(&Bob.public(), 0);
	let nonce = rt.system.nonce(&Alice.public());

	rt.execute_block(next_block(&rt, vec![signed_transfer(Alice, nonce, Bob, 300)])).unwrap();

	assert_eq!(rt.balances.balance(&Alice.public()), 700);
	assert_eq!(rt.balances.balance(&Bob.public()), 300);
}

#[test]
fn transfer_exact_balance_drains_sender() {
	init();
	let mut rt = Runtime::new();
	rt.balances.set_balance(&Alice.public(), 500);
	rt.balances.set_balance(&Bob.public(), 0);
	let nonce = rt.system.nonce(&Alice.public());

	rt.execute_block(next_block(&rt, vec![signed_transfer(Alice, nonce, Bob, 500)])).unwrap();

	assert_eq!(rt.balances.balance(&Alice.public()), 0);
	assert_eq!(rt.balances.balance(&Bob.public()), 500);
}

#[test]
fn insufficient_balance_fails_dispatch_block_still_commits() {
	init();
	let mut rt = Runtime::new();
	rt.balances.set_balance(&Alice.public(), 50);
	rt.balances.set_balance(&Bob.public(), 0);
	let nonce = rt.system.nonce(&Alice.public());
	let before = rt.system.block_number();

	// Block itself succeeds even though the dispatch fails inside.
	rt.execute_block(next_block(&rt, vec![signed_transfer(Alice, nonce, Bob, 9_999)])).unwrap();

	assert_eq!(rt.system.block_number(), before + 1);
	assert_eq!(rt.balances.balance(&Alice.public()), 50);
	assert_eq!(rt.balances.balance(&Bob.public()), 0);
}

#[test]
fn two_transfers_in_one_block_from_different_senders() {
	init();
	let mut rt = Runtime::new();
	rt.balances.set_balance(&Alice.public(), 1_000);
	rt.balances.set_balance(&Bob.public(), 1_000);
	rt.balances.set_balance(&Charlie.public(), 0);
	let an = rt.system.nonce(&Alice.public());
	let bn = rt.system.nonce(&Bob.public());

	rt.execute_block(next_block(&rt, vec![
		signed_transfer(Alice, an, Charlie, 100),
		signed_transfer(Bob, bn, Charlie, 200),
	]))
	.unwrap();

	assert_eq!(rt.balances.balance(&Alice.public()), 900);
	assert_eq!(rt.balances.balance(&Bob.public()), 800);
	assert_eq!(rt.balances.balance(&Charlie.public()), 300);
}

// ---------------------------------------------------------------------------
// Nonce tracking
// ---------------------------------------------------------------------------

#[test]
fn nonce_increments_after_successful_dispatch() {
	init();
	let mut rt = Runtime::new();
	rt.balances.set_balance(&Alice.public(), 1_000);
	let before = rt.system.nonce(&Alice.public());

	rt.execute_block(next_block(&rt, vec![signed_transfer(Alice, before, Bob, 10)])).unwrap();

	assert_eq!(rt.system.nonce(&Alice.public()), before + 1);
}

#[test]
fn nonce_mismatch_extrinsic_is_skipped() {
	init();
	let mut rt = Runtime::new();
	rt.balances.set_balance(&Alice.public(), 1_000);
	rt.balances.set_balance(&Bob.public(), 0);

	// Sign with a nonce that is far ahead of the runtime nonce.
	// Signature is valid for that nonce, but execute_block rejects it at the nonce-check step.
	let runtime_nonce = rt.system.nonce(&Alice.public());
	let wrong_nonce_ext = signed_transfer(Alice, runtime_nonce + 100, Bob, 200);
	assert!(wrong_nonce_ext.verify().is_ok(), "signature itself is valid");

	rt.execute_block(next_block(&rt, vec![wrong_nonce_ext])).unwrap();

	// Bob received nothing; extrinsic was skipped.
	assert_eq!(rt.balances.balance(&Bob.public()), 0);
}

#[test]
fn sequential_nonces_across_blocks() {
	init();
	let mut rt = Runtime::new();
	rt.balances.set_balance(&Alice.public(), 1_000);
	rt.balances.set_balance(&Bob.public(), 0);

	let n0 = rt.system.nonce(&Alice.public());
	rt.execute_block(next_block(&rt, vec![signed_transfer(Alice, n0, Bob, 10)])).unwrap();
	assert_eq!(rt.system.nonce(&Alice.public()), n0 + 1);

	let n1 = rt.system.nonce(&Alice.public());
	rt.execute_block(next_block(&rt, vec![signed_transfer(Alice, n1, Bob, 10)])).unwrap();
	assert_eq!(rt.system.nonce(&Alice.public()), n0 + 2);

	assert_eq!(rt.balances.balance(&Bob.public()), 20);
}

// ---------------------------------------------------------------------------
// Proof of existence
// ---------------------------------------------------------------------------

#[test]
fn poe_create_claim_recorded_on_chain() {
	init();
	let mut rt = Runtime::new();
	let nonce = rt.system.nonce(&Alice.public());
	let claim = "rt-poe-create";

	rt.execute_block(next_block(&rt, vec![signed_claim(Alice, nonce, claim)])).unwrap();

	assert_eq!(rt.proof_of_existence.get_claim(&claim.to_string()), Some(&Alice.public()));
}

#[test]
fn poe_duplicate_claim_is_rejected_at_dispatch() {
	init();
	let mut rt = Runtime::new();
	let a_nonce = rt.system.nonce(&Alice.public());
	let b_nonce = rt.system.nonce(&Bob.public());
	let claim = "rt-poe-duplicate";

	rt.execute_block(next_block(&rt, vec![signed_claim(Alice, a_nonce, claim)])).unwrap();
	// Bob attempts the same claim — block succeeds, dispatch fails silently.
	rt.execute_block(next_block(&rt, vec![signed_claim(Bob, b_nonce, claim)])).unwrap();

	assert_eq!(rt.proof_of_existence.get_claim(&claim.to_string()), Some(&Alice.public()));
}

#[test]
fn poe_revoke_allows_reclaim_by_new_owner() {
	init();
	let mut rt = Runtime::new();
	let a0 = rt.system.nonce(&Alice.public());
	let b0 = rt.system.nonce(&Bob.public());
	let claim = "rt-poe-revoke-reclaim";

	rt.execute_block(next_block(&rt, vec![signed_claim(Alice, a0, claim)])).unwrap();
	// a0+1 because Alice's nonce was incremented by the previous block.
	rt.execute_block(next_block(&rt, vec![signed_revoke(Alice, a0 + 1, claim)])).unwrap();
	assert_eq!(rt.proof_of_existence.get_claim(&claim.to_string()), None);

	rt.execute_block(next_block(&rt, vec![signed_claim(Bob, b0, claim)])).unwrap();
	assert_eq!(rt.proof_of_existence.get_claim(&claim.to_string()), Some(&Bob.public()));
}

// ---------------------------------------------------------------------------
// Genesis
// ---------------------------------------------------------------------------

#[test]
fn maybe_apply_genesis_idempotent() {
	init();
	let mut rt = Runtime::new();
	let before = rt.system.block_number();

	maybe_apply_genesis(&mut rt);
	let after_first = rt.system.block_number();

	// Calling again must be a no-op regardless of state.
	maybe_apply_genesis(&mut rt);
	assert_eq!(rt.system.block_number(), after_first);

	// If the chain was at block 0, genesis advanced it to 1 and funded accounts.
	if before == 0 {
		assert_eq!(after_first, 1);
		assert_eq!(rt.balances.balance(&Alice.public()), 1_000_000);
	}
}
