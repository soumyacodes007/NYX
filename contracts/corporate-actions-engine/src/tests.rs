extern crate std;

use super::*;
use asset_registry::AssetRegistryArgs;
use collateral_policy::CollateralPolicyArgs;
use entitlement_claim_verifier::{EntitlementClaimVerifier, EntitlementClaimVerifierArgs};
use participant_registry::ParticipantRegistryArgs;
use proof_gateway::{ProofGateway, ProofGatewayArgs};
use serde_json::{json, Value};
use soroban_sdk::{
    contract, contractimpl,
    testutils::Address as _,
    Address, Bytes, BytesN, Env, IntoVal, Symbol,
};
use std::{format, path::PathBuf, process::Command, string::ToString};
use zkdtcc_types::{CorporateActionStatus, CorporateActionType, ParticipantRole, ProofType};

#[contract]
struct MockVerifier;

#[contractimpl]
impl MockVerifier {
    pub fn verify(_env: Env, _proof_type: u32, statement_hash: BytesN<32>, proof: Bytes) -> bool {
        proof == Bytes::from_array(proof.env(), &statement_hash.to_array())
    }
}

struct PhaseSixContext {
    operator: Address,
    issuer: Address,
    claimant: Address,
    collateral_policy_id: Address,
    proof_gateway_id: Address,
    engine_id: Address,
    claimant_participant_id_hash: BytesN<32>,
    asset: Address,
    payout_asset: Address,
}

struct RuntimeClaimBundle {
    event_id_hash: BytesN<32>,
    event_root: BytesN<32>,
    claim_commitment: BytesN<32>,
    claim_nullifier: BytesN<32>,
    claim_amount: i128,
    entitlement_quantity: i128,
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

fn setup_phase_six(env: &Env) -> PhaseSixContext {
    env.mock_all_auths();

    let admin = Address::generate(env);
    let operator = Address::generate(env);
    let issuer = Address::generate(env);
    let claimant = Address::generate(env);
    let issuer_account = Address::generate(env);
    let asset = Address::generate(env);
    let payout_asset = Address::generate(env);

    let asset_registry_id =
        env.register(asset_registry::AssetRegistry, AssetRegistryArgs::__constructor(&admin));
    let asset_registry = asset_registry::AssetRegistryClient::new(env, &asset_registry_id);
    asset_registry.set_operator(&admin, &operator, &true);
    asset_registry.register_asset(
        &operator,
        &asset,
        &hash(env, 1),
        &issuer_account,
        &zkdtcc_types::AssetClass::DtcEntitlement,
        &true,
        &true,
        &true,
        &true,
        &hash(env, 2),
        &hash(env, 3),
    );
    asset_registry.register_asset(
        &operator,
        &payout_asset,
        &hash(env, 4),
        &issuer_account,
        &zkdtcc_types::AssetClass::UsdcSac,
        &true,
        &true,
        &true,
        &true,
        &hash(env, 5),
        &hash(env, 6),
    );

    let participant_registry_id = env.register(
        participant_registry::ParticipantRegistry,
        ParticipantRegistryArgs::__constructor(&admin),
    );
    let participant_registry =
        participant_registry::ParticipantRegistryClient::new(env, &participant_registry_id);
    participant_registry.set_operator(&admin, &operator, &true);

    let issuer_participant_id_hash = hash(env, 10);
    let claimant_participant_id_hash = hash(env, 11);
    participant_registry.register_participant(
        &operator,
        &issuer_participant_id_hash,
        &issuer,
        &ParticipantRole::IssuerOrDtcAdmin,
        &hash(env, 12),
        &hash(env, 13),
        &hash(env, 14),
    );
    participant_registry.register_participant(
        &operator,
        &claimant_participant_id_hash,
        &claimant,
        &ParticipantRole::InstitutionTrader,
        &hash(env, 15),
        &hash(env, 16),
        &hash(env, 17),
    );

    let collateral_policy_id = env.register(
        collateral_policy::CollateralPolicy,
        CollateralPolicyArgs::__constructor(&admin, &asset_registry_id, &1_000_000i128, &88u64),
    );
    let collateral_policy = collateral_policy::CollateralPolicyClient::new(env, &collateral_policy_id);
    collateral_policy.set_operator(&admin, &operator, &true);
    collateral_policy.upsert_asset_policy(
        &operator,
        &asset,
        &7u32,
        &10_000u32,
        &100_000i128,
        &88u64,
        &true,
    );
    collateral_policy.upsert_asset_policy(
        &operator,
        &payout_asset,
        &7u32,
        &10_000u32,
        &100_000i128,
        &88u64,
        &true,
    );

    let proof_gateway_id = env.register(
        ProofGateway,
        ProofGatewayArgs::__constructor(&admin, &participant_registry_id, &collateral_policy_id),
    );
    let proof_gateway = proof_gateway::ProofGatewayClient::new(env, &proof_gateway_id);
    proof_gateway.set_operator(&admin, &operator, &true);

    let engine_id = env.register(
        CorporateActionsEngine,
        CorporateActionsEngineArgs::__constructor(&admin, &participant_registry_id, &proof_gateway_id),
    );
    let engine = CorporateActionsEngineClient::new(env, &engine_id);
    engine.set_operator(&admin, &operator, &true);

    PhaseSixContext {
        operator,
        issuer,
        claimant,
        collateral_policy_id,
        proof_gateway_id,
        engine_id,
        claimant_participant_id_hash,
        asset,
        payout_asset,
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
    proof: &Bytes,
) -> zkdtcc_types::ProofReceipt {
    let proof_gateway = proof_gateway::ProofGatewayClient::new(env, proof_gateway_id);
    let summary = collateral_policy::CollateralPolicyClient::new(env, collateral_policy_id)
        .get_policy_summary();
    let nonce = hash(env, nonce_seed);
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
        proof,
    )
}

fn generate_entitlement_claim_bundle(
    env: &Env,
    statement_hash: &BytesN<32>,
    fixture_options: Value,
) -> RuntimeClaimBundle {
    let suffix = encode_hex(&statement_hash.to_array()[..4]).replace("0x", "");
    let output = Command::new("node")
        .current_dir(repo_root())
        .env("ZKDTCC_CIRCUIT_NAMESPACE", "phase6-engine-test")
        .arg("scripts/generate-entitlement-claim-proof.mjs")
        .arg(encode_hex(&statement_hash.to_array()))
        .arg(format!("claim-{suffix}"))
        .arg(fixture_options.to_string())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "entitlement claim bundle generation failed\nstdout:\n{}\nstderr:\n{}",
        std::string::String::from_utf8_lossy(&output.stdout),
        std::string::String::from_utf8_lossy(&output.stderr),
    );

    let stdout = std::string::String::from_utf8(output.stdout).unwrap();
    let marker = "__PHASE6_BUNDLE__";
    let line = stdout
        .lines()
        .find(|line| line.starts_with(marker))
        .expect("missing phase 6 bundle marker");
    let summary: Value = serde_json::from_str(&line[marker.len()..]).unwrap();
    let bundle = &summary["bundle"];

    RuntimeClaimBundle {
        event_id_hash: bytesn32_from_hex(env, bundle["eventIdHashHex"].as_str().unwrap()),
        event_root: bytesn32_from_hex(env, bundle["eventRootHex"].as_str().unwrap()),
        claim_commitment: bytesn32_from_hex(env, bundle["claimCommitmentHex"].as_str().unwrap()),
        claim_nullifier: bytesn32_from_hex(env, bundle["claimNullifierHex"].as_str().unwrap()),
        claim_amount: bundle["claimAmount"].as_str().unwrap().parse().unwrap(),
        entitlement_quantity: bundle["entitlementQuantity"].as_str().unwrap().parse().unwrap(),
        verification_key: bytes_from_hex(env, bundle["verificationKeyHex"].as_str().unwrap()),
        proof_payload: bytes_from_hex(env, bundle["proofPayloadHex"].as_str().unwrap()),
    }
}

fn build_real_bundle(ctx: &PhaseSixContext, env: &Env) -> RuntimeClaimBundle {
    let fixture_options = json!({
        "participantIdHashHex": encode_hex(&ctx.claimant_participant_id_hash.to_array()),
        "assetIdHashHex": encode_hex(&hash(env, 21).to_array()),
        "eventIdHashHex": encode_hex(&hash(env, 22).to_array())
    });
    let provisional_bundle =
        generate_entitlement_claim_bundle(env, &hash(env, 90), fixture_options.clone());

    let proof_gateway = proof_gateway::ProofGatewayClient::new(env, &ctx.proof_gateway_id);
    let collateral_policy = collateral_policy::CollateralPolicyClient::new(env, &ctx.collateral_policy_id);
    let summary = collateral_policy.get_policy_summary();
    let claim_nonce = hash(env, 91);
    let statement_hash = proof_gateway.build_statement_hash(
        &ProofType::EntitlementClaim,
        &ctx.claimant_participant_id_hash,
        &ctx.claimant,
        &claim_nonce,
        &900u32,
        &summary.policy_version,
        &summary.current_epoch,
        &provisional_bundle.claim_commitment,
        &summary.required_margin,
    );
    generate_entitlement_claim_bundle(env, &statement_hash, fixture_options)
}

#[test]
fn registers_event_and_records_real_claim() {
    let env = Env::default();
    let ctx = setup_phase_six(&env);
    let claim_verifier_id = hash(&env, 30);
    let collateral_policy = collateral_policy::CollateralPolicyClient::new(&env, &ctx.collateral_policy_id);
    collateral_policy.set_accepted_verifier(
        &ctx.operator,
        &ProofType::EntitlementClaim,
        &claim_verifier_id,
        &true,
    );

    let bundle = build_real_bundle(&ctx, &env);
    let verifier_contract = env.register(
        EntitlementClaimVerifier,
        EntitlementClaimVerifierArgs::__constructor(&bundle.verification_key),
    );
    let proof_gateway = proof_gateway::ProofGatewayClient::new(&env, &ctx.proof_gateway_id);
    proof_gateway.set_verifier_route(
        &ctx.operator,
        &claim_verifier_id,
        &verifier_contract,
        &true,
    );

    let engine = CorporateActionsEngineClient::new(&env, &ctx.engine_id);
    let event = engine.register_event(
        &ctx.issuer,
        &bundle.event_id_hash,
        &claim_verifier_id,
        &ctx.asset,
        &ctx.payout_asset,
        &CorporateActionType::Coupon,
        &bundle.event_root,
        &hash(&env, 31),
        &hash(&env, 32),
        &1_700_000_000u64,
        &1_699_000_000u64,
        &1_701_000_000u64,
        &0u32,
        &950u32,
        &25i128,
    );

    let receipt = create_proof_receipt(
        &env,
        &ctx.collateral_policy_id,
        &ctx.proof_gateway_id,
        &ctx.claimant,
        &ctx.claimant_participant_id_hash,
        &claim_verifier_id,
        &ProofType::EntitlementClaim,
        &bundle.claim_commitment,
        91,
        900,
        &bundle.proof_payload,
    );

    let claim = engine.claim(
        &ctx.claimant,
        &receipt.receipt_id,
        &event.event_id,
        &bundle.claim_commitment,
        &bundle.claim_nullifier,
        &bundle.entitlement_quantity,
        &bundle.claim_amount,
    );

    assert_eq!(event.event_root, bundle.event_root);
    assert_eq!(claim.event_id, event.event_id);
    assert_eq!(claim.claim_nullifier, bundle.claim_nullifier);
    assert_eq!(claim.disclosed_claim_amount, bundle.claim_amount);
    assert!(engine.has_event(&event.event_id));
    assert!(engine.has_claim(&claim.claim_id));
    assert!(engine.is_claim_nullifier_used(&event.event_id, &bundle.claim_nullifier));
    let stored = engine.get_claim(&claim.claim_id);
    assert_eq!(stored.claim_commitment, bundle.claim_commitment);
    let replay = env.try_invoke_contract::<CorporateActionClaimRecord, CorporateActionsEngineError>(
        &ctx.engine_id,
        &Symbol::new(&env, "claim"),
        soroban_sdk::vec![
            &env,
            ctx.claimant.into_val(&env),
            receipt.receipt_id.into_val(&env),
            event.event_id.into_val(&env),
            bundle.claim_commitment.into_val(&env),
            bundle.claim_nullifier.into_val(&env),
            bundle.entitlement_quantity.into_val(&env),
            bundle.claim_amount.into_val(&env),
        ],
    );
    assert!(matches!(
        replay,
        Err(Ok(CorporateActionsEngineError::ClaimNullifierUsed))
    ));
}

#[test]
fn rejects_claim_before_window() {
    let env = Env::default();
    let ctx = setup_phase_six(&env);
    let engine = CorporateActionsEngineClient::new(&env, &ctx.engine_id);

    let event = engine.register_event(
        &ctx.issuer,
        &hash(&env, 40),
        &hash(&env, 41),
        &ctx.asset,
        &ctx.payout_asset,
        &CorporateActionType::Coupon,
        &hash(&env, 42),
        &hash(&env, 43),
        &hash(&env, 44),
        &2u64,
        &1u64,
        &3u64,
        &50u32,
        &100u32,
        &5i128,
    );

    let result = env.try_invoke_contract::<CorporateActionClaimRecord, CorporateActionsEngineError>(
        &ctx.engine_id,
        &Symbol::new(&env, "claim"),
        soroban_sdk::vec![
            &env,
            ctx.claimant.into_val(&env),
            hash(&env, 45).into_val(&env),
            event.event_id.into_val(&env),
            hash(&env, 46).into_val(&env),
            hash(&env, 47).into_val(&env),
            10i128.into_val(&env),
            250i128.into_val(&env),
        ],
    );
    assert!(matches!(
        result,
        Err(Ok(CorporateActionsEngineError::ClaimWindowClosed))
    ));
}

#[test]
fn operator_can_close_event() {
    let env = Env::default();
    let ctx = setup_phase_six(&env);
    let engine = CorporateActionsEngineClient::new(&env, &ctx.engine_id);

    let event = engine.register_event(
        &ctx.issuer,
        &hash(&env, 49),
        &hash(&env, 50),
        &ctx.asset,
        &ctx.payout_asset,
        &CorporateActionType::Dividend,
        &hash(&env, 51),
        &hash(&env, 52),
        &hash(&env, 53),
        &2u64,
        &1u64,
        &3u64,
        &10u32,
        &20u32,
        &5i128,
    );
    let closed = engine.set_event_status(&ctx.operator, &event.event_id, &CorporateActionStatus::Closed);
    assert_eq!(closed.status, CorporateActionStatus::Closed);
}
