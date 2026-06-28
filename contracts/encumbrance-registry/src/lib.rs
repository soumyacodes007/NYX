#![no_std]

use soroban_sdk::{
    contract, contractclient, contracterror, contractevent, contractimpl, contracttype, Address,
    Bytes, BytesN, Env,
};
use zkdtcc_types::{
    AvailabilityAttestation, EncumbranceAttestor, EncumbranceLock, ProofReceipt, ProofType,
};

#[contractclient(name = "AssetRegistryClient")]
pub trait AssetRegistryContract {
    fn is_supported_asset(env: Env, asset: Address) -> bool;
}

#[contractclient(name = "ParticipantRegistryClient")]
pub trait ParticipantRegistryContract {
    fn is_wallet_registered(env: Env, wallet: Address) -> bool;
    fn wallet_owner(env: Env, wallet: Address) -> BytesN<32>;
}

#[contractclient(name = "ProofGatewayClient")]
pub trait ProofGatewayContract {
    fn has_receipt(env: Env, receipt_id: BytesN<32>) -> bool;
    fn get_receipt(env: Env, receipt_id: BytesN<32>) -> ProofReceipt;
}

const INSTANCE_BUMP_THRESHOLD: u32 = 17_280;
const INSTANCE_BUMP_TO: u32 = 518_400;
const PERSISTENT_BUMP_THRESHOLD: u32 = 17_280;
const PERSISTENT_BUMP_TO: u32 = 518_400;
const ATTESTATION_DOMAIN: &[u8] = b"zkdtcc:encumbrance-attestation:v1";
const LOCK_DOMAIN: &[u8] = b"zkdtcc:encumbrance-lock:v1";

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Operator(Address),
    ParticipantRegistry,
    AssetRegistry,
    ProofGateway,
    Attestor(BytesN<32>),
    Attestation(BytesN<32>),
    LotLock(BytesN<32>),
    ReleasedLot(BytesN<32>),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EncumbranceRegistryError {
    Unauthorized = 1,
    UnsupportedAsset = 2,
    ParticipantMismatch = 3,
    AttestorNotFound = 4,
    AttestorDisabled = 5,
    AttestationExists = 6,
    AttestationNotFound = 7,
    AttestationExpired = 8,
    AttestationFromFuture = 9,
    InvalidQuantity = 10,
    InvalidExpiry = 11,
    LockExists = 12,
    LockNotFound = 13,
    LockReleased = 14,
    LockStillActive = 15,
    ProofReceiptNotFound = 16,
    WrongProofType = 17,
    ProofParticipantMismatch = 18,
    ProofSubmitterMismatch = 19,
    ProofCommitmentMismatch = 20,
    ProofExpired = 21,
    AttestationBindingMismatch = 22,
}

#[contractevent(topics = ["operator_set"])]
pub struct OperatorSetEvent {
    pub operator: Address,
    pub enabled: bool,
}

#[contractevent(topics = ["attestor_set"])]
pub struct AttestorSetEvent {
    pub attestor_id: BytesN<32>,
    pub enabled: bool,
}

#[contractevent(topics = ["attestation_recorded"])]
pub struct AttestationRecordedEvent {
    pub attestation_id: BytesN<32>,
    pub attestor_id: BytesN<32>,
    pub participant_id_hash: BytesN<32>,
}

#[contractevent(topics = ["lot_locked"])]
pub struct LotLockedEvent {
    pub lot_nullifier: BytesN<32>,
    pub participant_id_hash: BytesN<32>,
    pub expiry_ledger: u32,
}

#[contractevent(topics = ["lot_released"])]
pub struct LotReleasedEvent {
    pub lot_nullifier: BytesN<32>,
    pub release_reference: BytesN<32>,
}

#[contract]
pub struct EncumbranceRegistry;

#[contractimpl]
impl EncumbranceRegistry {
    pub fn __constructor(
        env: Env,
        admin: Address,
        participant_registry: Address,
        asset_registry: Address,
        proof_gateway: Address,
    ) {
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::ParticipantRegistry, &participant_registry);
        env.storage()
            .instance()
            .set(&DataKey::AssetRegistry, &asset_registry);
        env.storage()
            .instance()
            .set(&DataKey::ProofGateway, &proof_gateway);
        bump_instance(&env);
    }

    pub fn set_operator(
        env: Env,
        admin: Address,
        operator: Address,
        enabled: bool,
    ) -> Result<(), EncumbranceRegistryError> {
        require_admin_auth(&env, &admin)?;
        let key = DataKey::Operator(operator.clone());
        env.storage().persistent().set(&key, &enabled);
        bump_persistent(&env, &key);
        bump_instance(&env);
        OperatorSetEvent { operator, enabled }.publish(&env);
        Ok(())
    }

    pub fn set_attestor(
        env: Env,
        operator: Address,
        attestor_id: BytesN<32>,
        public_key: BytesN<32>,
        enabled: bool,
    ) -> Result<(), EncumbranceRegistryError> {
        require_operator_auth(&env, &operator)?;
        let record = EncumbranceAttestor {
            attestor_id: attestor_id.clone(),
            public_key,
            enabled,
            updated_ledger: env.ledger().sequence(),
        };
        let key = DataKey::Attestor(attestor_id.clone());
        env.storage().persistent().set(&key, &record);
        bump_persistent(&env, &key);
        bump_instance(&env);
        AttestorSetEvent {
            attestor_id,
            enabled,
        }
        .publish(&env);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build_attestation_hash(
        env: Env,
        attestor_id: BytesN<32>,
        participant_id_hash: BytesN<32>,
        asset: Address,
        availability_root: BytesN<32>,
        scope_hash: BytesN<32>,
        issued_at_ledger: u32,
        expiry_ledger: u32,
    ) -> BytesN<32> {
        derive_attestation_hash(
            &env,
            &attestor_id,
            &participant_id_hash,
            &asset,
            &availability_root,
            &scope_hash,
            issued_at_ledger,
            expiry_ledger,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn publish_attestation(
        env: Env,
        attestor_id: BytesN<32>,
        participant_id_hash: BytesN<32>,
        asset: Address,
        availability_root: BytesN<32>,
        scope_hash: BytesN<32>,
        issued_at_ledger: u32,
        expiry_ledger: u32,
        signature: BytesN<64>,
    ) -> Result<AvailabilityAttestation, EncumbranceRegistryError> {
        ensure_supported_asset(&env, &asset)?;
        validate_attestation_window(&env, issued_at_ledger, expiry_ledger)?;

        let attestor = load_attestor(&env, &attestor_id)?;
        if !attestor.enabled {
            return Err(EncumbranceRegistryError::AttestorDisabled);
        }

        let digest = derive_attestation_hash(
            &env,
            &attestor_id,
            &participant_id_hash,
            &asset,
            &availability_root,
            &scope_hash,
            issued_at_ledger,
            expiry_ledger,
        );
        let digest_bytes = Bytes::from_array(&env, &digest.to_array());
        env.crypto()
            .ed25519_verify(&attestor.public_key, &digest_bytes, &signature);

        let attestation_id = derive_attestation_id(&env, &digest, &attestor_id);
        let key = DataKey::Attestation(attestation_id.clone());
        if env.storage().persistent().has(&key) {
            return Err(EncumbranceRegistryError::AttestationExists);
        }

        let attestation = AvailabilityAttestation {
            attestation_id: attestation_id.clone(),
            attestor_id: attestor_id.clone(),
            participant_id_hash: participant_id_hash.clone(),
            asset,
            availability_root,
            scope_hash,
            issued_at_ledger,
            expiry_ledger,
            recorded_ledger: env.ledger().sequence(),
        };

        env.storage().persistent().set(&key, &attestation);
        bump_persistent(&env, &key);
        bump_instance(&env);

        AttestationRecordedEvent {
            attestation_id,
            attestor_id,
            participant_id_hash,
        }
        .publish(&env);

        Ok(attestation)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn lock_lot(
        env: Env,
        submitter: Address,
        participant_id_hash: BytesN<32>,
        asset: Address,
        attestation_id: BytesN<32>,
        proof_receipt_id: BytesN<32>,
        lot_nullifier: BytesN<32>,
        scope_hash: BytesN<32>,
        reason_hash: BytesN<32>,
        quantity: i128,
        expiry_ledger: u32,
    ) -> Result<EncumbranceLock, EncumbranceRegistryError> {
        submitter.require_auth();
        ensure_supported_asset(&env, &asset)?;
        ensure_participant_binding(&env, &submitter, &participant_id_hash)?;

        if quantity <= 0 {
            return Err(EncumbranceRegistryError::InvalidQuantity);
        }
        if expiry_ledger <= env.ledger().sequence() {
            return Err(EncumbranceRegistryError::InvalidExpiry);
        }

        let lot_key = DataKey::LotLock(lot_nullifier.clone());
        if env.storage().persistent().has(&lot_key) {
            return Err(EncumbranceRegistryError::LockExists);
        }
        let released_key = DataKey::ReleasedLot(lot_nullifier.clone());
        if env.storage().persistent().has(&released_key) {
            return Err(EncumbranceRegistryError::LockExists);
        }

        let attestation = load_attestation(&env, &attestation_id)?;
        if env.ledger().sequence() > attestation.expiry_ledger {
            return Err(EncumbranceRegistryError::AttestationExpired);
        }
        if attestation.participant_id_hash != participant_id_hash
            || attestation.asset != asset
            || attestation.scope_hash != scope_hash
        {
            return Err(EncumbranceRegistryError::AttestationBindingMismatch);
        }
        if expiry_ledger > attestation.expiry_ledger {
            return Err(EncumbranceRegistryError::InvalidExpiry);
        }

        let proof_receipt = load_proof_receipt(&env, &proof_receipt_id)?;
        ensure_proof_binding(
            &env,
            &proof_receipt,
            &submitter,
            &participant_id_hash,
            &attestation.availability_root,
        )?;

        let lock_id = derive_lock_id(
            &env,
            &lot_nullifier,
            &attestation_id,
            &proof_receipt_id,
            &scope_hash,
            &reason_hash,
            quantity,
            expiry_ledger,
        );
        let ledger = env.ledger().sequence();
        let lock = EncumbranceLock {
            lock_id,
            lot_nullifier: lot_nullifier.clone(),
            participant_id_hash: participant_id_hash.clone(),
            submitter,
            asset,
            attestation_id,
            proof_receipt_id,
            availability_root: attestation.availability_root,
            scope_hash,
            reason_hash,
            quantity,
            expiry_ledger,
            released: false,
            release_reference: zero_hash(&env),
            created_ledger: ledger,
            updated_ledger: ledger,
        };

        env.storage().persistent().set(&lot_key, &lock);
        bump_persistent(&env, &lot_key);
        bump_instance(&env);
        LotLockedEvent {
            lot_nullifier,
            participant_id_hash,
            expiry_ledger,
        }
        .publish(&env);
        Ok(lock)
    }

    pub fn release_lot(
        env: Env,
        actor: Address,
        lot_nullifier: BytesN<32>,
        release_reference: BytesN<32>,
    ) -> Result<EncumbranceLock, EncumbranceRegistryError> {
        actor.require_auth();
        let mut lock = load_lock(&env, &lot_nullifier)?;
        if lock.released {
            return Err(EncumbranceRegistryError::LockReleased);
        }
        if actor != lock.submitter && !is_operator_or_admin(&env, &actor) {
            return Err(EncumbranceRegistryError::Unauthorized);
        }
        apply_release(&env, &lot_nullifier, &mut lock, release_reference)
    }

    pub fn sweep_expired_lock(
        env: Env,
        operator: Address,
        lot_nullifier: BytesN<32>,
        release_reference: BytesN<32>,
    ) -> Result<EncumbranceLock, EncumbranceRegistryError> {
        require_operator_auth(&env, &operator)?;
        let mut lock = load_lock(&env, &lot_nullifier)?;
        if lock.released {
            return Err(EncumbranceRegistryError::LockReleased);
        }
        if env.ledger().sequence() <= lock.expiry_ledger {
            return Err(EncumbranceRegistryError::LockStillActive);
        }
        apply_release(&env, &lot_nullifier, &mut lock, release_reference)
    }

    pub fn get_attestor(
        env: Env,
        attestor_id: BytesN<32>,
    ) -> Result<EncumbranceAttestor, EncumbranceRegistryError> {
        load_attestor(&env, &attestor_id)
    }

    pub fn get_attestation(
        env: Env,
        attestation_id: BytesN<32>,
    ) -> Result<AvailabilityAttestation, EncumbranceRegistryError> {
        load_attestation(&env, &attestation_id)
    }

    pub fn get_lock(
        env: Env,
        lot_nullifier: BytesN<32>,
    ) -> Result<EncumbranceLock, EncumbranceRegistryError> {
        load_lock(&env, &lot_nullifier)
    }

    pub fn is_lot_locked(env: Env, lot_nullifier: BytesN<32>) -> bool {
        let key = DataKey::LotLock(lot_nullifier);
        let exists = env.storage().persistent().has(&key);
        if exists {
            bump_persistent(&env, &key);
        }
        bump_instance(&env);
        exists
    }

    pub fn is_lot_released(env: Env, lot_nullifier: BytesN<32>) -> bool {
        let key = DataKey::ReleasedLot(lot_nullifier);
        let exists = env.storage().persistent().has(&key);
        if exists {
            bump_persistent(&env, &key);
        }
        bump_instance(&env);
        exists
    }
}

fn validate_attestation_window(
    env: &Env,
    issued_at_ledger: u32,
    expiry_ledger: u32,
) -> Result<(), EncumbranceRegistryError> {
    if issued_at_ledger == 0 || expiry_ledger <= issued_at_ledger {
        return Err(EncumbranceRegistryError::InvalidExpiry);
    }
    if issued_at_ledger > env.ledger().sequence() {
        return Err(EncumbranceRegistryError::AttestationFromFuture);
    }
    if expiry_ledger <= env.ledger().sequence() {
        return Err(EncumbranceRegistryError::AttestationExpired);
    }
    Ok(())
}

fn ensure_supported_asset(env: &Env, asset: &Address) -> Result<(), EncumbranceRegistryError> {
    let asset_registry: Address = env.storage().instance().get(&DataKey::AssetRegistry).unwrap();
    let registry = AssetRegistryClient::new(env, &asset_registry);
    if !registry.is_supported_asset(asset) {
        return Err(EncumbranceRegistryError::UnsupportedAsset);
    }
    Ok(())
}

fn ensure_participant_binding(
    env: &Env,
    submitter: &Address,
    participant_id_hash: &BytesN<32>,
) -> Result<(), EncumbranceRegistryError> {
    let participant_registry: Address = env
        .storage()
        .instance()
        .get(&DataKey::ParticipantRegistry)
        .unwrap();
    let registry = ParticipantRegistryClient::new(env, &participant_registry);
    if !registry.is_wallet_registered(submitter) {
        return Err(EncumbranceRegistryError::ParticipantMismatch);
    }
    let owner = registry.wallet_owner(submitter);
    if &owner != participant_id_hash {
        return Err(EncumbranceRegistryError::ParticipantMismatch);
    }
    Ok(())
}

fn ensure_proof_binding(
    env: &Env,
    proof_receipt: &ProofReceipt,
    submitter: &Address,
    participant_id_hash: &BytesN<32>,
    availability_root: &BytesN<32>,
) -> Result<(), EncumbranceRegistryError> {
    if proof_receipt.proof_type != ProofType::UnencumberedLot {
        return Err(EncumbranceRegistryError::WrongProofType);
    }
    if &proof_receipt.participant_id_hash != participant_id_hash {
        return Err(EncumbranceRegistryError::ProofParticipantMismatch);
    }
    if &proof_receipt.submitter != submitter {
        return Err(EncumbranceRegistryError::ProofSubmitterMismatch);
    }
    if &proof_receipt.portfolio_commitment != availability_root {
        return Err(EncumbranceRegistryError::ProofCommitmentMismatch);
    }
    if env.ledger().sequence() > proof_receipt.expiry_ledger {
        return Err(EncumbranceRegistryError::ProofExpired);
    }
    Ok(())
}

fn load_proof_receipt(
    env: &Env,
    proof_receipt_id: &BytesN<32>,
) -> Result<ProofReceipt, EncumbranceRegistryError> {
    let proof_gateway: Address = env.storage().instance().get(&DataKey::ProofGateway).unwrap();
    let gateway = ProofGatewayClient::new(env, &proof_gateway);
    if !gateway.has_receipt(proof_receipt_id) {
        return Err(EncumbranceRegistryError::ProofReceiptNotFound);
    }
    Ok(gateway.get_receipt(proof_receipt_id))
}

fn derive_attestation_hash(
    env: &Env,
    attestor_id: &BytesN<32>,
    participant_id_hash: &BytesN<32>,
    asset: &Address,
    availability_root: &BytesN<32>,
    scope_hash: &BytesN<32>,
    issued_at_ledger: u32,
    expiry_ledger: u32,
) -> BytesN<32> {
    let mut material = Bytes::new(env);
    material.extend_from_slice(ATTESTATION_DOMAIN);
    material.extend_from_slice(&env.ledger().network_id().to_array());
    append_address(&mut material, &env.current_contract_address());
    material.extend_from_slice(&attestor_id.to_array());
    material.extend_from_slice(&participant_id_hash.to_array());
    append_address(&mut material, asset);
    material.extend_from_slice(&availability_root.to_array());
    material.extend_from_slice(&scope_hash.to_array());
    material.extend_from_slice(&issued_at_ledger.to_be_bytes());
    material.extend_from_slice(&expiry_ledger.to_be_bytes());
    env.crypto().sha256(&material).into()
}

fn derive_attestation_id(env: &Env, digest: &BytesN<32>, attestor_id: &BytesN<32>) -> BytesN<32> {
    let mut material = Bytes::new(env);
    material.extend_from_slice(&digest.to_array());
    material.extend_from_slice(&attestor_id.to_array());
    env.crypto().sha256(&material).into()
}

fn derive_lock_id(
    env: &Env,
    lot_nullifier: &BytesN<32>,
    attestation_id: &BytesN<32>,
    proof_receipt_id: &BytesN<32>,
    scope_hash: &BytesN<32>,
    reason_hash: &BytesN<32>,
    quantity: i128,
    expiry_ledger: u32,
) -> BytesN<32> {
    let mut material = Bytes::new(env);
    material.extend_from_slice(LOCK_DOMAIN);
    material.extend_from_slice(&env.ledger().network_id().to_array());
    append_address(&mut material, &env.current_contract_address());
    material.extend_from_slice(&lot_nullifier.to_array());
    material.extend_from_slice(&attestation_id.to_array());
    material.extend_from_slice(&proof_receipt_id.to_array());
    material.extend_from_slice(&scope_hash.to_array());
    material.extend_from_slice(&reason_hash.to_array());
    material.extend_from_slice(&quantity.to_be_bytes());
    material.extend_from_slice(&expiry_ledger.to_be_bytes());
    env.crypto().sha256(&material).into()
}

fn append_address(material: &mut Bytes, address: &Address) {
    let address_bytes = address.to_string().to_bytes();
    material.extend_from_slice(&address_bytes.len().to_be_bytes());
    material.append(&address_bytes);
}

fn apply_release(
    env: &Env,
    lot_nullifier: &BytesN<32>,
    lock: &mut EncumbranceLock,
    release_reference: BytesN<32>,
) -> Result<EncumbranceLock, EncumbranceRegistryError> {
    lock.released = true;
    lock.release_reference = release_reference.clone();
    lock.updated_ledger = env.ledger().sequence();

    let lock_key = DataKey::LotLock(lot_nullifier.clone());
    env.storage().persistent().set(&lock_key, lock);
    bump_persistent(env, &lock_key);

    let released_key = DataKey::ReleasedLot(lot_nullifier.clone());
    env.storage()
        .persistent()
        .set(&released_key, &release_reference);
    bump_persistent(env, &released_key);
    bump_instance(env);

    LotReleasedEvent {
        lot_nullifier: lot_nullifier.clone(),
        release_reference,
    }
    .publish(env);

    Ok(lock.clone())
}

fn load_attestor(
    env: &Env,
    attestor_id: &BytesN<32>,
) -> Result<EncumbranceAttestor, EncumbranceRegistryError> {
    let key = DataKey::Attestor(attestor_id.clone());
    let record = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(EncumbranceRegistryError::AttestorNotFound)?;
    bump_persistent(env, &key);
    bump_instance(env);
    Ok(record)
}

fn load_attestation(
    env: &Env,
    attestation_id: &BytesN<32>,
) -> Result<AvailabilityAttestation, EncumbranceRegistryError> {
    let key = DataKey::Attestation(attestation_id.clone());
    let record = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(EncumbranceRegistryError::AttestationNotFound)?;
    bump_persistent(env, &key);
    bump_instance(env);
    Ok(record)
}

fn load_lock(
    env: &Env,
    lot_nullifier: &BytesN<32>,
) -> Result<EncumbranceLock, EncumbranceRegistryError> {
    let key = DataKey::LotLock(lot_nullifier.clone());
    let lock = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(EncumbranceRegistryError::LockNotFound)?;
    bump_persistent(env, &key);
    bump_instance(env);
    Ok(lock)
}

fn zero_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0; 32])
}

fn require_admin_auth(env: &Env, admin: &Address) -> Result<(), EncumbranceRegistryError> {
    admin.require_auth();
    let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    if &stored_admin != admin {
        return Err(EncumbranceRegistryError::Unauthorized);
    }
    bump_instance(env);
    Ok(())
}

fn require_operator_auth(env: &Env, operator: &Address) -> Result<(), EncumbranceRegistryError> {
    operator.require_auth();
    if is_operator_or_admin(env, operator) {
        bump_instance(env);
        return Ok(());
    }
    Err(EncumbranceRegistryError::Unauthorized)
}

fn is_operator_or_admin(env: &Env, operator: &Address) -> bool {
    let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    if admin == *operator {
        return true;
    }
    let key = DataKey::Operator(operator.clone());
    let enabled = env.storage().persistent().get(&key).unwrap_or(false);
    if enabled {
        bump_persistent(env, &key);
    }
    enabled
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
    use ed25519_dalek::{Signer, SigningKey};
    use participant_registry::ParticipantRegistryArgs;
    use proof_gateway::{ProofGateway, ProofGatewayArgs};
    use soroban_sdk::{
        contract, contractimpl,
        testutils::{Address as _, Ledger as _},
        vec, Address, BytesN, Env, IntoVal, Symbol,
    };

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

    #[allow(clippy::type_complexity)]
    fn setup_phase_three(
        env: &Env,
    ) -> (
        Address,
        Address,
        Address,
        Address,
        Address,
        Address,
        Address,
        Address,
        BytesN<32>,
        BytesN<32>,
    ) {
        env.mock_all_auths();
        env.ledger().set_sequence_number(100);

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
            &zkdtcc_types::AssetClass::DtcEntitlement,
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
            &ProofType::UnencumberedLot,
            &verifier_id,
            &true,
        );

        let verifier_contract = env.register(MockVerifier, ());
        let proof_gateway_id = env.register(
            ProofGateway,
            ProofGatewayArgs::__constructor(&admin, &participant_registry_id, &collateral_policy_id),
        );
        let proof_gateway = proof_gateway::ProofGatewayClient::new(env, &proof_gateway_id);
        proof_gateway.set_operator(&admin, &operator, &true);
        proof_gateway.set_verifier_route(&operator, &verifier_id, &verifier_contract, &true);

        (
            admin,
            operator,
            submitter,
            asset,
            asset_registry_id,
            participant_registry_id,
            collateral_policy_id,
            proof_gateway_id,
            participant_id_hash,
            verifier_id,
        )
    }

    fn sign_attestation_hash(env: &Env, signing_key: &SigningKey, digest: &BytesN<32>) -> BytesN<64> {
        let signature = signing_key.sign(&digest.to_array());
        BytesN::from_array(env, &signature.to_bytes())
    }

    fn create_proof_receipt(
        env: &Env,
        collateral_policy_id: &Address,
        proof_gateway_id: &Address,
        submitter: &Address,
        participant_id_hash: &BytesN<32>,
        verifier_id: &BytesN<32>,
        availability_root: &BytesN<32>,
        nonce_seed: u8,
        expiry_ledger: u32,
    ) -> ProofReceipt {
        let proof_gateway = proof_gateway::ProofGatewayClient::new(env, proof_gateway_id);
        let summary = collateral_policy::CollateralPolicyClient::new(env, collateral_policy_id)
            .get_policy_summary();
        let nonce = hash(env, nonce_seed);
        let statement_hash = proof_gateway.build_statement_hash(
            &ProofType::UnencumberedLot,
            participant_id_hash,
            submitter,
            &nonce,
            &expiry_ledger,
            &summary.policy_version,
            &summary.current_epoch,
            availability_root,
            &summary.required_margin,
        );
        let proof = Bytes::from_array(env, &statement_hash.to_array());
        proof_gateway.verify_and_record(
            submitter,
            participant_id_hash,
            &ProofType::UnencumberedLot,
            verifier_id,
            availability_root,
            &nonce,
            &expiry_ledger,
            &summary.policy_version,
            &summary.current_epoch,
            &summary.required_margin,
            &proof,
        )
    }

    #[test]
    fn records_attestation_and_lock() {
        let env = Env::default();
        let (
            admin,
            operator,
            submitter,
            asset,
            asset_registry_id,
            participant_registry_id,
            collateral_policy_id,
            proof_gateway_id,
            participant_id_hash,
            verifier_id,
        ) = setup_phase_three(&env);

        let contract_id = env.register(
            EncumbranceRegistry,
            EncumbranceRegistryArgs::__constructor(
                &admin,
                &participant_registry_id,
                &asset_registry_id,
                &proof_gateway_id,
            ),
        );
        let client = EncumbranceRegistryClient::new(&env, &contract_id);
        client.set_operator(&admin, &operator, &true);

        let signing_key = SigningKey::from_bytes(&[7u8; 32]);
        let attestor_id = hash(&env, 60);
        let public_key = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
        client.set_attestor(&operator, &attestor_id, &public_key, &true);

        let availability_root = hash(&env, 61);
        let scope_hash = hash(&env, 62);
        let issued_at_ledger = 100u32;
        let attestation_expiry = 220u32;
        let digest = client.build_attestation_hash(
            &attestor_id,
            &participant_id_hash,
            &asset,
            &availability_root,
            &scope_hash,
            &issued_at_ledger,
            &attestation_expiry,
        );
        let signature = sign_attestation_hash(&env, &signing_key, &digest);
        let attestation = client.publish_attestation(
            &attestor_id,
            &participant_id_hash,
            &asset,
            &availability_root,
            &scope_hash,
            &issued_at_ledger,
            &attestation_expiry,
            &signature,
        );

        let proof_receipt = create_proof_receipt(
            &env,
            &collateral_policy_id,
            &proof_gateway_id,
            &submitter,
            &participant_id_hash,
            &verifier_id,
            &availability_root,
            70,
            200,
        );
        let lot_nullifier = hash(&env, 71);
        let reason_hash = hash(&env, 72);
        let lock = client.lock_lot(
            &submitter,
            &participant_id_hash,
            &asset,
            &attestation.attestation_id,
            &proof_receipt.receipt_id,
            &lot_nullifier,
            &scope_hash,
            &reason_hash,
            &500_000i128,
            &190u32,
        );

        assert_eq!(lock.lot_nullifier, lot_nullifier);
        assert_eq!(lock.attestation_id, attestation.attestation_id);
        assert_eq!(lock.proof_receipt_id, proof_receipt.receipt_id);
        assert_eq!(lock.availability_root, availability_root);
        assert_eq!(client.get_attestation(&attestation.attestation_id), attestation);
        assert_eq!(client.get_lock(&lot_nullifier), lock);
        assert!(client.is_lot_locked(&lot_nullifier));
        assert!(!client.is_lot_released(&lot_nullifier));
    }

    #[test]
    fn rejects_invalid_attestor_signature() {
        let env = Env::default();
        let (
            admin,
            operator,
            _submitter,
            asset,
            asset_registry_id,
            participant_registry_id,
            _collateral_policy_id,
            proof_gateway_id,
            participant_id_hash,
            _verifier_id,
        ) = setup_phase_three(&env);

        let contract_id = env.register(
            EncumbranceRegistry,
            EncumbranceRegistryArgs::__constructor(
                &admin,
                &participant_registry_id,
                &asset_registry_id,
                &proof_gateway_id,
            ),
        );
        let client = EncumbranceRegistryClient::new(&env, &contract_id);
        client.set_operator(&admin, &operator, &true);

        let signing_key = SigningKey::from_bytes(&[8u8; 32]);
        let attestor_id = hash(&env, 80);
        let public_key = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
        client.set_attestor(&operator, &attestor_id, &public_key, &true);

        let result = env.try_invoke_contract::<AvailabilityAttestation, EncumbranceRegistryError>(
            &contract_id,
            &Symbol::new(&env, "publish_attestation"),
            vec![
                &env,
                attestor_id.into_val(&env),
                participant_id_hash.into_val(&env),
                asset.into_val(&env),
                hash(&env, 81).into_val(&env),
                hash(&env, 82).into_val(&env),
                100u32.into_val(&env),
                200u32.into_val(&env),
                BytesN::from_array(&env, &[9u8; 64]).into_val(&env),
            ],
        );

        assert!(matches!(result, Err(Err(_))));
    }

    #[test]
    fn rejects_reused_lot_nullifier() {
        let env = Env::default();
        let (
            admin,
            operator,
            submitter,
            asset,
            asset_registry_id,
            participant_registry_id,
            collateral_policy_id,
            proof_gateway_id,
            participant_id_hash,
            verifier_id,
        ) = setup_phase_three(&env);

        let contract_id = env.register(
            EncumbranceRegistry,
            EncumbranceRegistryArgs::__constructor(
                &admin,
                &participant_registry_id,
                &asset_registry_id,
                &proof_gateway_id,
            ),
        );
        let client = EncumbranceRegistryClient::new(&env, &contract_id);
        client.set_operator(&admin, &operator, &true);

        let signing_key = SigningKey::from_bytes(&[10u8; 32]);
        let attestor_id = hash(&env, 90);
        let public_key = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
        client.set_attestor(&operator, &attestor_id, &public_key, &true);

        let availability_root = hash(&env, 91);
        let scope_hash = hash(&env, 92);
        let digest = client.build_attestation_hash(
            &attestor_id,
            &participant_id_hash,
            &asset,
            &availability_root,
            &scope_hash,
            &100u32,
            &220u32,
        );
        let signature = sign_attestation_hash(&env, &signing_key, &digest);
        let attestation = client.publish_attestation(
            &attestor_id,
            &participant_id_hash,
            &asset,
            &availability_root,
            &scope_hash,
            &100u32,
            &220u32,
            &signature,
        );

        let proof_receipt = create_proof_receipt(
            &env,
            &collateral_policy_id,
            &proof_gateway_id,
            &submitter,
            &participant_id_hash,
            &verifier_id,
            &availability_root,
            93,
            205,
        );
        let lot_nullifier = hash(&env, 94);
        client.lock_lot(
            &submitter,
            &participant_id_hash,
            &asset,
            &attestation.attestation_id,
            &proof_receipt.receipt_id,
            &lot_nullifier,
            &scope_hash,
            &hash(&env, 95),
            &10i128,
            &190u32,
        );

        let result = env.try_invoke_contract::<EncumbranceLock, EncumbranceRegistryError>(
            &contract_id,
            &Symbol::new(&env, "lock_lot"),
            vec![
                &env,
                submitter.into_val(&env),
                participant_id_hash.into_val(&env),
                asset.into_val(&env),
                attestation.attestation_id.into_val(&env),
                proof_receipt.receipt_id.into_val(&env),
                lot_nullifier.into_val(&env),
                scope_hash.into_val(&env),
                hash(&env, 96).into_val(&env),
                10i128.into_val(&env),
                191u32.into_val(&env),
            ],
        );

        assert!(matches!(result, Err(Ok(EncumbranceRegistryError::LockExists))));
    }

    #[test]
    fn releases_lot_and_marks_released() {
        let env = Env::default();
        let (
            admin,
            operator,
            submitter,
            asset,
            asset_registry_id,
            participant_registry_id,
            collateral_policy_id,
            proof_gateway_id,
            participant_id_hash,
            verifier_id,
        ) = setup_phase_three(&env);

        let contract_id = env.register(
            EncumbranceRegistry,
            EncumbranceRegistryArgs::__constructor(
                &admin,
                &participant_registry_id,
                &asset_registry_id,
                &proof_gateway_id,
            ),
        );
        let client = EncumbranceRegistryClient::new(&env, &contract_id);
        client.set_operator(&admin, &operator, &true);

        let signing_key = SigningKey::from_bytes(&[11u8; 32]);
        let attestor_id = hash(&env, 100);
        let public_key = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
        client.set_attestor(&operator, &attestor_id, &public_key, &true);

        let availability_root = hash(&env, 101);
        let scope_hash = hash(&env, 102);
        let digest = client.build_attestation_hash(
            &attestor_id,
            &participant_id_hash,
            &asset,
            &availability_root,
            &scope_hash,
            &100u32,
            &240u32,
        );
        let signature = sign_attestation_hash(&env, &signing_key, &digest);
        let attestation = client.publish_attestation(
            &attestor_id,
            &participant_id_hash,
            &asset,
            &availability_root,
            &scope_hash,
            &100u32,
            &240u32,
            &signature,
        );

        let proof_receipt = create_proof_receipt(
            &env,
            &collateral_policy_id,
            &proof_gateway_id,
            &submitter,
            &participant_id_hash,
            &verifier_id,
            &availability_root,
            103,
            230,
        );
        let lot_nullifier = hash(&env, 104);
        client.lock_lot(
            &submitter,
            &participant_id_hash,
            &asset,
            &attestation.attestation_id,
            &proof_receipt.receipt_id,
            &lot_nullifier,
            &scope_hash,
            &hash(&env, 105),
            &15i128,
            &210u32,
        );

        let released = client.release_lot(&submitter, &lot_nullifier, &hash(&env, 106));
        assert!(released.released);
        assert_eq!(released.release_reference, hash(&env, 106));
        assert!(client.is_lot_released(&lot_nullifier));
        assert!(client.get_lock(&lot_nullifier).released);
    }

    #[test]
    fn sweeps_expired_lot() {
        let env = Env::default();
        let (
            admin,
            operator,
            submitter,
            asset,
            asset_registry_id,
            participant_registry_id,
            collateral_policy_id,
            proof_gateway_id,
            participant_id_hash,
            verifier_id,
        ) = setup_phase_three(&env);

        let contract_id = env.register(
            EncumbranceRegistry,
            EncumbranceRegistryArgs::__constructor(
                &admin,
                &participant_registry_id,
                &asset_registry_id,
                &proof_gateway_id,
            ),
        );
        let client = EncumbranceRegistryClient::new(&env, &contract_id);
        client.set_operator(&admin, &operator, &true);

        let signing_key = SigningKey::from_bytes(&[12u8; 32]);
        let attestor_id = hash(&env, 110);
        let public_key = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
        client.set_attestor(&operator, &attestor_id, &public_key, &true);

        let availability_root = hash(&env, 111);
        let scope_hash = hash(&env, 112);
        let digest = client.build_attestation_hash(
            &attestor_id,
            &participant_id_hash,
            &asset,
            &availability_root,
            &scope_hash,
            &100u32,
            &180u32,
        );
        let signature = sign_attestation_hash(&env, &signing_key, &digest);
        let attestation = client.publish_attestation(
            &attestor_id,
            &participant_id_hash,
            &asset,
            &availability_root,
            &scope_hash,
            &100u32,
            &180u32,
            &signature,
        );

        let proof_receipt = create_proof_receipt(
            &env,
            &collateral_policy_id,
            &proof_gateway_id,
            &submitter,
            &participant_id_hash,
            &verifier_id,
            &availability_root,
            113,
            170,
        );
        let lot_nullifier = hash(&env, 114);
        client.lock_lot(
            &submitter,
            &participant_id_hash,
            &asset,
            &attestation.attestation_id,
            &proof_receipt.receipt_id,
            &lot_nullifier,
            &scope_hash,
            &hash(&env, 115),
            &25i128,
            &140u32,
        );

        env.ledger().set_sequence_number(141);
        let released = client.sweep_expired_lock(&operator, &lot_nullifier, &hash(&env, 116));
        assert!(released.released);
        assert_eq!(released.release_reference, hash(&env, 116));
        assert!(client.is_lot_released(&lot_nullifier));
    }
}
