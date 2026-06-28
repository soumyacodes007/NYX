#![no_std]

use asset_registry::AssetRegistryClient;
use compliance_control::ComplianceControlClient;
use participant_registry::ParticipantRegistryClient;
use proof_gateway::ProofGatewayClient;
use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, Bytes, BytesN,
    Env,
};
use zkdtcc_types::{
    CorporateActionClaimRecord, CorporateActionClaimStatus, CorporateActionEventRecord,
    CorporateActionStatus, CorporateActionType, ParticipantRole, ParticipantStatus, ProofReceipt,
    ProofType,
};

const INSTANCE_BUMP_THRESHOLD: u32 = 17_280;
const INSTANCE_BUMP_TO: u32 = 518_400;
const PERSISTENT_BUMP_THRESHOLD: u32 = 17_280;
const PERSISTENT_BUMP_TO: u32 = 518_400;
const EVENT_DOMAIN: &[u8] = b"zkdtcc:corp-action-event:v1";
const CLAIM_DOMAIN: &[u8] = b"zkdtcc:corp-action-claim:v1";

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Operator(Address),
    ParticipantRegistry,
    ProofGateway,
    AssetRegistry,
    ComplianceControl,
    Event(BytesN<32>),
    Claim(BytesN<32>),
    ClaimNullifier(BytesN<32>, BytesN<32>),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum CorporateActionsEngineError {
    Unauthorized = 1,
    ParticipantMismatch = 2,
    WrongParticipantRole = 3,
    ParticipantNotActive = 4,
    EventExists = 5,
    EventNotFound = 6,
    InvalidEventSchedule = 7,
    EventNotActive = 8,
    ClaimWindowClosed = 9,
    ProofReceiptNotFound = 10,
    WrongProofType = 11,
    ProofParticipantMismatch = 12,
    ProofSubmitterMismatch = 13,
    ProofVerifierMismatch = 14,
    ProofExpired = 15,
    ClaimCommitmentMismatch = 16,
    ClaimNullifierUsed = 17,
    ClaimExists = 18,
    InvalidDisclosedAmounts = 19,
    ProofReceiptNotUsable = 20,
    AssetActionsDisabled = 21,
    ProtocolPaused = 22,
    ParticipantFrozen = 23,
    AssetPaused = 24,
    ClaimNotFound = 25,
}

#[contractevent(topics = ["operator_set"])]
pub struct OperatorSetEvent {
    pub operator: Address,
    pub enabled: bool,
}

#[contractevent(topics = ["event_registered"])]
pub struct CorporateActionRegisteredEvent {
    pub event_id: BytesN<32>,
    pub asset: Address,
    pub payout_asset: Address,
}

#[contractevent(topics = ["event_status_set"])]
pub struct CorporateActionStatusSetEvent {
    pub event_id: BytesN<32>,
    pub status: CorporateActionStatus,
}

#[contractevent(topics = ["claim_recorded"])]
pub struct ClaimRecordedEvent {
    pub claim_id: BytesN<32>,
    pub event_id: BytesN<32>,
    pub claim_nullifier: BytesN<32>,
}

#[contract]
pub struct CorporateActionsEngine;

#[contractimpl]
impl CorporateActionsEngine {
    pub fn __constructor(
        env: Env,
        admin: Address,
        participant_registry: Address,
        proof_gateway: Address,
        asset_registry: Address,
        compliance_control: Address,
    ) {
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::ParticipantRegistry, &participant_registry);
        env.storage()
            .instance()
            .set(&DataKey::ProofGateway, &proof_gateway);
        env.storage()
            .instance()
            .set(&DataKey::AssetRegistry, &asset_registry);
        env.storage()
            .instance()
            .set(&DataKey::ComplianceControl, &compliance_control);
        bump_instance(&env);
    }

    pub fn set_operator(
        env: Env,
        admin: Address,
        operator: Address,
        enabled: bool,
    ) -> Result<(), CorporateActionsEngineError> {
        require_admin_auth(&env, &admin)?;
        let key = DataKey::Operator(operator.clone());
        env.storage().persistent().set(&key, &enabled);
        bump_persistent(&env, &key);
        bump_instance(&env);
        OperatorSetEvent { operator, enabled }.publish(&env);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build_event_id(
        env: Env,
        issuer: Address,
        asset: Address,
        payout_asset: Address,
        event_root: BytesN<32>,
        manifest_hash: BytesN<32>,
        record_date: u64,
        payable_date: u64,
    ) -> BytesN<32> {
        derive_event_id(
            &env,
            &issuer,
            &asset,
            &payout_asset,
            &event_root,
            &manifest_hash,
            record_date,
            payable_date,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn register_event(
        env: Env,
        issuer: Address,
        event_id: BytesN<32>,
        verifier_id: BytesN<32>,
        asset: Address,
        payout_asset: Address,
        action_type: CorporateActionType,
        event_root: BytesN<32>,
        manifest_hash: BytesN<32>,
        metadata_hash: BytesN<32>,
        record_date: u64,
        ex_date: u64,
        payable_date: u64,
        claim_start_ledger: u32,
        claim_end_ledger: u32,
        payout_rate: i128,
    ) -> Result<CorporateActionEventRecord, CorporateActionsEngineError> {
        issuer.require_auth();
        ensure_issuer_admin(&env, &issuer)?;
        ensure_protocol_not_paused(&env)?;
        ensure_assets_live(&env, &asset, &payout_asset)?;
        if claim_start_ledger >= claim_end_ledger || ex_date > record_date || record_date > payable_date {
            return Err(CorporateActionsEngineError::InvalidEventSchedule);
        }

        let key = DataKey::Event(event_id.clone());
        if env.storage().persistent().has(&key) {
            return Err(CorporateActionsEngineError::EventExists);
        }

        let record = CorporateActionEventRecord {
            event_id: event_id.clone(),
            asset: asset.clone(),
            payout_asset: payout_asset.clone(),
            issuer: issuer.clone(),
            verifier_id,
            action_type,
            status: CorporateActionStatus::Active,
            event_root,
            manifest_hash,
            metadata_hash,
            record_date,
            ex_date,
            payable_date,
            claim_start_ledger,
            claim_end_ledger,
            payout_rate,
            withholding_policy_hash: zero_hash(&env),
            created_ledger: env.ledger().sequence(),
            updated_ledger: env.ledger().sequence(),
        };

        env.storage().persistent().set(&key, &record);
        bump_persistent(&env, &key);
        bump_instance(&env);
        CorporateActionRegisteredEvent {
            event_id,
            asset,
            payout_asset,
        }
        .publish(&env);
        Ok(record)
    }

    pub fn set_event_status(
        env: Env,
        operator: Address,
        event_id: BytesN<32>,
        status: CorporateActionStatus,
    ) -> Result<CorporateActionEventRecord, CorporateActionsEngineError> {
        require_operator_auth(&env, &operator)?;
        let mut record = load_event(&env, &event_id)?;
        record.status = status.clone();
        record.updated_ledger = env.ledger().sequence();
        save_event(&env, &record);
        bump_instance(&env);
        CorporateActionStatusSetEvent { event_id, status }.publish(&env);
        Ok(record)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn claim(
        env: Env,
        claimant: Address,
        proof_receipt_id: BytesN<32>,
        event_id: BytesN<32>,
        claim_commitment: BytesN<32>,
        claim_nullifier: BytesN<32>,
        disclosed_entitlement_quantity: i128,
        disclosed_claim_amount: i128,
    ) -> Result<CorporateActionClaimRecord, CorporateActionsEngineError> {
        claimant.require_auth();
        let participant_id_hash = ensure_trader(&env, &claimant)?;
        ensure_protocol_not_paused(&env)?;
        ensure_participant_not_frozen(&env, &participant_id_hash)?;
        if disclosed_entitlement_quantity <= 0 || disclosed_claim_amount <= 0 {
            return Err(CorporateActionsEngineError::InvalidDisclosedAmounts);
        }

        let event = load_event(&env, &event_id)?;
        ensure_assets_live(&env, &event.asset, &event.payout_asset)?;
        if event.status != CorporateActionStatus::Active {
            return Err(CorporateActionsEngineError::EventNotActive);
        }
        let ledger = env.ledger().sequence();
        if ledger < event.claim_start_ledger || ledger > event.claim_end_ledger {
            return Err(CorporateActionsEngineError::ClaimWindowClosed);
        }

        let proof_receipt = load_proof_receipt(&env, &proof_receipt_id)?;
        ensure_claim_receipt(
            &env,
            &proof_receipt,
            &claimant,
            &participant_id_hash,
            &event.verifier_id,
            &claim_commitment,
        )?;

        let nullifier_key = DataKey::ClaimNullifier(event_id.clone(), claim_nullifier.clone());
        if env.storage().persistent().has(&nullifier_key) {
            return Err(CorporateActionsEngineError::ClaimNullifierUsed);
        }

        let claim_id = derive_claim_id(
            &env,
            &event_id,
            &proof_receipt_id,
            &claim_commitment,
            &claim_nullifier,
        );
        let claim_key = DataKey::Claim(claim_id.clone());
        if env.storage().persistent().has(&claim_key) {
            return Err(CorporateActionsEngineError::ClaimExists);
        }

        let claim = CorporateActionClaimRecord {
            claim_id: claim_id.clone(),
            event_id: event_id.clone(),
            claimant,
            participant_id_hash,
            verifier_id: event.verifier_id,
            proof_receipt_id,
            claim_commitment,
            claim_nullifier: claim_nullifier.clone(),
            disclosed_entitlement_quantity,
            disclosed_claim_amount,
            claim_status: CorporateActionClaimStatus::Recorded,
            payment_batch_id: zero_hash(&env),
            reversal_reference: zero_hash(&env),
            recorded_ledger: ledger,
        };

        env.storage().persistent().set(&nullifier_key, &claim_id);
        env.storage().persistent().set(&claim_key, &claim);
        bump_persistent(&env, &nullifier_key);
        bump_persistent(&env, &claim_key);
        bump_instance(&env);

        ClaimRecordedEvent {
            claim_id,
            event_id,
            claim_nullifier,
        }
        .publish(&env);
        Ok(claim)
    }

    pub fn set_withholding_policy(
        env: Env,
        operator: Address,
        event_id: BytesN<32>,
        withholding_policy_hash: BytesN<32>,
    ) -> Result<CorporateActionEventRecord, CorporateActionsEngineError> {
        require_operator_auth(&env, &operator)?;
        let mut record = load_event(&env, &event_id)?;
        record.withholding_policy_hash = withholding_policy_hash;
        record.updated_ledger = env.ledger().sequence();
        save_event(&env, &record);
        bump_instance(&env);
        Ok(record)
    }

    pub fn mark_claim_paid(
        env: Env,
        operator: Address,
        claim_id: BytesN<32>,
        payment_batch_id: BytesN<32>,
    ) -> Result<CorporateActionClaimRecord, CorporateActionsEngineError> {
        require_operator_auth(&env, &operator)?;
        let mut claim = load_claim(&env, &claim_id)?;
        claim.claim_status = CorporateActionClaimStatus::Paid;
        claim.payment_batch_id = payment_batch_id;
        save_claim(&env, &claim);
        bump_instance(&env);
        Ok(claim)
    }

    pub fn reverse_claim(
        env: Env,
        operator: Address,
        claim_id: BytesN<32>,
        reversal_reference: BytesN<32>,
    ) -> Result<CorporateActionClaimRecord, CorporateActionsEngineError> {
        require_operator_auth(&env, &operator)?;
        let mut claim = load_claim(&env, &claim_id)?;
        claim.claim_status = CorporateActionClaimStatus::Reversed;
        claim.reversal_reference = reversal_reference;
        save_claim(&env, &claim);
        bump_instance(&env);
        Ok(claim)
    }

    pub fn get_event(
        env: Env,
        event_id: BytesN<32>,
    ) -> Result<CorporateActionEventRecord, CorporateActionsEngineError> {
        load_event(&env, &event_id)
    }

    pub fn get_claim(
        env: Env,
        claim_id: BytesN<32>,
    ) -> Result<CorporateActionClaimRecord, CorporateActionsEngineError> {
        let key = DataKey::Claim(claim_id);
        let claim = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(CorporateActionsEngineError::ClaimNotFound)?;
        bump_persistent(&env, &key);
        bump_instance(&env);
        Ok(claim)
    }

    pub fn has_event(env: Env, event_id: BytesN<32>) -> bool {
        let key = DataKey::Event(event_id);
        let exists = env.storage().persistent().has(&key);
        if exists {
            bump_persistent(&env, &key);
        }
        bump_instance(&env);
        exists
    }

    pub fn has_claim(env: Env, claim_id: BytesN<32>) -> bool {
        let key = DataKey::Claim(claim_id);
        let exists = env.storage().persistent().has(&key);
        if exists {
            bump_persistent(&env, &key);
        }
        bump_instance(&env);
        exists
    }

    pub fn is_claim_nullifier_used(
        env: Env,
        event_id: BytesN<32>,
        claim_nullifier: BytesN<32>,
    ) -> bool {
        let key = DataKey::ClaimNullifier(event_id, claim_nullifier);
        let exists = env.storage().persistent().has(&key);
        if exists {
            bump_persistent(&env, &key);
        }
        bump_instance(&env);
        exists
    }
}

fn ensure_issuer_admin(env: &Env, issuer: &Address) -> Result<(), CorporateActionsEngineError> {
    let (participant_id_hash, participant) = load_active_participant(env, issuer)?;
    let _ = participant_id_hash;
    if participant.role != ParticipantRole::IssuerOrDtcAdmin {
        return Err(CorporateActionsEngineError::WrongParticipantRole);
    }
    Ok(())
}

fn ensure_trader(env: &Env, claimant: &Address) -> Result<BytesN<32>, CorporateActionsEngineError> {
    let (participant_id_hash, participant) = load_active_participant(env, claimant)?;
    if participant.role != ParticipantRole::InstitutionTrader {
        return Err(CorporateActionsEngineError::WrongParticipantRole);
    }
    Ok(participant_id_hash)
}

fn load_active_participant(
    env: &Env,
    wallet: &Address,
) -> Result<(BytesN<32>, zkdtcc_types::ParticipantRecord), CorporateActionsEngineError> {
    let participant_registry: Address = env
        .storage()
        .instance()
        .get(&DataKey::ParticipantRegistry)
        .unwrap();
    let registry = ParticipantRegistryClient::new(env, &participant_registry);
    if !registry.is_wallet_registered(wallet) {
        return Err(CorporateActionsEngineError::ParticipantMismatch);
    }
    let participant_id_hash = registry.wallet_owner(wallet);
    let participant = registry.get_participant(&participant_id_hash);
    if participant.status != ParticipantStatus::Active {
        return Err(CorporateActionsEngineError::ParticipantNotActive);
    }
    Ok((participant_id_hash, participant))
}

fn ensure_claim_receipt(
    env: &Env,
    proof_receipt: &ProofReceipt,
    claimant: &Address,
    participant_id_hash: &BytesN<32>,
    verifier_id: &BytesN<32>,
    claim_commitment: &BytesN<32>,
) -> Result<(), CorporateActionsEngineError> {
    if proof_receipt.proof_type != ProofType::EntitlementClaim {
        return Err(CorporateActionsEngineError::WrongProofType);
    }
    if &proof_receipt.participant_id_hash != participant_id_hash {
        return Err(CorporateActionsEngineError::ProofParticipantMismatch);
    }
    if &proof_receipt.submitter != claimant {
        return Err(CorporateActionsEngineError::ProofSubmitterMismatch);
    }
    if &proof_receipt.verifier_id != verifier_id {
        return Err(CorporateActionsEngineError::ProofVerifierMismatch);
    }
    if &proof_receipt.portfolio_commitment != claim_commitment {
        return Err(CorporateActionsEngineError::ClaimCommitmentMismatch);
    }
    let proof_gateway: Address = env.storage().instance().get(&DataKey::ProofGateway).unwrap();
    let gateway = ProofGatewayClient::new(env, &proof_gateway);
    if !gateway.is_receipt_usable(&proof_receipt.receipt_id) {
        return Err(CorporateActionsEngineError::ProofReceiptNotUsable);
    }
    if env.ledger().sequence() > proof_receipt.expiry_ledger {
        return Err(CorporateActionsEngineError::ProofExpired);
    }
    Ok(())
}

fn load_proof_receipt(
    env: &Env,
    proof_receipt_id: &BytesN<32>,
) -> Result<ProofReceipt, CorporateActionsEngineError> {
    let proof_gateway: Address = env.storage().instance().get(&DataKey::ProofGateway).unwrap();
    let gateway = ProofGatewayClient::new(env, &proof_gateway);
    if !gateway.has_receipt(proof_receipt_id) {
        return Err(CorporateActionsEngineError::ProofReceiptNotFound);
    }
    Ok(gateway.get_receipt(proof_receipt_id))
}

fn load_event(
    env: &Env,
    event_id: &BytesN<32>,
) -> Result<CorporateActionEventRecord, CorporateActionsEngineError> {
    let key = DataKey::Event(event_id.clone());
    let event = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(CorporateActionsEngineError::EventNotFound)?;
    bump_persistent(env, &key);
    bump_instance(env);
    Ok(event)
}

fn save_event(env: &Env, event: &CorporateActionEventRecord) {
    let key = DataKey::Event(event.event_id.clone());
    env.storage().persistent().set(&key, event);
    bump_persistent(env, &key);
}

fn load_claim(
    env: &Env,
    claim_id: &BytesN<32>,
) -> Result<CorporateActionClaimRecord, CorporateActionsEngineError> {
    let key = DataKey::Claim(claim_id.clone());
    let claim = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(CorporateActionsEngineError::ClaimNotFound)?;
    bump_persistent(env, &key);
    Ok(claim)
}

fn save_claim(env: &Env, claim: &CorporateActionClaimRecord) {
    let key = DataKey::Claim(claim.claim_id.clone());
    env.storage().persistent().set(&key, claim);
    bump_persistent(env, &key);
}

fn ensure_protocol_not_paused(env: &Env) -> Result<(), CorporateActionsEngineError> {
    let compliance_control: Address = env
        .storage()
        .instance()
        .get(&DataKey::ComplianceControl)
        .unwrap();
    let compliance = ComplianceControlClient::new(env, &compliance_control);
    if compliance.is_globally_paused() {
        return Err(CorporateActionsEngineError::ProtocolPaused);
    }
    Ok(())
}

fn ensure_participant_not_frozen(
    env: &Env,
    participant_id_hash: &BytesN<32>,
) -> Result<(), CorporateActionsEngineError> {
    let compliance_control: Address = env
        .storage()
        .instance()
        .get(&DataKey::ComplianceControl)
        .unwrap();
    let compliance = ComplianceControlClient::new(env, &compliance_control);
    if compliance.is_participant_frozen(participant_id_hash) {
        return Err(CorporateActionsEngineError::ParticipantFrozen);
    }
    Ok(())
}

fn ensure_assets_live(
    env: &Env,
    asset: &Address,
    payout_asset: &Address,
) -> Result<(), CorporateActionsEngineError> {
    let compliance_control: Address = env
        .storage()
        .instance()
        .get(&DataKey::ComplianceControl)
        .unwrap();
    let compliance = ComplianceControlClient::new(env, &compliance_control);
    if compliance.is_asset_paused(asset) || compliance.is_asset_paused(payout_asset) {
        return Err(CorporateActionsEngineError::AssetPaused);
    }
    let asset_registry: Address = env.storage().instance().get(&DataKey::AssetRegistry).unwrap();
    let registry = AssetRegistryClient::new(env, &asset_registry);
    if !registry.is_asset_corp_actions_enabled(asset)
        || !registry.is_asset_corp_actions_enabled(payout_asset)
    {
        return Err(CorporateActionsEngineError::AssetActionsDisabled);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn derive_event_id(
    env: &Env,
    issuer: &Address,
    asset: &Address,
    payout_asset: &Address,
    event_root: &BytesN<32>,
    manifest_hash: &BytesN<32>,
    record_date: u64,
    payable_date: u64,
) -> BytesN<32> {
    let mut material = Bytes::new(env);
    material.extend_from_slice(EVENT_DOMAIN);
    append_address(&mut material, issuer);
    append_address(&mut material, asset);
    append_address(&mut material, payout_asset);
    material.extend_from_slice(&event_root.to_array());
    material.extend_from_slice(&manifest_hash.to_array());
    material.extend_from_slice(&record_date.to_be_bytes());
    material.extend_from_slice(&payable_date.to_be_bytes());
    env.crypto().sha256(&material).into()
}

fn derive_claim_id(
    env: &Env,
    event_id: &BytesN<32>,
    proof_receipt_id: &BytesN<32>,
    claim_commitment: &BytesN<32>,
    claim_nullifier: &BytesN<32>,
) -> BytesN<32> {
    let mut material = Bytes::new(env);
    material.extend_from_slice(CLAIM_DOMAIN);
    append_address(&mut material, &env.current_contract_address());
    material.extend_from_slice(&event_id.to_array());
    material.extend_from_slice(&proof_receipt_id.to_array());
    material.extend_from_slice(&claim_commitment.to_array());
    material.extend_from_slice(&claim_nullifier.to_array());
    env.crypto().sha256(&material).into()
}

fn append_address(material: &mut Bytes, address: &Address) {
    let address_bytes = address.to_string().to_bytes();
    material.extend_from_slice(&address_bytes.len().to_be_bytes());
    material.append(&address_bytes);
}

fn zero_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0; 32])
}

fn require_admin_auth(env: &Env, admin: &Address) -> Result<(), CorporateActionsEngineError> {
    admin.require_auth();
    let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    if stored_admin != *admin {
        return Err(CorporateActionsEngineError::Unauthorized);
    }
    Ok(())
}

fn require_operator_auth(env: &Env, operator: &Address) -> Result<(), CorporateActionsEngineError> {
    operator.require_auth();
    let key = DataKey::Operator(operator.clone());
    let enabled = env.storage().persistent().get(&key).unwrap_or(false);
    if !enabled {
        return Err(CorporateActionsEngineError::Unauthorized);
    }
    bump_persistent(env, &key);
    Ok(())
}

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_TO);
}

fn bump_persistent<K>(env: &Env, key: &K)
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    env.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_BUMP_TO);
}

#[cfg(test)]
mod tests;
