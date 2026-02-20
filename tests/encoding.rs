use parity_scale_codec::{Decode, Encode};
use rust_state_machine::{balances, proof_of_existence, support, types, RuntimeCall};
use support::keyring::AccountKeyring::{Alice, Bob};

// ---------------------------------------------------------------------------
// Helpers — construct typed extrinsics without a live runtime
// ---------------------------------------------------------------------------

fn transfer_ext(nonce: u32) -> types::Extrinsic {
	let call =
		RuntimeCall::balances(balances::Call::transfer { to: Bob.public(), amount: 100 });
	support::UncheckedExtrinsic::new_signed(&Alice.signing_key(), nonce, call)
}

fn claim_ext(nonce: u32) -> types::Extrinsic {
	let call = RuntimeCall::proof_of_existence(proof_of_existence::Call::create_claim {
		claim: "test-document".to_string(),
	});
	support::UncheckedExtrinsic::new_signed(&Alice.signing_key(), nonce, call)
}

// ---------------------------------------------------------------------------
// SCALE block encoding / decoding
// ---------------------------------------------------------------------------

#[test]
fn block_with_extrinsics_roundtrip() {
	let block = types::Block {
		header: support::Header { block_number: 42 },
		extrinsics: vec![transfer_ext(0), claim_ext(1)],
	};

	let decoded = types::Block::decode(&mut &block.encode()[..]).expect("decode succeeds");

	assert_eq!(decoded.header.block_number, 42);
	assert_eq!(decoded.extrinsics.len(), 2);
	assert_eq!(decoded.extrinsics[0].signer, Alice.public());
	assert_eq!(decoded.extrinsics[0].nonce, 0);
	assert_eq!(decoded.extrinsics[1].nonce, 1);
}

#[test]
fn empty_block_roundtrip() {
	let block =
		types::Block { header: support::Header { block_number: 1 }, extrinsics: vec![] };
	let decoded = types::Block::decode(&mut &block.encode()[..]).unwrap();
	assert_eq!(decoded.header.block_number, 1);
	assert!(decoded.extrinsics.is_empty());
}

#[test]
fn extrinsic_fields_survive_encode_decode() {
	let ext = transfer_ext(7);
	let decoded = types::Extrinsic::decode(&mut &ext.encode()[..]).unwrap();

	assert_eq!(decoded.signer, Alice.public());
	assert_eq!(decoded.nonce, 7);
	assert_eq!(decoded.signature, ext.signature);
}

#[test]
fn encoded_bytes_are_deterministic() {
	assert_eq!(transfer_ext(3).encode(), transfer_ext(3).encode());
}

#[test]
fn different_calls_produce_different_encodings() {
	assert_ne!(transfer_ext(0).encode(), claim_ext(0).encode());
}

// ---------------------------------------------------------------------------
// Ed25519 signature correctness
// ---------------------------------------------------------------------------

#[test]
fn fresh_extrinsic_has_valid_signature() {
	assert!(transfer_ext(0).verify().is_ok());
	assert!(claim_ext(0).verify().is_ok());
}

#[test]
fn signature_survives_encode_decode_roundtrip() {
	let decoded = types::Extrinsic::decode(&mut &transfer_ext(3).encode()[..]).unwrap();
	assert!(decoded.verify().is_ok());
}

#[test]
fn tampered_nonce_invalidates_signature() {
	let mut ext = transfer_ext(0);
	ext.nonce = 99;
	assert!(ext.verify().is_err());
}

#[test]
fn swapped_signer_field_invalidates_signature() {
	let mut ext = transfer_ext(0);
	ext.signer = Bob.public();
	assert!(ext.verify().is_err());
}

#[test]
fn different_nonces_produce_different_signatures() {
	assert_ne!(transfer_ext(0).signature, transfer_ext(1).signature);
}

#[test]
fn different_signers_produce_different_signatures() {
	let ext_alice = support::UncheckedExtrinsic::new_signed(
		&Alice.signing_key(),
		0,
		RuntimeCall::balances(balances::Call::transfer { to: Bob.public(), amount: 50 }),
	);
	let ext_bob = support::UncheckedExtrinsic::new_signed(
		&Bob.signing_key(),
		0,
		RuntimeCall::balances(balances::Call::transfer { to: Alice.public(), amount: 50 }),
	);
	assert_ne!(ext_alice.signature, ext_bob.signature);
}

// ---------------------------------------------------------------------------
// verify_batch — parallel signature verification
// ---------------------------------------------------------------------------

#[test]
fn verify_batch_accepts_all_valid_extrinsics() {
	let exts: Vec<_> = (0..5).map(transfer_ext).collect();
	let results = support::verify_batch(&exts);
	assert!(results.iter().all(|r| r.is_ok()));
}

#[test]
fn verify_batch_identifies_single_tampered_entry() {
	let mut exts: Vec<_> = (0..4).map(transfer_ext).collect();
	exts[2].nonce = 99; // tamper index 2 only

	let results = support::verify_batch(&exts);
	assert!(results[0].is_ok());
	assert!(results[1].is_ok());
	assert!(results[2].is_err());
	assert!(results[3].is_ok());
}

#[test]
fn verify_batch_on_empty_slice_returns_empty_vec() {
	let exts: Vec<types::Extrinsic> = vec![];
	assert!(support::verify_batch(&exts).is_empty());
}
