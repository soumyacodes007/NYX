extern crate std;

use super::*;
use asset_registry::AssetRegistryArgs;
use collateral_policy::CollateralPolicyArgs;
use compliance_control::ComplianceControlArgs;
use participant_registry::ParticipantRegistryArgs;
use private_match_verifier::{PrivateMatchVerifier, PrivateMatchVerifierArgs};
use proof_gateway::{ProofGateway, ProofGatewayArgs};
use serde_json::{json, Value};
use soroban_sdk::{
    contract, contractimpl,
    testutils::Address as _,
    Address, Bytes, BytesN, Env, IntoVal, Symbol,
};
use std::{format, path::PathBuf, process::Command, string::ToString};
use zkdtcc_types::{ParticipantRole, ProofType};

#[contract]
struct MockVerifier;

#[contractimpl]
impl MockVerifier {
    pub fn verify(_env: Env, _proof_type: u32, statement_hash: BytesN<32>, proof: Bytes) -> bool {
        proof == Bytes::from_array(proof.env(), &statement_hash.to_array())
    }
}

struct PhaseFourContext {
    operator: Address,
    bid_submitter: Address,
    ask_submitter: Address,
    matcher: Address,
    collateral_policy_id: Address,
    proof_gateway_id: Address,
    order_pool_id: Address,
    bid_participant_id_hash: BytesN<32>,
    ask_participant_id_hash: BytesN<32>,
    matcher_participant_id_hash: BytesN<32>,
    collateral_verifier_id: BytesN<32>,
    encumbrance_verifier_id: BytesN<32>,
}

struct RuntimeMatchBundle {
    bid_order_commitment: BytesN<32>,
    ask_order_commitment: BytesN<32>,
    instrument_id_hash: BytesN<32>,
    execution_commitment: BytesN<32>,
    verification_key: Bytes,
    proof_payload: Bytes,
}

fn hash(env: &Env, value: u8) -> BytesN<32> {
    BytesN::from_array(env, &[value; 32])
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn encode_hex(bytes: &[u8]) -> std::string::String {
    let mut out = std::string::String::with_capacity(bytes.len() * 2 + 2);
    out.push_str("0x");
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn decode_hex(hex: &str) -> std::vec::Vec<u8> {
    let raw = hex.strip_prefix("0x").unwrap_or(hex);
    assert_eq!(raw.len() % 2, 0, "hex must have even length");
    let mut out = std::vec::Vec::with_capacity(raw.len() / 2);
    let bytes = raw.as_bytes();
    for index in (0..bytes.len()).step_by(2) {
        let hi = (bytes[index] as char).to_digit(16).unwrap() as u8;
        let lo = (bytes[index + 1] as char).to_digit(16).unwrap() as u8;
        out.push((hi << 4) | lo);
    }
    out
}

fn bytes_from_hex(env: &Env, hex: &str) -> Bytes {
    Bytes::from_slice(env, &decode_hex(hex))
}

fn bytesn32_from_hex(env: &Env, hex: &str) -> BytesN<32> {
    let bytes = decode_hex(hex);
    BytesN::from_array(env, &bytes.try_into().unwrap())
}

fn setup_phase_four(env: &Env) -> PhaseFourContext {
    env.mock_all_auths();

    let admin = Address::generate(env);
    let operator = Address::generate(env);
    let bid_submitter = Address::generate(env);
    let ask_submitter = Address::generate(env);
    let matcher = Address::generate(env);
    let issuer = Address::generate(env);
    let asset = Address::generate(env);

    let asset_registry_id = env.register(asset_registry::AssetRegistry, AssetRegistryArgs::__constructor(&admin));
    let asset_registry = asset_registry::AssetRegistryClient::new(env, &asset_registry_id);
    asset_registry.set_operator(&admin, &operator, &true);
    asset_registry.register_asset(
        &operator,
        &asset,
        &hash(env, 1),
        &issuer,
        &zkdtcc_types::AssetClass::UsdcSac,
        &true,
        &true,
        &true,
        &true,
        &hash(env, 2),
        &hash(env, 3),
    );

    let participant_registry_id = env.register(
        participant_registry::ParticipantRegistry,
        ParticipantRegistryArgs::__constructor(&admin),
    );
    let participant_registry = participant_registry::ParticipantRegistryClient::new(env, &participant_registry_id);
    participant_registry.set_operator(&admin, &operator, &true);
    let bid_participant_id_hash = hash(env, 10);
    let ask_participant_id_hash = hash(env, 11);
    let matcher_participant_id_hash = hash(env, 12);
    participant_registry.register_participant(
        &operator,
        &bid_participant_id_hash,
        &bid_submitter,
        &ParticipantRole::InstitutionTrader,
        &hash(env, 13),
        &hash(env, 14),
        &hash(env, 15),
    );
    participant_registry.register_participant(
        &operator,
        &ask_participant_id_hash,
        &ask_submitter,
        &ParticipantRole::InstitutionTrader,
        &hash(env, 16),
        &hash(env, 17),
        &hash(env, 18),
    );
    participant_registry.register_participant(
        &operator,
        &matcher_participant_id_hash,
        &matcher,
        &ParticipantRole::Matcher,
        &hash(env, 19),
        &hash(env, 20),
        &hash(env, 21),
    );

    let collateral_policy_id = env.register(
        collateral_policy::CollateralPolicy,
        CollateralPolicyArgs::__constructor(&admin, &asset_registry_id, &1_000_000i128, &42u64),
    );
    let collateral_policy = collateral_policy::CollateralPolicyClient::new(env, &collateral_policy_id);
    collateral_policy.set_operator(&admin, &operator, &true);
    collateral_policy.upsert_asset_policy(
        &operator,
        &asset,
        &7u32,
        &8_000u32,
        &125_000i128,
        &42u64,
        &true,
    );

    let collateral_verifier_id = hash(env, 30);
    let encumbrance_verifier_id = hash(env, 31);
    collateral_policy.set_accepted_verifier(
        &operator,
        &ProofType::CollateralSufficiency,
        &collateral_verifier_id,
        &true,
    );
    collateral_policy.set_accepted_verifier(
        &operator,
        &ProofType::UnencumberedLot,
        &encumbrance_verifier_id,
        &true,
    );

    let mock_verifier = env.register(MockVerifier, ());
    let compliance_control_id = env.register(
        compliance_control::ComplianceControl,
        ComplianceControlArgs::__constructor(&admin),
    );
    let compliance_control = compliance_control::ComplianceControlClient::new(env, &compliance_control_id);
    compliance_control.set_operator(&admin, &operator, &true);
    let proof_gateway_id = env.register(
        ProofGateway,
        ProofGatewayArgs::__constructor(&admin, &participant_registry_id, &collateral_policy_id),
    );
    let proof_gateway = proof_gateway::ProofGatewayClient::new(env, &proof_gateway_id);
    proof_gateway.set_operator(&admin, &operator, &true);
    proof_gateway.set_verifier_route(&operator, &collateral_verifier_id, &mock_verifier, &true);
    proof_gateway.set_verifier_route(&operator, &encumbrance_verifier_id, &mock_verifier, &true);

    let order_pool_id = env.register(
        OrderCommitPool,
        OrderCommitPoolArgs::__constructor(
            &admin,
            &participant_registry_id,
            &proof_gateway_id,
            &compliance_control_id,
        ),
    );
    let order_pool = OrderCommitPoolClient::new(env, &order_pool_id);
    order_pool.set_operator(&admin, &operator, &true);
    order_pool.set_matcher(&operator, &matcher, &true);

    PhaseFourContext {
        operator,
        bid_submitter,
        ask_submitter,
        matcher,
        collateral_policy_id,
        proof_gateway_id,
        order_pool_id,
        bid_participant_id_hash,
        ask_participant_id_hash,
        matcher_participant_id_hash,
        collateral_verifier_id,
        encumbrance_verifier_id,
    }
}

fn create_proof_receipt(
    env: &Env,
    collateral_policy_id: &Address,
    proof_gateway_id: &Address,
    submitter: &Address,
    participant_id_hash: &BytesN<32>,
    verifier_id: &BytesN<32>,
    proof_type: &ProofType,
    portfolio_commitment: &BytesN<32>,
    nonce_seed: u8,
    expiry_ledger: u32,
) -> zkdtcc_types::ProofReceipt {
    let proof_gateway = proof_gateway::ProofGatewayClient::new(env, proof_gateway_id);
    let summary = collateral_policy::CollateralPolicyClient::new(env, collateral_policy_id)
        .get_policy_summary();
    let nonce = hash(env, nonce_seed);
    let statement_hash = proof_gateway.build_statement_hash(
        proof_type,
        participant_id_hash,
        submitter,
        &nonce,
        &expiry_ledger,
        &summary.policy_version,
        &summary.current_epoch,
        portfolio_commitment,
        &summary.required_margin,
    );
    let proof = Bytes::from_array(env, &statement_hash.to_array());
    proof_gateway.verify_and_record(
        submitter,
        participant_id_hash,
        proof_type,
        verifier_id,
        portfolio_commitment,
        &nonce,
        &expiry_ledger,
        &summary.policy_version,
        &summary.current_epoch,
        &summary.required_margin,
        &proof,
    )
}

fn generate_private_match_bundle(
    env: &Env,
    statement_hash: &BytesN<32>,
    fixture_options: Value,
) -> RuntimeMatchBundle {
    let suffix = encode_hex(&statement_hash.to_array()[..4]).replace("0x", "");
    let output = Command::new("node")
        .current_dir(repo_root())
        .arg("scripts/generate-private-match-proof.mjs")
        .arg(encode_hex(&statement_hash.to_array()))
        .arg(format!("order-pool-{suffix}"))
        .arg(fixture_options.to_string())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "private match bundle generation failed\nstdout:\n{}\nstderr:\n{}",
        std::string::String::from_utf8_lossy(&output.stdout),
        std::string::String::from_utf8_lossy(&output.stderr),
    );

    let stdout = std::string::String::from_utf8(output.stdout).unwrap();
    let marker = "__PHASE4_BUNDLE__";
    let line = stdout
        .lines()
        .find(|line| line.starts_with(marker))
        .expect("missing phase 4 bundle marker");
    let summary: Value = serde_json::from_str(&line[marker.len()..]).unwrap();
    let bundle = &summary["bundle"];

    RuntimeMatchBundle {
        bid_order_commitment: bytesn32_from_hex(env, bundle["bidOrderCommitmentHex"].as_str().unwrap()),
        ask_order_commitment: bytesn32_from_hex(env, bundle["askOrderCommitmentHex"].as_str().unwrap()),
        instrument_id_hash: bytesn32_from_hex(env, bundle["instrumentIdHashHex"].as_str().unwrap()),
        execution_commitment: bytesn32_from_hex(env, bundle["executionCommitmentHex"].as_str().unwrap()),
        verification_key: bytes_from_hex(env, bundle["verificationKeyHex"].as_str().unwrap()),
        proof_payload: bytes_from_hex(env, bundle["proofPayloadHex"].as_str().unwrap()),
    }
}

#[test]
fn records_private_match_execution_with_real_proof() {
    let env = Env::default();
    let ctx = setup_phase_four(&env);
    let order_pool = OrderCommitPoolClient::new(&env, &ctx.order_pool_id);
    let proof_gateway = proof_gateway::ProofGatewayClient::new(&env, &ctx.proof_gateway_id);
    let collateral_policy = collateral_policy::CollateralPolicyClient::new(&env, &ctx.collateral_policy_id);

    let bid_collateral_receipt = create_proof_receipt(
        &env,
        &ctx.collateral_policy_id,
        &ctx.proof_gateway_id,
        &ctx.bid_submitter,
        &ctx.bid_participant_id_hash,
        &ctx.collateral_verifier_id,
        &ProofType::CollateralSufficiency,
        &hash(&env, 40),
        41,
        500,
    );
    let bid_encumbrance_receipt = create_proof_receipt(
        &env,
        &ctx.collateral_policy_id,
        &ctx.proof_gateway_id,
        &ctx.bid_submitter,
        &ctx.bid_participant_id_hash,
        &ctx.encumbrance_verifier_id,
        &ProofType::UnencumberedLot,
        &hash(&env, 42),
        43,
        500,
    );
    let ask_collateral_receipt = create_proof_receipt(
        &env,
        &ctx.collateral_policy_id,
        &ctx.proof_gateway_id,
        &ctx.ask_submitter,
        &ctx.ask_participant_id_hash,
        &ctx.collateral_verifier_id,
        &ProofType::CollateralSufficiency,
        &hash(&env, 44),
        45,
        500,
    );
    let ask_encumbrance_receipt = create_proof_receipt(
        &env,
        &ctx.collateral_policy_id,
        &ctx.proof_gateway_id,
        &ctx.ask_submitter,
        &ctx.ask_participant_id_hash,
        &ctx.encumbrance_verifier_id,
        &ProofType::UnencumberedLot,
        &hash(&env, 46),
        47,
        500,
    );

    let fixture_options = json!({
        "bidParticipantIdHashHex": encode_hex(&ctx.bid_participant_id_hash.to_array()),
        "askParticipantIdHashHex": encode_hex(&ctx.ask_participant_id_hash.to_array()),
        "bidCollateralProofReceiptIdHex": encode_hex(&bid_collateral_receipt.receipt_id.to_array()),
        "bidEncumbranceProofReceiptIdHex": encode_hex(&bid_encumbrance_receipt.receipt_id.to_array()),
        "askCollateralProofReceiptIdHex": encode_hex(&ask_collateral_receipt.receipt_id.to_array()),
        "askEncumbranceProofReceiptIdHex": encode_hex(&ask_encumbrance_receipt.receipt_id.to_array())
    });
    let provisional_bundle = generate_private_match_bundle(&env, &hash(&env, 90), fixture_options.clone());

    let private_match_verifier_id = hash(&env, 32);
    collateral_policy.set_accepted_verifier(
        &ctx.operator,
        &ProofType::PrivateMatch,
        &private_match_verifier_id,
        &true,
    );

    let summary = collateral_policy.get_policy_summary();
    let match_nonce = hash(&env, 91);
    let statement_hash = proof_gateway.build_statement_hash(
        &ProofType::PrivateMatch,
        &ctx.matcher_participant_id_hash,
        &ctx.matcher,
        &match_nonce,
        &500u32,
        &summary.policy_version,
        &summary.current_epoch,
        &provisional_bundle.execution_commitment,
        &summary.required_margin,
    );
    let bundle = generate_private_match_bundle(&env, &statement_hash, fixture_options);

    let verifier_contract = env.register(
        PrivateMatchVerifier,
        PrivateMatchVerifierArgs::__constructor(&bundle.verification_key),
    );
    proof_gateway.set_verifier_route(
        &ctx.operator,
        &private_match_verifier_id,
        &verifier_contract,
        &true,
    );

    let batch_id = hash(&env, 80);
    let bid_order = order_pool.commit_order(
        &ctx.bid_submitter,
        &ctx.bid_participant_id_hash,
        &bundle.instrument_id_hash,
        &batch_id,
        &OrderSide::Bid,
        &bundle.bid_order_commitment,
        &bid_collateral_receipt.receipt_id,
        &bid_encumbrance_receipt.receipt_id,
        &hash(&env, 81),
        &500u32,
    );
    let ask_order = order_pool.commit_order(
        &ctx.ask_submitter,
        &ctx.ask_participant_id_hash,
        &bundle.instrument_id_hash,
        &batch_id,
        &OrderSide::Ask,
        &bundle.ask_order_commitment,
        &ask_collateral_receipt.receipt_id,
        &ask_encumbrance_receipt.receipt_id,
        &hash(&env, 82),
        &500u32,
    );

    let private_match_receipt = proof_gateway.verify_and_record(
        &ctx.matcher,
        &ctx.matcher_participant_id_hash,
        &ProofType::PrivateMatch,
        &private_match_verifier_id,
        &bundle.execution_commitment,
        &match_nonce,
        &500u32,
        &summary.policy_version,
        &summary.current_epoch,
        &summary.required_margin,
        &bundle.proof_payload,
    );

    let execution = order_pool.match_orders(
        &ctx.matcher,
        &private_match_verifier_id,
        &private_match_receipt.receipt_id,
        &bid_order.order_id,
        &ask_order.order_id,
        &bundle.execution_commitment,
        &hash(&env, 92),
        &hash(&env, 93),
        &hash(&env, 94),
    );

    assert_eq!(execution.batch_id, batch_id);
    assert_eq!(execution.execution_commitment, bundle.execution_commitment);
    assert_eq!(execution.proof_receipt_id, private_match_receipt.receipt_id);
    assert!(order_pool.is_execution_nullifier_used(&hash(&env, 93)));
    assert!(order_pool.is_execution_nullifier_used(&hash(&env, 94)));

    let stored_bid = order_pool.get_order(&bid_order.order_id);
    let stored_ask = order_pool.get_order(&ask_order.order_id);
    assert_eq!(stored_bid.status, OrderStatus::Matched);
    assert_eq!(stored_ask.status, OrderStatus::Matched);
    assert_eq!(stored_bid.matched_execution_id, execution.execution_id);
    assert_eq!(stored_ask.matched_execution_id, execution.execution_id);
}

#[test]
fn cancels_order_and_rejects_reused_cancel_nullifier() {
    let env = Env::default();
    let ctx = setup_phase_four(&env);
    let order_pool = OrderCommitPoolClient::new(&env, &ctx.order_pool_id);

    let collateral_receipt = create_proof_receipt(
        &env,
        &ctx.collateral_policy_id,
        &ctx.proof_gateway_id,
        &ctx.bid_submitter,
        &ctx.bid_participant_id_hash,
        &ctx.collateral_verifier_id,
        &ProofType::CollateralSufficiency,
        &hash(&env, 50),
        51,
        500,
    );
    let encumbrance_receipt = create_proof_receipt(
        &env,
        &ctx.collateral_policy_id,
        &ctx.proof_gateway_id,
        &ctx.bid_submitter,
        &ctx.bid_participant_id_hash,
        &ctx.encumbrance_verifier_id,
        &ProofType::UnencumberedLot,
        &hash(&env, 52),
        53,
        500,
    );

    let order = order_pool.commit_order(
        &ctx.bid_submitter,
        &ctx.bid_participant_id_hash,
        &hash(&env, 54),
        &hash(&env, 55),
        &OrderSide::Bid,
        &hash(&env, 56),
        &collateral_receipt.receipt_id,
        &encumbrance_receipt.receipt_id,
        &hash(&env, 57),
        &500u32,
    );
    let cancelled = order_pool.cancel_order(&ctx.bid_submitter, &order.order_id, &hash(&env, 57));
    assert_eq!(cancelled.status, OrderStatus::Cancelled);
    assert!(order_pool.is_cancel_nullifier_used(&hash(&env, 57)));

    let result = env.try_invoke_contract::<OrderCommitmentRecord, OrderCommitPoolError>(
        &ctx.order_pool_id,
        &Symbol::new(&env, "cancel_order"),
        soroban_sdk::vec![
            &env,
            ctx.bid_submitter.into_val(&env),
            order.order_id.into_val(&env),
            hash(&env, 57).into_val(&env),
        ],
    );
    assert!(matches!(result, Err(Ok(OrderCommitPoolError::OrderNotActive))));
}

#[test]
fn rejects_match_with_batch_mismatch() {
    let env = Env::default();
    let ctx = setup_phase_four(&env);
    let order_pool = OrderCommitPoolClient::new(&env, &ctx.order_pool_id);

    let bid_collateral_receipt = create_proof_receipt(
        &env,
        &ctx.collateral_policy_id,
        &ctx.proof_gateway_id,
        &ctx.bid_submitter,
        &ctx.bid_participant_id_hash,
        &ctx.collateral_verifier_id,
        &ProofType::CollateralSufficiency,
        &hash(&env, 60),
        61,
        500,
    );
    let bid_encumbrance_receipt = create_proof_receipt(
        &env,
        &ctx.collateral_policy_id,
        &ctx.proof_gateway_id,
        &ctx.bid_submitter,
        &ctx.bid_participant_id_hash,
        &ctx.encumbrance_verifier_id,
        &ProofType::UnencumberedLot,
        &hash(&env, 62),
        63,
        500,
    );
    let ask_collateral_receipt = create_proof_receipt(
        &env,
        &ctx.collateral_policy_id,
        &ctx.proof_gateway_id,
        &ctx.ask_submitter,
        &ctx.ask_participant_id_hash,
        &ctx.collateral_verifier_id,
        &ProofType::CollateralSufficiency,
        &hash(&env, 64),
        65,
        500,
    );
    let ask_encumbrance_receipt = create_proof_receipt(
        &env,
        &ctx.collateral_policy_id,
        &ctx.proof_gateway_id,
        &ctx.ask_submitter,
        &ctx.ask_participant_id_hash,
        &ctx.encumbrance_verifier_id,
        &ProofType::UnencumberedLot,
        &hash(&env, 66),
        67,
        500,
    );

    let bid_order = order_pool.commit_order(
        &ctx.bid_submitter,
        &ctx.bid_participant_id_hash,
        &hash(&env, 68),
        &hash(&env, 69),
        &OrderSide::Bid,
        &hash(&env, 70),
        &bid_collateral_receipt.receipt_id,
        &bid_encumbrance_receipt.receipt_id,
        &hash(&env, 71),
        &500u32,
    );
    let ask_order = order_pool.commit_order(
        &ctx.ask_submitter,
        &ctx.ask_participant_id_hash,
        &hash(&env, 68),
        &hash(&env, 72),
        &OrderSide::Ask,
        &hash(&env, 73),
        &ask_collateral_receipt.receipt_id,
        &ask_encumbrance_receipt.receipt_id,
        &hash(&env, 74),
        &500u32,
    );

    let result = env.try_invoke_contract::<PrivateMatchExecution, OrderCommitPoolError>(
        &ctx.order_pool_id,
        &Symbol::new(&env, "match_orders"),
        soroban_sdk::vec![
            &env,
            ctx.matcher.into_val(&env),
            hash(&env, 75).into_val(&env),
            hash(&env, 76).into_val(&env),
            bid_order.order_id.into_val(&env),
            ask_order.order_id.into_val(&env),
            hash(&env, 77).into_val(&env),
            hash(&env, 78).into_val(&env),
            hash(&env, 79).into_val(&env),
            hash(&env, 80).into_val(&env),
        ],
    );
    assert!(matches!(result, Err(Ok(OrderCommitPoolError::BatchMismatch))));
}
