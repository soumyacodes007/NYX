#![no_std]

use collateral_policy::CollateralPolicyClient;
use participant_registry::ParticipantRegistryClient;
use soroban_sdk::{
    contract, contractclient, contracterror, contractevent, contractimpl, contracttype, Address,
    Bytes, BytesN, Env,
};
use zkdtcc_types::{
    ProofReceipt, ProofType, ProofVerifierPolicy, ProofVerifierRoute, RevokedProofReceipt,
};

const INSTANCE_BUMP_THRESHOLD: u32 = 17_280;
const INSTANCE_BUMP_TO: u32 = 518_400;
const PERSISTENT_BUMP_THRESHOLD: u32 = 17_280;
const PERSISTENT_BUMP_TO: u32 = 518_400;
const STATEMENT_DOMAIN: &[u8] = b"zkdtcc:proof-gateway:v1";

#[contractclient(name = "VerifierClient")]
pub trait VerifierContract {
    fn verify(env: Env, proof_type: u32, statement_hash: BytesN<32>, proof: Bytes) -> bool;
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Operator(Address),
    ParticipantRegistry,
    CollateralPolicy,
    VerifierRoute(BytesN<32>),
    VerifierPolicy(BytesN<32>),
    UsedNonce(ProofType, BytesN<32>, BytesN<32>),
    Receipt(BytesN<32>),
    RevokedReceipt(BytesN<32>),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ProofGatewayError {
    Unauthorized = 1,
    ParticipantMismatch = 2,
    ProofExpired = 3,
    PolicyVersionMismatch = 4,
    EpochMismatch = 5,
    RequiredMarginMismatch = 6,
    UnsupportedVerifier = 7,
    VerifierRouteNotFound = 8,
    VerifierRejected = 9,
    NonceUsed = 10,
    ReceiptNotFound = 11,
    VerifierPolicyInactive = 12,
    ReceiptRevoked = 13,
}

#[contractevent(topics = ["operator_set"])]
pub struct OperatorSetEvent {
    pub operator: Address,
    pub enabled: bool,
}

#[contractevent(topics = ["verifier_route_set"])]
pub struct VerifierRouteSetEvent {
    pub verifier_id: BytesN<32>,
    pub verifier: Address,
    pub enabled: bool,
}

#[contractevent(topics = ["verifier_policy_set"])]
pub struct VerifierPolicySetEvent {
    pub verifier_id: BytesN<32>,
    pub enabled: bool,
}

#[contractevent(topics = ["proof_recorded"])]
pub struct ProofRecordedEvent {
    pub receipt_id: BytesN<32>,
    pub proof_type: ProofType,
    pub participant_id_hash: BytesN<32>,
    pub verifier_id: BytesN<32>,
}

#[contractevent(topics = ["receipt_revoked"])]
pub struct ReceiptRevokedEvent {
    pub receipt_id: BytesN<32>,
    pub case_id: BytesN<32>,
}

#[contract]
pub struct ProofGateway;

#[contractimpl]
impl ProofGateway {
    pub fn __constructor(
        env: Env,
        admin: Address,
        participant_registry: Address,
        collateral_policy: Address,
    ) {
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::ParticipantRegistry, &participant_registry);
        env.storage()
            .instance()
            .set(&DataKey::CollateralPolicy, &collateral_policy);
        bump_instance(&env);
    }

    pub fn set_operator(
        env: Env,
        admin: Address,
        operator: Address,
        enabled: bool,
    ) -> Result<(), ProofGatewayError> {
        require_admin_auth(&env, &admin)?;
        let key = DataKey::Operator(operator.clone());
        env.storage().persistent().set(&key, &enabled);
        bump_persistent(&env, &key);
        bump_instance(&env);
        OperatorSetEvent { operator, enabled }.publish(&env);
        Ok(())
    }

    pub fn set_verifier_route(
        env: Env,
        operator: Address,
        verifier_id: BytesN<32>,
        verifier: Address,
        enabled: bool,
    ) -> Result<(), ProofGatewayError> {
        require_operator_auth(&env, &operator)?;
        let route = ProofVerifierRoute {
            verifier_id: verifier_id.clone(),
            verifier: verifier.clone(),
            enabled,
            updated_ledger: env.ledger().sequence(),
        };
        let key = DataKey::VerifierRoute(verifier_id.clone());
        env.storage().persistent().set(&key, &route);
        bump_persistent(&env, &key);
        bump_instance(&env);
        VerifierRouteSetEvent {
            verifier_id,
            verifier,
            enabled,
        }
        .publish(&env);
        Ok(())
    }

    pub fn set_verifier_policy(
        env: Env,
        operator: Address,
        verifier_id: BytesN<32>,
        enabled: bool,
        valid_from_ledger: u32,
        valid_until_ledger: u32,
        policy_cutoff_hash: BytesN<32>,
    ) -> Result<(), ProofGatewayError> {
        require_operator_auth(&env, &operator)?;
        let policy = ProofVerifierPolicy {
            verifier_id: verifier_id.clone(),
            enabled,
            valid_from_ledger,
            valid_until_ledger,
            policy_cutoff_hash,
            updated_ledger: env.ledger().sequence(),
        };
        let key = DataKey::VerifierPolicy(verifier_id.clone());
        env.storage().persistent().set(&key, &policy);
        bump_persistent(&env, &key);
        bump_instance(&env);
        VerifierPolicySetEvent {
            verifier_id,
            enabled,
        }
        .publish(&env);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build_statement_hash(
        env: Env,
        proof_type: ProofType,
        participant_id_hash: BytesN<32>,
        submitter: Address,
        nonce: BytesN<32>,
        expiry_ledger: u32,
        policy_version: u32,
        epoch_id: u64,
        portfolio_commitment: BytesN<32>,
        required_margin: i128,
    ) -> BytesN<32> {
        derive_statement_hash(
            &env,
            &proof_type,
            &participant_id_hash,
            &submitter,
            &nonce,
            expiry_ledger,
            policy_version,
            epoch_id,
            &portfolio_commitment,
            required_margin,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn verify_and_record(
        env: Env,
        submitter: Address,
        participant_id_hash: BytesN<32>,
        proof_type: ProofType,
        verifier_id: BytesN<32>,
        portfolio_commitment: BytesN<32>,
        nonce: BytesN<32>,
        expiry_ledger: u32,
        policy_version: u32,
        epoch_id: u64,
        required_margin: i128,
        proof: Bytes,
    ) -> Result<ProofReceipt, ProofGatewayError> {
        submitter.require_auth();
        ensure_participant_binding(&env, &submitter, &participant_id_hash)?;
        ensure_policy_inputs(
            &env,
            &proof_type,
            &verifier_id,
            policy_version,
            epoch_id,
            required_margin,
        )?;

        if env.ledger().sequence() > expiry_ledger {
            return Err(ProofGatewayError::ProofExpired);
        }

        let nonce_key = DataKey::UsedNonce(
            proof_type.clone(),
            participant_id_hash.clone(),
            nonce.clone(),
        );
        if env.storage().persistent().has(&nonce_key) {
            return Err(ProofGatewayError::NonceUsed);
        }

        let route = load_verifier_route(&env, &verifier_id)?;
        if !route.enabled {
            return Err(ProofGatewayError::UnsupportedVerifier);
        }
        ensure_verifier_policy_active(&env, &verifier_id)?;

        let statement_hash = derive_statement_hash(
            &env,
            &proof_type,
            &participant_id_hash,
            &submitter,
            &nonce,
            expiry_ledger,
            policy_version,
            epoch_id,
            &portfolio_commitment,
            required_margin,
        );

        let verifier_client = VerifierClient::new(&env, &route.verifier);
        let verified = verifier_client.verify(
            &proof_type_code(&proof_type),
            &statement_hash,
            &proof,
        );
        if !verified {
            return Err(ProofGatewayError::VerifierRejected);
        }

        let mut material = Bytes::new(&env);
        material.extend_from_slice(&statement_hash.to_array());
        material.extend_from_slice(&verifier_id.to_array());
        material.extend_from_slice(&nonce.to_array());
        let receipt_id: BytesN<32> = env.crypto().sha256(&material).into();

        let receipt = ProofReceipt {
            receipt_id: receipt_id.clone(),
            proof_type: proof_type.clone(),
            participant_id_hash: participant_id_hash.clone(),
            submitter: submitter.clone(),
            verifier_id: verifier_id.clone(),
            statement_hash,
            portfolio_commitment,
            nonce,
            policy_version,
            epoch_id,
            required_margin,
            expiry_ledger,
            recorded_ledger: env.ledger().sequence(),
        };

        env.storage().persistent().set(&nonce_key, &receipt_id);
        env.storage()
            .persistent()
            .set(&DataKey::Receipt(receipt_id.clone()), &receipt);
        bump_persistent(&env, &nonce_key);
        bump_persistent(&env, &DataKey::Receipt(receipt_id.clone()));
        bump_instance(&env);

        ProofRecordedEvent {
            receipt_id,
            proof_type,
            participant_id_hash,
            verifier_id,
        }
        .publish(&env);

        Ok(receipt)
    }

    pub fn revoke_receipt(
        env: Env,
        operator: Address,
        receipt_id: BytesN<32>,
        reason_code: BytesN<32>,
        case_id: BytesN<32>,
    ) -> Result<RevokedProofReceipt, ProofGatewayError> {
        require_operator_auth(&env, &operator)?;
        let _ = Self::get_receipt(env.clone(), receipt_id.clone())?;
        let revoked = RevokedProofReceipt {
            receipt_id: receipt_id.clone(),
            reason_code,
            case_id: case_id.clone(),
            revoked_ledger: env.ledger().sequence(),
        };
        let key = DataKey::RevokedReceipt(receipt_id.clone());
        env.storage().persistent().set(&key, &revoked);
        bump_persistent(&env, &key);
        bump_instance(&env);
        ReceiptRevokedEvent { receipt_id, case_id }.publish(&env);
        Ok(revoked)
    }

    pub fn get_receipt(
        env: Env,
        receipt_id: BytesN<32>,
    ) -> Result<ProofReceipt, ProofGatewayError> {
        let key = DataKey::Receipt(receipt_id);
        let receipt = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ProofGatewayError::ReceiptNotFound)?;
        bump_persistent(&env, &key);
        bump_instance(&env);
        Ok(receipt)
    }

    pub fn has_receipt(env: Env, receipt_id: BytesN<32>) -> bool {
        let key = DataKey::Receipt(receipt_id);
        let exists = env.storage().persistent().has(&key);
        if exists {
            bump_persistent(&env, &key);
        }
        bump_instance(&env);
        exists
    }

    pub fn is_receipt_usable(env: Env, receipt_id: BytesN<32>) -> bool {
        let receipt = match Self::get_receipt(env.clone(), receipt_id.clone()) {
            Ok(receipt) => receipt,
            Err(_) => return false,
        };
        if env.ledger().sequence() > receipt.expiry_ledger {
            return false;
        }
        let revoked_key = DataKey::RevokedReceipt(receipt_id);
        if env.storage().persistent().has(&revoked_key) {
            bump_persistent(&env, &revoked_key);
            return false;
        }
        ensure_verifier_policy_active(&env, &receipt.verifier_id).is_ok()
    }
}

fn ensure_participant_binding(
    env: &Env,
    submitter: &Address,
    participant_id_hash: &BytesN<32>,
) -> Result<(), ProofGatewayError> {
    let participant_registry: Address = env
        .storage()
        .instance()
        .get(&DataKey::ParticipantRegistry)
        .unwrap();
    let registry = ParticipantRegistryClient::new(env, &participant_registry);
    let owner = registry.wallet_owner(submitter);
    if &owner != participant_id_hash {
        return Err(ProofGatewayError::ParticipantMismatch);
    }
    Ok(())
}

fn ensure_policy_inputs(
    env: &Env,
    proof_type: &ProofType,
    verifier_id: &BytesN<32>,
    policy_version: u32,
    epoch_id: u64,
    required_margin: i128,
) -> Result<(), ProofGatewayError> {
    let collateral_policy: Address = env
        .storage()
        .instance()
        .get(&DataKey::CollateralPolicy)
        .unwrap();
    let policy = CollateralPolicyClient::new(env, &collateral_policy);
    let summary = policy.get_policy_summary();
    if summary.policy_version != policy_version {
        return Err(ProofGatewayError::PolicyVersionMismatch);
    }
    if summary.current_epoch != epoch_id {
        return Err(ProofGatewayError::EpochMismatch);
    }
    if summary.required_margin != required_margin {
        return Err(ProofGatewayError::RequiredMarginMismatch);
    }
    if !policy.is_verifier_accepted(proof_type, verifier_id) {
        return Err(ProofGatewayError::UnsupportedVerifier);
    }
    Ok(())
}

fn load_verifier_route(
    env: &Env,
    verifier_id: &BytesN<32>,
) -> Result<ProofVerifierRoute, ProofGatewayError> {
    let key = DataKey::VerifierRoute(verifier_id.clone());
    let route = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(ProofGatewayError::VerifierRouteNotFound)?;
    bump_persistent(env, &key);
    Ok(route)
}

fn ensure_verifier_policy_active(
    env: &Env,
    verifier_id: &BytesN<32>,
) -> Result<(), ProofGatewayError> {
    let key = DataKey::VerifierPolicy(verifier_id.clone());
    if !env.storage().persistent().has(&key) {
        return Ok(());
    }
    let policy: ProofVerifierPolicy = env.storage().persistent().get(&key).unwrap();
    bump_persistent(env, &key);
    let ledger = env.ledger().sequence();
    if !policy.enabled || ledger < policy.valid_from_ledger || ledger > policy.valid_until_ledger {
        return Err(ProofGatewayError::VerifierPolicyInactive);
    }
    Ok(())
}

fn derive_statement_hash(
    env: &Env,
    proof_type: &ProofType,
    participant_id_hash: &BytesN<32>,
    submitter: &Address,
    nonce: &BytesN<32>,
    expiry_ledger: u32,
    policy_version: u32,
    epoch_id: u64,
    portfolio_commitment: &BytesN<32>,
    required_margin: i128,
) -> BytesN<32> {
    let mut material = Bytes::new(env);
    material.extend_from_slice(STATEMENT_DOMAIN);
    material.extend_from_slice(&env.ledger().network_id().to_array());
    append_address(&mut material, submitter);
    append_address(&mut material, &env.current_contract_address());
    material.extend_from_slice(&proof_type_code(proof_type).to_be_bytes());
    material.extend_from_slice(&participant_id_hash.to_array());
    material.extend_from_slice(&nonce.to_array());
    material.extend_from_slice(&expiry_ledger.to_be_bytes());
    material.extend_from_slice(&policy_version.to_be_bytes());
    material.extend_from_slice(&epoch_id.to_be_bytes());
    material.extend_from_slice(&portfolio_commitment.to_array());
    material.extend_from_slice(&required_margin.to_be_bytes());
    env.crypto().sha256(&material).into()
}

fn append_address(material: &mut Bytes, address: &Address) {
    let address_str = address.to_string();
    let address_bytes = address_str.to_bytes();
    material.extend_from_slice(&address_bytes.len().to_be_bytes());
    material.append(&address_bytes);
}

fn proof_type_code(proof_type: &ProofType) -> u32 {
    match proof_type {
        ProofType::Eligibility => 1,
        ProofType::CollateralSufficiency => 2,
        ProofType::UnencumberedLot => 3,
        ProofType::PrivateMatch => 4,
        ProofType::BatchNetting => 5,
        ProofType::EntitlementClaim => 6,
    }
}

fn require_admin_auth(env: &Env, admin: &Address) -> Result<(), ProofGatewayError> {
    admin.require_auth();
    let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    if &stored_admin != admin {
        return Err(ProofGatewayError::Unauthorized);
    }
    bump_instance(env);
    Ok(())
}

fn require_operator_auth(env: &Env, operator: &Address) -> Result<(), ProofGatewayError> {
    operator.require_auth();
    let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    if admin == *operator {
        bump_instance(env);
        return Ok(());
    }

    let key = DataKey::Operator(operator.clone());
    let is_enabled = env.storage().persistent().get(&key).unwrap_or(false);
    if !is_enabled {
        return Err(ProofGatewayError::Unauthorized);
    }
    bump_persistent(env, &key);
    bump_instance(env);
    Ok(())
}

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_TO);
}

fn bump_persistent(env: &Env, key: &DataKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_BUMP_TO);
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use asset_registry::AssetRegistryArgs;
    use collateral_policy::CollateralPolicyArgs;
    use participant_registry::ParticipantRegistryArgs;
    use serde_json::Value;
    use soroban_sdk::{
        contract, contractimpl,
        testutils::{Address as _, Ledger as _},
        vec, Address, BytesN, Env, IntoVal, Symbol,
    };
    use std::{format, path::PathBuf, process::Command};
    use unencumbered_lot_verifier::{UnencumberedLotVerifier, UnencumberedLotVerifierArgs};

    #[contract]
    struct MockVerifier;

    #[contractimpl]
    impl MockVerifier {
        pub fn verify(_env: Env, _proof_type: u32, statement_hash: BytesN<32>, proof: Bytes) -> bool {
            proof == Bytes::from_array(proof.env(), &statement_hash.to_array())
        }
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

    struct RuntimeProofBundle {
        availability_root: BytesN<32>,
        proof_nonce: BytesN<32>,
        verification_key: Bytes,
        proof_payload: Bytes,
    }

    fn generate_runtime_proof_bundle(
        env: &Env,
        statement_hash: &BytesN<32>,
        participant_id_hash: &BytesN<32>,
    ) -> RuntimeProofBundle {
        let suffix = encode_hex(&statement_hash.to_array()[..4]).replace("0x", "");
        let output = Command::new("node")
            .current_dir(repo_root())
            .arg("scripts/generate-unencumbered-proof.mjs")
            .arg(encode_hex(&statement_hash.to_array()))
            .arg(format!("gateway-{suffix}"))
            .arg(encode_hex(&participant_id_hash.to_array()))
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "proof generation failed\nstdout:\n{}\nstderr:\n{}",
            std::string::String::from_utf8_lossy(&output.stdout),
            std::string::String::from_utf8_lossy(&output.stderr),
        );

        let stdout = std::string::String::from_utf8(output.stdout).unwrap();
        let marker = "__PHASE3_BUNDLE__";
        let line = stdout
            .lines()
            .find(|line| line.starts_with(marker))
            .expect("missing proof bundle marker");
        let summary: Value = serde_json::from_str(&line[marker.len()..]).unwrap();
        let bundle = &summary["bundle"];

        RuntimeProofBundle {
            availability_root: bytesn32_from_hex(
                env,
                bundle["availabilityRootHex"].as_str().unwrap(),
            ),
            proof_nonce: bytesn32_from_hex(env, bundle["proofNonceHex"].as_str().unwrap()),
            verification_key: bytes_from_hex(
                env,
                bundle["verificationKeyHex"].as_str().unwrap(),
            ),
            proof_payload: bytes_from_hex(env, bundle["proofPayloadHex"].as_str().unwrap()),
        }
    }

    fn setup_phase_two(env: &Env) -> (Address, Address, Address, Address, Address, Address, BytesN<32>) {
        env.mock_all_auths();

        let admin = Address::generate(env);
        let operator = Address::generate(env);
        let submitter = Address::generate(env);
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
        let participant_id_hash = hash(env, 10);
        participant_registry.register_participant(
            &operator,
            &participant_id_hash,
            &submitter,
            &zkdtcc_types::ParticipantRole::InstitutionTrader,
            &hash(env, 11),
            &hash(env, 12),
            &hash(env, 13),
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

        let verifier_id = hash(env, 20);
        collateral_policy.set_accepted_verifier(
            &operator,
            &ProofType::CollateralSufficiency,
            &verifier_id,
            &true,
        );

        (
            admin,
            operator,
            submitter,
            participant_registry_id,
            collateral_policy_id,
            asset,
            participant_id_hash,
        )
    }

    #[test]
    fn records_proof_receipt() {
        let env = Env::default();
        let (
            admin,
            operator,
            submitter,
            participant_registry_id,
            collateral_policy_id,
            _asset,
            participant_id_hash,
        ) = setup_phase_two(&env);
        let verifier_id = hash(&env, 20);
        let verifier_contract = env.register(MockVerifier, ());

        let contract_id = env.register(
            ProofGateway,
            ProofGatewayArgs::__constructor(&admin, &participant_registry_id, &collateral_policy_id),
        );
        let client = ProofGatewayClient::new(&env, &contract_id);
        client.set_operator(&admin, &operator, &true);
        client.set_verifier_route(&operator, &verifier_id, &verifier_contract, &true);

        let statement_hash = client.build_statement_hash(
            &ProofType::CollateralSufficiency,
            &participant_id_hash,
            &submitter,
            &hash(&env, 30),
            &500u32,
            &3u32,
            &42u64,
            &hash(&env, 31),
            &1_000_000i128,
        );
        let proof = Bytes::from_array(&env, &statement_hash.to_array());
        let receipt = client.verify_and_record(
            &submitter,
            &participant_id_hash,
            &ProofType::CollateralSufficiency,
            &verifier_id,
            &hash(&env, 31),
            &hash(&env, 30),
            &500u32,
            &3u32,
            &42u64,
            &1_000_000i128,
            &proof,
        );

        assert_eq!(receipt.participant_id_hash, participant_id_hash);
        assert_eq!(receipt.verifier_id, verifier_id);
        assert_eq!(receipt.statement_hash, statement_hash);
        assert_eq!(client.get_receipt(&receipt.receipt_id), receipt);
    }

    #[test]
    fn revokes_receipt_without_erasing_history() {
        let env = Env::default();
        let (
            admin,
            operator,
            submitter,
            participant_registry_id,
            collateral_policy_id,
            _asset,
            participant_id_hash,
        ) = setup_phase_two(&env);
        let verifier_id = hash(&env, 21);
        let verifier_contract = env.register(MockVerifier, ());

        let contract_id = env.register(
            ProofGateway,
            ProofGatewayArgs::__constructor(&admin, &participant_registry_id, &collateral_policy_id),
        );
        let client = ProofGatewayClient::new(&env, &contract_id);
        client.set_operator(&admin, &operator, &true);
        client.set_verifier_route(&operator, &verifier_id, &verifier_contract, &true);

        let statement_hash = client.build_statement_hash(
            &ProofType::CollateralSufficiency,
            &participant_id_hash,
            &submitter,
            &hash(&env, 60),
            &500u32,
            &3u32,
            &42u64,
            &hash(&env, 61),
            &1_000_000i128,
        );
        let proof = Bytes::from_array(&env, &statement_hash.to_array());
        let receipt = client.verify_and_record(
            &submitter,
            &participant_id_hash,
            &ProofType::CollateralSufficiency,
            &verifier_id,
            &hash(&env, 61),
            &hash(&env, 60),
            &500u32,
            &3u32,
            &42u64,
            &1_000_000i128,
            &proof,
        );

        assert!(client.is_receipt_usable(&receipt.receipt_id));
        client.revoke_receipt(&operator, &receipt.receipt_id, &hash(&env, 62), &hash(&env, 63));
        assert_eq!(client.get_receipt(&receipt.receipt_id), receipt);
        assert!(!client.is_receipt_usable(&receipt.receipt_id));
    }

    #[test]
    fn rejects_participant_mismatch() {
        let env = Env::default();
        let (
            admin,
            operator,
            submitter,
            participant_registry_id,
            collateral_policy_id,
            _asset,
            _participant_id_hash,
        ) = setup_phase_two(&env);
        let verifier_id = hash(&env, 20);
        let verifier_contract = env.register(MockVerifier, ());

        let contract_id = env.register(
            ProofGateway,
            ProofGatewayArgs::__constructor(&admin, &participant_registry_id, &collateral_policy_id),
        );
        let client = ProofGatewayClient::new(&env, &contract_id);
        client.set_operator(&admin, &operator, &true);
        client.set_verifier_route(&operator, &verifier_id, &verifier_contract, &true);

        let result = env.try_invoke_contract::<ProofReceipt, ProofGatewayError>(
            &contract_id,
            &Symbol::new(&env, "verify_and_record"),
            vec![
                &env,
                submitter.into_val(&env),
                hash(&env, 99).into_val(&env),
                ProofType::CollateralSufficiency.into_val(&env),
                verifier_id.into_val(&env),
                hash(&env, 31).into_val(&env),
                hash(&env, 30).into_val(&env),
                500u32.into_val(&env),
                3u32.into_val(&env),
                42u64.into_val(&env),
                1_000_000i128.into_val(&env),
                Bytes::from_slice(&env, &[1]).into_val(&env),
            ],
        );

        assert!(matches!(result, Err(Ok(ProofGatewayError::ParticipantMismatch))));
    }

    #[test]
    fn rejects_policy_version_mismatch() {
        let env = Env::default();
        let (
            admin,
            operator,
            submitter,
            participant_registry_id,
            collateral_policy_id,
            _asset,
            participant_id_hash,
        ) = setup_phase_two(&env);
        let verifier_id = hash(&env, 20);
        let verifier_contract = env.register(MockVerifier, ());
        let policy = collateral_policy::CollateralPolicyClient::new(&env, &collateral_policy_id);

        let contract_id = env.register(
            ProofGateway,
            ProofGatewayArgs::__constructor(&admin, &participant_registry_id, &collateral_policy_id),
        );
        let client = ProofGatewayClient::new(&env, &contract_id);
        client.set_operator(&admin, &operator, &true);
        client.set_verifier_route(&operator, &verifier_id, &verifier_contract, &true);

        policy.set_global_policy(&operator, &1_500_000i128, &43u64);
        let statement_hash = client.build_statement_hash(
            &ProofType::CollateralSufficiency,
            &participant_id_hash,
            &submitter,
            &hash(&env, 32),
            &500u32,
            &3u32,
            &42u64,
            &hash(&env, 33),
            &1_000_000i128,
        );
        let proof = Bytes::from_array(&env, &statement_hash.to_array());

        let result = env.try_invoke_contract::<ProofReceipt, ProofGatewayError>(
            &contract_id,
            &Symbol::new(&env, "verify_and_record"),
            vec![
                &env,
                submitter.into_val(&env),
                participant_id_hash.into_val(&env),
                ProofType::CollateralSufficiency.into_val(&env),
                verifier_id.into_val(&env),
                hash(&env, 33).into_val(&env),
                hash(&env, 32).into_val(&env),
                500u32.into_val(&env),
                3u32.into_val(&env),
                42u64.into_val(&env),
                1_000_000i128.into_val(&env),
                proof.into_val(&env),
            ],
        );

        assert!(matches!(result, Err(Ok(ProofGatewayError::PolicyVersionMismatch))));
    }

    #[test]
    fn rejects_reused_nonce() {
        let env = Env::default();
        let (
            admin,
            operator,
            submitter,
            participant_registry_id,
            collateral_policy_id,
            _asset,
            participant_id_hash,
        ) = setup_phase_two(&env);
        let verifier_id = hash(&env, 20);
        let verifier_contract = env.register(MockVerifier, ());

        let contract_id = env.register(
            ProofGateway,
            ProofGatewayArgs::__constructor(&admin, &participant_registry_id, &collateral_policy_id),
        );
        let client = ProofGatewayClient::new(&env, &contract_id);
        client.set_operator(&admin, &operator, &true);
        client.set_verifier_route(&operator, &verifier_id, &verifier_contract, &true);

        let nonce = hash(&env, 40);
        let statement_hash = client.build_statement_hash(
            &ProofType::CollateralSufficiency,
            &participant_id_hash,
            &submitter,
            &nonce,
            &500u32,
            &3u32,
            &42u64,
            &hash(&env, 41),
            &1_000_000i128,
        );
        let proof = Bytes::from_array(&env, &statement_hash.to_array());
        client.verify_and_record(
            &submitter,
            &participant_id_hash,
            &ProofType::CollateralSufficiency,
            &verifier_id,
            &hash(&env, 41),
            &nonce,
            &500u32,
            &3u32,
            &42u64,
            &1_000_000i128,
            &proof,
        );

        let result = env.try_invoke_contract::<ProofReceipt, ProofGatewayError>(
            &contract_id,
            &Symbol::new(&env, "verify_and_record"),
            vec![
                &env,
                submitter.into_val(&env),
                participant_id_hash.into_val(&env),
                ProofType::CollateralSufficiency.into_val(&env),
                verifier_id.into_val(&env),
                hash(&env, 41).into_val(&env),
                nonce.into_val(&env),
                500u32.into_val(&env),
                3u32.into_val(&env),
                42u64.into_val(&env),
                1_000_000i128.into_val(&env),
                proof.into_val(&env),
            ],
        );

        assert!(matches!(result, Err(Ok(ProofGatewayError::NonceUsed))));
    }

    #[test]
    fn rejects_unsupported_verifier() {
        let env = Env::default();
        let (
            admin,
            operator,
            submitter,
            participant_registry_id,
            collateral_policy_id,
            _asset,
            participant_id_hash,
        ) = setup_phase_two(&env);
        let contract_id = env.register(
            ProofGateway,
            ProofGatewayArgs::__constructor(&admin, &participant_registry_id, &collateral_policy_id),
        );
        let client = ProofGatewayClient::new(&env, &contract_id);
        client.set_operator(&admin, &operator, &true);

        let result = env.try_invoke_contract::<ProofReceipt, ProofGatewayError>(
            &contract_id,
            &Symbol::new(&env, "verify_and_record"),
            vec![
                &env,
                submitter.into_val(&env),
                participant_id_hash.into_val(&env),
                ProofType::CollateralSufficiency.into_val(&env),
                hash(&env, 99).into_val(&env),
                hash(&env, 41).into_val(&env),
                hash(&env, 42).into_val(&env),
                500u32.into_val(&env),
                3u32.into_val(&env),
                42u64.into_val(&env),
                1_000_000i128.into_val(&env),
                Bytes::from_slice(&env, &[1]).into_val(&env),
            ],
        );

        assert!(matches!(result, Err(Ok(ProofGatewayError::UnsupportedVerifier))));
    }

    #[test]
    fn rejects_invalid_proof() {
        let env = Env::default();
        let (
            admin,
            operator,
            submitter,
            participant_registry_id,
            collateral_policy_id,
            _asset,
            participant_id_hash,
        ) = setup_phase_two(&env);
        let verifier_id = hash(&env, 20);
        let verifier_contract = env.register(MockVerifier, ());

        let contract_id = env.register(
            ProofGateway,
            ProofGatewayArgs::__constructor(&admin, &participant_registry_id, &collateral_policy_id),
        );
        let client = ProofGatewayClient::new(&env, &contract_id);
        client.set_operator(&admin, &operator, &true);
        client.set_verifier_route(&operator, &verifier_id, &verifier_contract, &true);

        let result = env.try_invoke_contract::<ProofReceipt, ProofGatewayError>(
            &contract_id,
            &Symbol::new(&env, "verify_and_record"),
            vec![
                &env,
                submitter.into_val(&env),
                participant_id_hash.into_val(&env),
                ProofType::CollateralSufficiency.into_val(&env),
                verifier_id.into_val(&env),
                hash(&env, 41).into_val(&env),
                hash(&env, 42).into_val(&env),
                500u32.into_val(&env),
                3u32.into_val(&env),
                42u64.into_val(&env),
                1_000_000i128.into_val(&env),
                Bytes::from_slice(&env, &[7, 7, 7]).into_val(&env),
            ],
        );

        assert!(matches!(result, Err(Ok(ProofGatewayError::VerifierRejected))));
    }

    #[test]
    fn rejects_expired_proof() {
        let env = Env::default();
        let (
            admin,
            operator,
            submitter,
            participant_registry_id,
            collateral_policy_id,
            _asset,
            participant_id_hash,
        ) = setup_phase_two(&env);
        let verifier_id = hash(&env, 20);
        let verifier_contract = env.register(MockVerifier, ());
        env.ledger().set_sequence_number(600);

        let contract_id = env.register(
            ProofGateway,
            ProofGatewayArgs::__constructor(&admin, &participant_registry_id, &collateral_policy_id),
        );
        let client = ProofGatewayClient::new(&env, &contract_id);
        client.set_operator(&admin, &operator, &true);
        client.set_verifier_route(&operator, &verifier_id, &verifier_contract, &true);

        let statement_hash = client.build_statement_hash(
            &ProofType::CollateralSufficiency,
            &participant_id_hash,
            &submitter,
            &hash(&env, 50),
            &500u32,
            &3u32,
            &42u64,
            &hash(&env, 51),
            &1_000_000i128,
        );
        let proof = Bytes::from_array(&env, &statement_hash.to_array());

        let result = env.try_invoke_contract::<ProofReceipt, ProofGatewayError>(
            &contract_id,
            &Symbol::new(&env, "verify_and_record"),
            vec![
                &env,
                submitter.into_val(&env),
                participant_id_hash.into_val(&env),
                ProofType::CollateralSufficiency.into_val(&env),
                verifier_id.into_val(&env),
                hash(&env, 51).into_val(&env),
                hash(&env, 50).into_val(&env),
                500u32.into_val(&env),
                3u32.into_val(&env),
                42u64.into_val(&env),
                1_000_000i128.into_val(&env),
                proof.into_val(&env),
            ],
        );

        assert!(matches!(result, Err(Ok(ProofGatewayError::ProofExpired))));
    }

    #[test]
    fn records_unencumbered_lot_receipt_with_real_bn254_proof() {
        let env = Env::default();
        let (
            admin,
            operator,
            submitter,
            participant_registry_id,
            collateral_policy_id,
            _asset,
            _participant_id_hash,
        ) = setup_phase_two(&env);
        let verifier_id = hash(&env, 77);
        let policy = collateral_policy::CollateralPolicyClient::new(&env, &collateral_policy_id);
        policy.set_accepted_verifier(
            &operator,
            &ProofType::UnencumberedLot,
            &verifier_id,
            &true,
        );

        let contract_id = env.register(
            ProofGateway,
            ProofGatewayArgs::__constructor(&admin, &participant_registry_id, &collateral_policy_id),
        );
        let client = ProofGatewayClient::new(&env, &contract_id);

        let participant_id_hash = hash(&env, 10);
        let provisional_bundle =
            generate_runtime_proof_bundle(&env, &hash(&env, 200), &participant_id_hash);

        let summary = policy.get_policy_summary();
        let statement_hash = client.build_statement_hash(
            &ProofType::UnencumberedLot,
            &participant_id_hash,
            &submitter,
            &provisional_bundle.proof_nonce,
            &500u32,
            &summary.policy_version,
            &summary.current_epoch,
            &provisional_bundle.availability_root,
            &summary.required_margin,
        );
        let bundle = generate_runtime_proof_bundle(&env, &statement_hash, &participant_id_hash);
        let verifier_contract = env.register(
            UnencumberedLotVerifier,
            UnencumberedLotVerifierArgs::__constructor(&bundle.verification_key),
        );

        client.set_operator(&admin, &operator, &true);
        client.set_verifier_route(&operator, &verifier_id, &verifier_contract, &true);

        let receipt = client.verify_and_record(
            &submitter,
            &participant_id_hash,
            &ProofType::UnencumberedLot,
            &verifier_id,
            &bundle.availability_root,
            &bundle.proof_nonce,
            &500u32,
            &summary.policy_version,
            &summary.current_epoch,
            &summary.required_margin,
            &bundle.proof_payload,
        );

        assert_eq!(receipt.proof_type, ProofType::UnencumberedLot);
        assert_eq!(receipt.participant_id_hash, participant_id_hash);
        assert_eq!(receipt.statement_hash, statement_hash);
        assert_eq!(receipt.portfolio_commitment, bundle.availability_root);
        assert_eq!(receipt.nonce, bundle.proof_nonce);
    }
}
