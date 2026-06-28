extern crate std;

use super::*;
use asset_registry::AssetRegistryArgs;
use batch_netting_verifier::{BatchNettingVerifier, BatchNettingVerifierArgs};
use collateral_policy::CollateralPolicyArgs;
use participant_registry::ParticipantRegistryArgs;
use proof_gateway::{ProofGateway, ProofGatewayArgs};
use serde_json::{json, Value};
use soroban_sdk::{
    contract, contractimpl,
    testutils::Address as _,
    Address, Bytes, BytesN, Env, IntoVal, Symbol,
};
use std::{format, path::PathBuf, process::Command, string::ToString};
use zkdtcc_types::{OrderSide, ParticipantRole, ProofType};

#[contract]
struct MockVerifier;

#[contractimpl]
impl MockVerifier {
    pub fn verify(_env: Env, _proof_type: u32, statement_hash: BytesN<32>, proof: Bytes) -> bool {
        proof == Bytes::from_array(proof.env(), &statement_hash.to_array())
    }
}

struct PhaseFiveContext {
    operator: Address,
    matcher: Address,
    settler: Address,
    collateral_policy_id: Address,
    proof_gateway_id: Address,
    order_pool_id: Address,
    settlement_engine_id: Address,
    participant_a: Address,
    participant_b: Address,
    participant_c: Address,
    matcher_participant_id_hash: BytesN<32>,
    settler_participant_id_hash: BytesN<32>,
    participant_a_id_hash: BytesN<32>,
    participant_b_id_hash: BytesN<32>,
    participant_c_id_hash: BytesN<32>,
    collateral_verifier_id: BytesN<32>,
    encumbrance_verifier_id: BytesN<32>,
    private_match_verifier_id: BytesN<32>,
}

struct RuntimeBatchBundle {
    instrument_id_hash: BytesN<32>,
    batch_id: BytesN<32>,
    execution_a_commitment: BytesN<32>,
    execution_b_commitment: BytesN<32>,
    settlement_commitment: BytesN<32>,
    net_vector_hash: BytesN<32>,
    trade_nullifier_a: BytesN<32>,
    trade_nullifier_b: BytesN<32>,
    a_bid_order_commitment: BytesN<32>,
    a_ask_order_commitment: BytesN<32>,
    b_bid_order_commitment: BytesN<32>,
    b_ask_order_commitment: BytesN<32>,
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

fn setup_phase_five(env: &Env) -> PhaseFiveContext {
    env.mock_all_auths();

    let admin = Address::generate(env);
    let operator = Address::generate(env);
    let participant_a = Address::generate(env);
    let participant_b = Address::generate(env);
    let participant_c = Address::generate(env);
    let matcher = Address::generate(env);
    let settler = Address::generate(env);
    let issuer = Address::generate(env);
    let asset = Address::generate(env);

    let asset_registry_id =
        env.register(asset_registry::AssetRegistry, AssetRegistryArgs::__constructor(&admin));
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
    let participant_registry =
        participant_registry::ParticipantRegistryClient::new(env, &participant_registry_id);
    participant_registry.set_operator(&admin, &operator, &true);

    let participant_a_id_hash = hash(env, 10);
    let participant_b_id_hash = hash(env, 11);
    let participant_c_id_hash = hash(env, 12);
    let matcher_participant_id_hash = hash(env, 13);
    let settler_participant_id_hash = hash(env, 14);

    participant_registry.register_participant(
        &operator,
        &participant_a_id_hash,
        &participant_a,
        &ParticipantRole::InstitutionTrader,
        &hash(env, 20),
        &hash(env, 21),
        &hash(env, 22),
    );
    participant_registry.register_participant(
        &operator,
        &participant_b_id_hash,
        &participant_b,
        &ParticipantRole::InstitutionTrader,
        &hash(env, 23),
        &hash(env, 24),
        &hash(env, 25),
    );
    participant_registry.register_participant(
        &operator,
        &participant_c_id_hash,
        &participant_c,
        &ParticipantRole::InstitutionTrader,
        &hash(env, 26),
        &hash(env, 27),
        &hash(env, 28),
    );
    participant_registry.register_participant(
        &operator,
        &matcher_participant_id_hash,
        &matcher,
        &ParticipantRole::Matcher,
        &hash(env, 29),
        &hash(env, 30),
        &hash(env, 31),
    );
    participant_registry.register_participant(
        &operator,
        &settler_participant_id_hash,
        &settler,
        &ParticipantRole::SettlementOperator,
        &hash(env, 32),
        &hash(env, 33),
        &hash(env, 34),
    );

    let collateral_policy_id = env.register(
        collateral_policy::CollateralPolicy,
        CollateralPolicyArgs::__constructor(&admin, &asset_registry_id, &1_000_000i128, &77u64),
    );
    let collateral_policy = collateral_policy::CollateralPolicyClient::new(env, &collateral_policy_id);
    collateral_policy.set_operator(&admin, &operator, &true);
    collateral_policy.upsert_asset_policy(
        &operator,
        &asset,
        &7u32,
        &8_500u32,
        &110_000i128,
        &77u64,
        &true,
    );

    let collateral_verifier_id = hash(env, 40);
    let encumbrance_verifier_id = hash(env, 41);
    let private_match_verifier_id = hash(env, 42);
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
    collateral_policy.set_accepted_verifier(
        &operator,
        &ProofType::PrivateMatch,
        &private_match_verifier_id,
        &true,
    );

    let mock_verifier = env.register(MockVerifier, ());
    let proof_gateway_id = env.register(
        ProofGateway,
        ProofGatewayArgs::__constructor(&admin, &participant_registry_id, &collateral_policy_id),
    );
    let proof_gateway = proof_gateway::ProofGatewayClient::new(env, &proof_gateway_id);
    proof_gateway.set_operator(&admin, &operator, &true);
    proof_gateway.set_verifier_route(&operator, &collateral_verifier_id, &mock_verifier, &true);
    proof_gateway.set_verifier_route(&operator, &encumbrance_verifier_id, &mock_verifier, &true);
    proof_gateway.set_verifier_route(&operator, &private_match_verifier_id, &mock_verifier, &true);

    let order_pool_id = env.register(
        order_commit_pool::OrderCommitPool,
        order_commit_pool::OrderCommitPoolArgs::__constructor(
            &admin,
            &participant_registry_id,
            &proof_gateway_id,
        ),
    );
    let order_pool = OrderCommitPoolClient::new(env, &order_pool_id);
    order_pool.set_operator(&admin, &operator, &true);
    order_pool.set_matcher(&operator, &matcher, &true);

    let settlement_engine_id = env.register(
        SettlementNettingEngine,
        SettlementNettingEngineArgs::__constructor(
            &admin,
            &participant_registry_id,
            &proof_gateway_id,
            &order_pool_id,
        ),
    );
    let settlement_engine = SettlementNettingEngineClient::new(env, &settlement_engine_id);
    settlement_engine.set_operator(&admin, &operator, &true);
    settlement_engine.set_settler(&operator, &settler, &true);

    PhaseFiveContext {
        operator,
        matcher,
        settler,
        collateral_policy_id,
        proof_gateway_id,
        order_pool_id,
        settlement_engine_id,
        participant_a,
        participant_b,
        participant_c,
        matcher_participant_id_hash,
        settler_participant_id_hash,
        participant_a_id_hash,
        participant_b_id_hash,
        participant_c_id_hash,
        collateral_verifier_id,
        encumbrance_verifier_id,
        private_match_verifier_id,
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

fn generate_batch_netting_bundle(
    env: &Env,
    statement_hash: &BytesN<32>,
    fixture_options: Value,
) -> RuntimeBatchBundle {
    let suffix = encode_hex(&statement_hash.to_array()[..4]).replace("0x", "");
    let output = Command::new("node")
        .current_dir(repo_root())
        .env("ZKDTCC_CIRCUIT_NAMESPACE", "phase5-settlement-test")
        .arg("scripts/generate-batch-netting-proof.mjs")
        .arg(encode_hex(&statement_hash.to_array()))
        .arg(format!("settlement-{suffix}"))
        .arg(fixture_options.to_string())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "batch netting bundle generation failed\nstdout:\n{}\nstderr:\n{}",
        std::string::String::from_utf8_lossy(&output.stdout),
        std::string::String::from_utf8_lossy(&output.stderr),
    );

    let stdout = std::string::String::from_utf8(output.stdout).unwrap();
    let marker = "__PHASE5_BUNDLE__";
    let line = stdout
        .lines()
        .find(|line| line.starts_with(marker))
        .expect("missing phase 5 bundle marker");
    let summary: Value = serde_json::from_str(&line[marker.len()..]).unwrap();
    let bundle = &summary["bundle"];

    RuntimeBatchBundle {
        instrument_id_hash: bytesn32_from_hex(env, bundle["instrumentIdHashHex"].as_str().unwrap()),
        batch_id: bytesn32_from_hex(env, bundle["batchIdHex"].as_str().unwrap()),
        execution_a_commitment: bytesn32_from_hex(
            env,
            bundle["executionACommitmentHex"].as_str().unwrap(),
        ),
        execution_b_commitment: bytesn32_from_hex(
            env,
            bundle["executionBCommitmentHex"].as_str().unwrap(),
        ),
        settlement_commitment: bytesn32_from_hex(
            env,
            bundle["settlementCommitmentHex"].as_str().unwrap(),
        ),
        net_vector_hash: bytesn32_from_hex(env, bundle["netVectorHashHex"].as_str().unwrap()),
        trade_nullifier_a: bytesn32_from_hex(env, bundle["tradeNullifierAHex"].as_str().unwrap()),
        trade_nullifier_b: bytesn32_from_hex(env, bundle["tradeNullifierBHex"].as_str().unwrap()),
        a_bid_order_commitment: bytesn32_from_hex(
            env,
            bundle["tradeA"]["bidOrderCommitmentHex"].as_str().unwrap(),
        ),
        a_ask_order_commitment: bytesn32_from_hex(
            env,
            bundle["tradeA"]["askOrderCommitmentHex"].as_str().unwrap(),
        ),
        b_bid_order_commitment: bytesn32_from_hex(
            env,
            bundle["tradeB"]["bidOrderCommitmentHex"].as_str().unwrap(),
        ),
        b_ask_order_commitment: bytesn32_from_hex(
            env,
            bundle["tradeB"]["askOrderCommitmentHex"].as_str().unwrap(),
        ),
        verification_key: bytes_from_hex(env, bundle["verificationKeyHex"].as_str().unwrap()),
        proof_payload: bytes_from_hex(env, bundle["proofPayloadHex"].as_str().unwrap()),
    }
}

fn build_real_bundle(ctx: &PhaseFiveContext, env: &Env) -> RuntimeBatchBundle {
    let fixture_options = json!({
        "participantAIdHashHex": encode_hex(&ctx.participant_a_id_hash.to_array()),
        "participantBIdHashHex": encode_hex(&ctx.participant_b_id_hash.to_array()),
        "participantCIdHashHex": encode_hex(&ctx.participant_c_id_hash.to_array()),
    });
    let provisional_bundle = generate_batch_netting_bundle(env, &hash(env, 90), fixture_options.clone());

    let proof_gateway = proof_gateway::ProofGatewayClient::new(env, &ctx.proof_gateway_id);
    let collateral_policy = collateral_policy::CollateralPolicyClient::new(env, &ctx.collateral_policy_id);
    let summary = collateral_policy.get_policy_summary();
    let netting_nonce = hash(env, 91);
    let statement_hash = proof_gateway.build_statement_hash(
        &ProofType::BatchNetting,
        &ctx.settler_participant_id_hash,
        &ctx.settler,
        &netting_nonce,
        &700u32,
        &summary.policy_version,
        &summary.current_epoch,
        &provisional_bundle.settlement_commitment,
        &summary.required_margin,
    );
    generate_batch_netting_bundle(env, &statement_hash, fixture_options)
}

#[test]
fn settles_real_batch_netting_proof() {
    let env = Env::default();
    let ctx = setup_phase_five(&env);
    let batch_netting_verifier_id = hash(&env, 43);
    let collateral_policy = collateral_policy::CollateralPolicyClient::new(&env, &ctx.collateral_policy_id);
    collateral_policy.set_accepted_verifier(
        &ctx.operator,
        &ProofType::BatchNetting,
        &batch_netting_verifier_id,
        &true,
    );
    let bundle = build_real_bundle(&ctx, &env);
    let verifier_contract = env.register(
        BatchNettingVerifier,
        BatchNettingVerifierArgs::__constructor(&bundle.verification_key),
    );
    let proof_gateway = proof_gateway::ProofGatewayClient::new(&env, &ctx.proof_gateway_id);
    proof_gateway.set_verifier_route(
        &ctx.operator,
        &batch_netting_verifier_id,
        &verifier_contract,
        &true,
    );

    let a_collateral_receipt = create_proof_receipt(
        &env,
        &ctx.collateral_policy_id,
        &ctx.proof_gateway_id,
        &ctx.participant_a,
        &ctx.participant_a_id_hash,
        &ctx.collateral_verifier_id,
        &ProofType::CollateralSufficiency,
        &hash(&env, 50),
        51,
        700,
    );
    let a_encumbrance_receipt = create_proof_receipt(
        &env,
        &ctx.collateral_policy_id,
        &ctx.proof_gateway_id,
        &ctx.participant_a,
        &ctx.participant_a_id_hash,
        &ctx.encumbrance_verifier_id,
        &ProofType::UnencumberedLot,
        &hash(&env, 52),
        53,
        700,
    );
    let b_collateral_receipt = create_proof_receipt(
        &env,
        &ctx.collateral_policy_id,
        &ctx.proof_gateway_id,
        &ctx.participant_b,
        &ctx.participant_b_id_hash,
        &ctx.collateral_verifier_id,
        &ProofType::CollateralSufficiency,
        &hash(&env, 54),
        55,
        700,
    );
    let b_encumbrance_receipt = create_proof_receipt(
        &env,
        &ctx.collateral_policy_id,
        &ctx.proof_gateway_id,
        &ctx.participant_b,
        &ctx.participant_b_id_hash,
        &ctx.encumbrance_verifier_id,
        &ProofType::UnencumberedLot,
        &hash(&env, 56),
        57,
        700,
    );
    let c_collateral_receipt = create_proof_receipt(
        &env,
        &ctx.collateral_policy_id,
        &ctx.proof_gateway_id,
        &ctx.participant_c,
        &ctx.participant_c_id_hash,
        &ctx.collateral_verifier_id,
        &ProofType::CollateralSufficiency,
        &hash(&env, 58),
        59,
        700,
    );
    let c_encumbrance_receipt = create_proof_receipt(
        &env,
        &ctx.collateral_policy_id,
        &ctx.proof_gateway_id,
        &ctx.participant_c,
        &ctx.participant_c_id_hash,
        &ctx.encumbrance_verifier_id,
        &ProofType::UnencumberedLot,
        &hash(&env, 60),
        61,
        700,
    );

    let order_pool = OrderCommitPoolClient::new(&env, &ctx.order_pool_id);
    let trade_a_bid_order = order_pool.commit_order(
        &ctx.participant_a,
        &ctx.participant_a_id_hash,
        &bundle.instrument_id_hash,
        &bundle.batch_id,
        &OrderSide::Bid,
        &bundle.a_bid_order_commitment,
        &a_collateral_receipt.receipt_id,
        &a_encumbrance_receipt.receipt_id,
        &hash(&env, 62),
        &700u32,
    );
    let trade_a_ask_order = order_pool.commit_order(
        &ctx.participant_b,
        &ctx.participant_b_id_hash,
        &bundle.instrument_id_hash,
        &bundle.batch_id,
        &OrderSide::Ask,
        &bundle.a_ask_order_commitment,
        &b_collateral_receipt.receipt_id,
        &b_encumbrance_receipt.receipt_id,
        &hash(&env, 63),
        &700u32,
    );
    let trade_b_bid_order = order_pool.commit_order(
        &ctx.participant_c,
        &ctx.participant_c_id_hash,
        &bundle.instrument_id_hash,
        &bundle.batch_id,
        &OrderSide::Bid,
        &bundle.b_bid_order_commitment,
        &c_collateral_receipt.receipt_id,
        &c_encumbrance_receipt.receipt_id,
        &hash(&env, 64),
        &700u32,
    );
    let trade_b_ask_order = order_pool.commit_order(
        &ctx.participant_a,
        &ctx.participant_a_id_hash,
        &bundle.instrument_id_hash,
        &bundle.batch_id,
        &OrderSide::Ask,
        &bundle.b_ask_order_commitment,
        &a_collateral_receipt.receipt_id,
        &a_encumbrance_receipt.receipt_id,
        &hash(&env, 65),
        &700u32,
    );

    let trade_a_match_receipt = create_proof_receipt(
        &env,
        &ctx.collateral_policy_id,
        &ctx.proof_gateway_id,
        &ctx.matcher,
        &ctx.matcher_participant_id_hash,
        &ctx.private_match_verifier_id,
        &ProofType::PrivateMatch,
        &bundle.execution_a_commitment,
        66,
        700,
    );
    let trade_b_match_receipt = create_proof_receipt(
        &env,
        &ctx.collateral_policy_id,
        &ctx.proof_gateway_id,
        &ctx.matcher,
        &ctx.matcher_participant_id_hash,
        &ctx.private_match_verifier_id,
        &ProofType::PrivateMatch,
        &bundle.execution_b_commitment,
        67,
        700,
    );

    let execution_a = order_pool.match_orders(
        &ctx.matcher,
        &ctx.private_match_verifier_id,
        &trade_a_match_receipt.receipt_id,
        &trade_a_bid_order.order_id,
        &trade_a_ask_order.order_id,
        &bundle.execution_a_commitment,
        &hash(&env, 68),
        &hash(&env, 69),
        &hash(&env, 70),
    );
    let execution_b = order_pool.match_orders(
        &ctx.matcher,
        &ctx.private_match_verifier_id,
        &trade_b_match_receipt.receipt_id,
        &trade_b_bid_order.order_id,
        &trade_b_ask_order.order_id,
        &bundle.execution_b_commitment,
        &hash(&env, 71),
        &hash(&env, 72),
        &hash(&env, 73),
    );

    let summary = collateral_policy.get_policy_summary();
    let settlement_nonce = hash(&env, 91);
    let settlement_receipt = proof_gateway.verify_and_record(
        &ctx.settler,
        &ctx.settler_participant_id_hash,
        &ProofType::BatchNetting,
        &batch_netting_verifier_id,
        &bundle.settlement_commitment,
        &settlement_nonce,
        &700u32,
        &summary.policy_version,
        &summary.current_epoch,
        &summary.required_margin,
        &bundle.proof_payload,
    );

    let settlement_engine = SettlementNettingEngineClient::new(&env, &ctx.settlement_engine_id);
    let settled = settlement_engine.settle_batch(
        &ctx.settler,
        &batch_netting_verifier_id,
        &settlement_receipt.receipt_id,
        &bundle.settlement_commitment,
        &bundle.net_vector_hash,
        &execution_a.execution_id,
        &execution_b.execution_id,
        &bundle.trade_nullifier_a,
        &bundle.trade_nullifier_b,
    );

    assert_eq!(settled.batch_id, bundle.batch_id);
    assert_eq!(settled.instrument_id_hash, bundle.instrument_id_hash);
    assert_eq!(settled.net_vector_hash, bundle.net_vector_hash);
    assert!(settlement_engine.has_batch(&settled.settlement_id));
    assert!(settlement_engine.is_trade_nullifier_used(&bundle.trade_nullifier_a));
    assert!(settlement_engine.is_trade_nullifier_used(&bundle.trade_nullifier_b));
    assert!(settlement_engine.is_execution_settled(&execution_a.execution_id));
    assert!(settlement_engine.is_execution_settled(&execution_b.execution_id));

    let stored = settlement_engine.get_batch(&settled.settlement_id);
    assert_eq!(stored.execution_a_commitment, bundle.execution_a_commitment);
    assert_eq!(stored.execution_b_commitment, bundle.execution_b_commitment);
}

#[test]
fn rejects_duplicate_trade_nullifier_arguments() {
    let env = Env::default();
    let ctx = setup_phase_five(&env);

    let trade_nullifier = hash(&env, 80);
    let result = env.try_invoke_contract::<SettlementBatchRecord, SettlementNettingEngineError>(
        &ctx.settlement_engine_id,
        &Symbol::new(&env, "settle_batch"),
        soroban_sdk::vec![
            &env,
            ctx.settler.into_val(&env),
            hash(&env, 81).into_val(&env),
            hash(&env, 82).into_val(&env),
            hash(&env, 83).into_val(&env),
            hash(&env, 84).into_val(&env),
            hash(&env, 85).into_val(&env),
            hash(&env, 86).into_val(&env),
            trade_nullifier.into_val(&env),
            trade_nullifier.into_val(&env),
        ],
    );
    assert!(matches!(
        result,
        Err(Ok(SettlementNettingEngineError::DuplicateTradeNullifier))
    ));
}

#[test]
fn rejects_duplicate_execution_ids() {
    let env = Env::default();
    let ctx = setup_phase_five(&env);

    let execution_id = hash(&env, 100);
    let result = env.try_invoke_contract::<SettlementBatchRecord, SettlementNettingEngineError>(
        &ctx.settlement_engine_id,
        &Symbol::new(&env, "settle_batch"),
        soroban_sdk::vec![
            &env,
            ctx.settler.into_val(&env),
            hash(&env, 101).into_val(&env),
            hash(&env, 102).into_val(&env),
            hash(&env, 103).into_val(&env),
            hash(&env, 104).into_val(&env),
            execution_id.into_val(&env),
            execution_id.into_val(&env),
            hash(&env, 105).into_val(&env),
            hash(&env, 106).into_val(&env),
        ],
    );
    assert!(matches!(
        result,
        Err(Ok(SettlementNettingEngineError::DuplicateExecution))
    ));
}
