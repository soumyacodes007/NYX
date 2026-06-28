#![no_std]

use compliance_control::ComplianceControlClient;
use participant_registry::ParticipantRegistryClient;
use proof_gateway::ProofGatewayClient;
use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, Bytes, BytesN,
    Env,
};
use zkdtcc_types::{OrderCommitmentRecord, OrderSide, OrderStatus, PrivateMatchExecution, ProofReceipt, ProofType};

const INSTANCE_BUMP_THRESHOLD: u32 = 17_280;
const INSTANCE_BUMP_TO: u32 = 518_400;
const PERSISTENT_BUMP_THRESHOLD: u32 = 17_280;
const PERSISTENT_BUMP_TO: u32 = 518_400;
const ORDER_DOMAIN: &[u8] = b"zkdtcc:order-commit:v1";
const EXECUTION_DOMAIN: &[u8] = b"zkdtcc:private-match:v1";

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Operator(Address),
    Matcher(Address),
    ParticipantRegistry,
    ProofGateway,
    ComplianceControl,
    Order(BytesN<32>),
    CancelNullifier(BytesN<32>),
    ExecutionNullifier(BytesN<32>),
    Execution(BytesN<32>),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum OrderCommitPoolError {
    Unauthorized = 1,
    ParticipantMismatch = 2,
    InvalidExpiry = 3,
    OrderExists = 4,
    OrderNotFound = 5,
    OrderNotActive = 6,
    OrderExpired = 7,
    CancelNullifierMismatch = 8,
    CancelNullifierUsed = 9,
    ExecutionNullifierUsed = 10,
    ProofReceiptNotFound = 11,
    WrongProofType = 12,
    ProofParticipantMismatch = 13,
    ProofSubmitterMismatch = 14,
    ProofExpired = 15,
    BatchMismatch = 16,
    InstrumentMismatch = 17,
    InvalidOrderSides = 18,
    SelfTrade = 19,
    MatcherDisabled = 20,
    ExecutionExists = 21,
    ProofVerifierMismatch = 22,
    ExecutionCommitmentMismatch = 23,
    ProtocolPaused = 24,
    ParticipantFrozen = 25,
    ProofReceiptNotUsable = 26,
}

#[contractevent(topics = ["operator_set"])]
pub struct OperatorSetEvent {
    pub operator: Address,
    pub enabled: bool,
}

#[contractevent(topics = ["matcher_set"])]
pub struct MatcherSetEvent {
    pub matcher: Address,
    pub enabled: bool,
}

#[contractevent(topics = ["order_committed"])]
pub struct OrderCommittedEvent {
    pub order_id: BytesN<32>,
    pub participant_id_hash: BytesN<32>,
    pub batch_id: BytesN<32>,
}

#[contractevent(topics = ["order_cancelled"])]
pub struct OrderCancelledEvent {
    pub order_id: BytesN<32>,
    pub cancel_nullifier: BytesN<32>,
}

#[contractevent(topics = ["order_expired"])]
pub struct OrderExpiredEvent {
    pub order_id: BytesN<32>,
}

#[contractevent(topics = ["private_match_recorded"])]
pub struct PrivateMatchRecordedEvent {
    pub execution_id: BytesN<32>,
    pub batch_id: BytesN<32>,
    pub proof_receipt_id: BytesN<32>,
}

#[contract]
pub struct OrderCommitPool;

#[contractimpl]
impl OrderCommitPool {
    pub fn __constructor(
        env: Env,
        admin: Address,
        participant_registry: Address,
        proof_gateway: Address,
        compliance_control: Address,
    ) {
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::ParticipantRegistry, &participant_registry);
        env.storage().instance().set(&DataKey::ProofGateway, &proof_gateway);
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
    ) -> Result<(), OrderCommitPoolError> {
        require_admin_auth(&env, &admin)?;
        let key = DataKey::Operator(operator.clone());
        env.storage().persistent().set(&key, &enabled);
        bump_persistent(&env, &key);
        bump_instance(&env);
        OperatorSetEvent { operator, enabled }.publish(&env);
        Ok(())
    }

    pub fn set_matcher(
        env: Env,
        operator: Address,
        matcher: Address,
        enabled: bool,
    ) -> Result<(), OrderCommitPoolError> {
        require_operator_auth(&env, &operator)?;
        let key = DataKey::Matcher(matcher.clone());
        env.storage().persistent().set(&key, &enabled);
        bump_persistent(&env, &key);
        bump_instance(&env);
        MatcherSetEvent { matcher, enabled }.publish(&env);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build_order_id(
        env: Env,
        submitter: Address,
        participant_id_hash: BytesN<32>,
        instrument_id_hash: BytesN<32>,
        batch_id: BytesN<32>,
        side: OrderSide,
        order_commitment: BytesN<32>,
        cancel_nullifier: BytesN<32>,
        expiry_ledger: u32,
    ) -> BytesN<32> {
        derive_order_id(
            &env,
            &submitter,
            &participant_id_hash,
            &instrument_id_hash,
            &batch_id,
            &side,
            &order_commitment,
            &cancel_nullifier,
            expiry_ledger,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn commit_order(
        env: Env,
        submitter: Address,
        participant_id_hash: BytesN<32>,
        instrument_id_hash: BytesN<32>,
        batch_id: BytesN<32>,
        side: OrderSide,
        order_commitment: BytesN<32>,
        collateral_proof_receipt_id: BytesN<32>,
        encumbrance_proof_receipt_id: BytesN<32>,
        cancel_nullifier: BytesN<32>,
        expiry_ledger: u32,
    ) -> Result<OrderCommitmentRecord, OrderCommitPoolError> {
        submitter.require_auth();
        ensure_protocol_live(&env)?;
        ensure_participant_binding(&env, &submitter, &participant_id_hash)?;
        ensure_participant_not_frozen(&env, &participant_id_hash)?;
        if env.ledger().sequence() > expiry_ledger {
            return Err(OrderCommitPoolError::InvalidExpiry);
        }

        let collateral_receipt = load_proof_receipt(&env, &collateral_proof_receipt_id)?;
        ensure_receipt_binding(
            &env,
            &collateral_receipt,
            &participant_id_hash,
            &submitter,
            ProofType::CollateralSufficiency,
        )?;
        let encumbrance_receipt = load_proof_receipt(&env, &encumbrance_proof_receipt_id)?;
        ensure_receipt_binding(
            &env,
            &encumbrance_receipt,
            &participant_id_hash,
            &submitter,
            ProofType::UnencumberedLot,
        )?;

        let order_id = derive_order_id(
            &env,
            &submitter,
            &participant_id_hash,
            &instrument_id_hash,
            &batch_id,
            &side,
            &order_commitment,
            &cancel_nullifier,
            expiry_ledger,
        );
        let key = DataKey::Order(order_id.clone());
        if env.storage().persistent().has(&key) {
            return Err(OrderCommitPoolError::OrderExists);
        }

        let record = OrderCommitmentRecord {
            order_id: order_id.clone(),
            participant_id_hash: participant_id_hash.clone(),
            submitter: submitter.clone(),
            instrument_id_hash,
            batch_id: batch_id.clone(),
            side,
            order_commitment,
            collateral_proof_receipt_id,
            encumbrance_proof_receipt_id,
            expiry_ledger,
            status: OrderStatus::Active,
            cancel_nullifier,
            matched_execution_id: zero_hash(&env),
            created_ledger: env.ledger().sequence(),
            updated_ledger: env.ledger().sequence(),
        };

        env.storage().persistent().set(&key, &record);
        bump_persistent(&env, &key);
        bump_instance(&env);

        OrderCommittedEvent {
            order_id,
            participant_id_hash,
            batch_id,
        }
        .publish(&env);

        Ok(record)
    }

    pub fn cancel_order(
        env: Env,
        submitter: Address,
        order_id: BytesN<32>,
        cancel_nullifier: BytesN<32>,
    ) -> Result<OrderCommitmentRecord, OrderCommitPoolError> {
        submitter.require_auth();
        let mut order = load_order(&env, &order_id)?;
        if order.submitter != submitter {
            return Err(OrderCommitPoolError::Unauthorized);
        }
        if order.status != OrderStatus::Active {
            return Err(OrderCommitPoolError::OrderNotActive);
        }
        if env.ledger().sequence() > order.expiry_ledger {
            return Err(OrderCommitPoolError::OrderExpired);
        }
        if order.cancel_nullifier != cancel_nullifier {
            return Err(OrderCommitPoolError::CancelNullifierMismatch);
        }

        let nullifier_key = DataKey::CancelNullifier(cancel_nullifier.clone());
        if env.storage().persistent().has(&nullifier_key) {
            return Err(OrderCommitPoolError::CancelNullifierUsed);
        }
        env.storage().persistent().set(&nullifier_key, &order_id);
        bump_persistent(&env, &nullifier_key);

        order.status = OrderStatus::Cancelled;
        order.updated_ledger = env.ledger().sequence();
        save_order(&env, &order);
        bump_instance(&env);

        OrderCancelledEvent {
            order_id,
            cancel_nullifier,
        }
        .publish(&env);

        Ok(order)
    }

    pub fn expire_order(env: Env, order_id: BytesN<32>) -> Result<OrderCommitmentRecord, OrderCommitPoolError> {
        let mut order = load_order(&env, &order_id)?;
        if order.status != OrderStatus::Active {
            return Err(OrderCommitPoolError::OrderNotActive);
        }
        if env.ledger().sequence() <= order.expiry_ledger {
            return Err(OrderCommitPoolError::InvalidExpiry);
        }

        order.status = OrderStatus::Expired;
        order.updated_ledger = env.ledger().sequence();
        save_order(&env, &order);
        bump_instance(&env);
        OrderExpiredEvent { order_id }.publish(&env);
        Ok(order)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn match_orders(
        env: Env,
        matcher: Address,
        verifier_id: BytesN<32>,
        proof_receipt_id: BytesN<32>,
        bid_order_id: BytesN<32>,
        ask_order_id: BytesN<32>,
        execution_commitment: BytesN<32>,
        encrypted_receipt_hash: BytesN<32>,
        bid_execution_nullifier: BytesN<32>,
        ask_execution_nullifier: BytesN<32>,
    ) -> Result<PrivateMatchExecution, OrderCommitPoolError> {
        matcher.require_auth();
        ensure_protocol_live(&env)?;
        ensure_matcher_enabled(&env, &matcher)?;

        let mut bid_order = load_order(&env, &bid_order_id)?;
        let mut ask_order = load_order(&env, &ask_order_id)?;
        ensure_participant_not_frozen(&env, &bid_order.participant_id_hash)?;
        ensure_participant_not_frozen(&env, &ask_order.participant_id_hash)?;
        ensure_matchable_order(&env, &bid_order)?;
        ensure_matchable_order(&env, &ask_order)?;
        if bid_order.side != OrderSide::Bid || ask_order.side != OrderSide::Ask {
            return Err(OrderCommitPoolError::InvalidOrderSides);
        }
        if bid_order.batch_id != ask_order.batch_id {
            return Err(OrderCommitPoolError::BatchMismatch);
        }
        if bid_order.instrument_id_hash != ask_order.instrument_id_hash {
            return Err(OrderCommitPoolError::InstrumentMismatch);
        }
        if bid_order.participant_id_hash == ask_order.participant_id_hash {
            return Err(OrderCommitPoolError::SelfTrade);
        }

        let bid_nullifier_key = DataKey::ExecutionNullifier(bid_execution_nullifier.clone());
        if env.storage().persistent().has(&bid_nullifier_key) {
            return Err(OrderCommitPoolError::ExecutionNullifierUsed);
        }
        let ask_nullifier_key = DataKey::ExecutionNullifier(ask_execution_nullifier.clone());
        if env.storage().persistent().has(&ask_nullifier_key) {
            return Err(OrderCommitPoolError::ExecutionNullifierUsed);
        }

        let proof_receipt = load_proof_receipt(&env, &proof_receipt_id)?;
        if proof_receipt.proof_type != ProofType::PrivateMatch {
            return Err(OrderCommitPoolError::WrongProofType);
        }
        if proof_receipt.submitter != matcher {
            return Err(OrderCommitPoolError::ProofSubmitterMismatch);
        }
        if proof_receipt.verifier_id != verifier_id {
            return Err(OrderCommitPoolError::ProofVerifierMismatch);
        }
        if proof_receipt.portfolio_commitment != execution_commitment {
            return Err(OrderCommitPoolError::ExecutionCommitmentMismatch);
        }
        if env.ledger().sequence() > proof_receipt.expiry_ledger {
            return Err(OrderCommitPoolError::ProofExpired);
        }

        let execution_id = derive_execution_id(
            &env,
            &bid_order_id,
            &ask_order_id,
            &execution_commitment,
            &bid_execution_nullifier,
            &ask_execution_nullifier,
        );
        let execution_key = DataKey::Execution(execution_id.clone());
        if env.storage().persistent().has(&execution_key) {
            return Err(OrderCommitPoolError::ExecutionExists);
        }

        let execution = PrivateMatchExecution {
            execution_id: execution_id.clone(),
            batch_id: bid_order.batch_id.clone(),
            bid_order_id: bid_order_id.clone(),
            ask_order_id: ask_order_id.clone(),
            matcher,
            verifier_id,
            proof_receipt_id: proof_receipt_id.clone(),
            execution_commitment,
            encrypted_receipt_hash,
            bid_execution_nullifier: bid_execution_nullifier.clone(),
            ask_execution_nullifier: ask_execution_nullifier.clone(),
            recorded_ledger: env.ledger().sequence(),
        };

        bid_order.status = OrderStatus::Matched;
        bid_order.matched_execution_id = execution_id.clone();
        bid_order.updated_ledger = env.ledger().sequence();
        ask_order.status = OrderStatus::Matched;
        ask_order.matched_execution_id = execution_id.clone();
        ask_order.updated_ledger = env.ledger().sequence();

        save_order(&env, &bid_order);
        save_order(&env, &ask_order);
        env.storage().persistent().set(&bid_nullifier_key, &execution_id);
        env.storage().persistent().set(&ask_nullifier_key, &execution_id);
        env.storage().persistent().set(&execution_key, &execution);
        bump_persistent(&env, &bid_nullifier_key);
        bump_persistent(&env, &ask_nullifier_key);
        bump_persistent(&env, &execution_key);
        bump_instance(&env);

        PrivateMatchRecordedEvent {
            execution_id,
            batch_id: bid_order.batch_id,
            proof_receipt_id,
        }
        .publish(&env);

        Ok(execution)
    }

    pub fn get_order(env: Env, order_id: BytesN<32>) -> Result<OrderCommitmentRecord, OrderCommitPoolError> {
        load_order(&env, &order_id)
    }

    pub fn get_execution(
        env: Env,
        execution_id: BytesN<32>,
    ) -> Result<PrivateMatchExecution, OrderCommitPoolError> {
        let key = DataKey::Execution(execution_id);
        let execution = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(OrderCommitPoolError::OrderNotFound)?;
        bump_persistent(&env, &key);
        Ok(execution)
    }

    pub fn has_order(env: Env, order_id: BytesN<32>) -> bool {
        env.storage().persistent().has(&DataKey::Order(order_id))
    }

    pub fn is_cancel_nullifier_used(env: Env, cancel_nullifier: BytesN<32>) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::CancelNullifier(cancel_nullifier))
    }

    pub fn is_execution_nullifier_used(env: Env, execution_nullifier: BytesN<32>) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::ExecutionNullifier(execution_nullifier))
    }

    pub fn has_execution(env: Env, execution_id: BytesN<32>) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::Execution(execution_id))
    }
}

fn ensure_participant_binding(
    env: &Env,
    submitter: &Address,
    participant_id_hash: &BytesN<32>,
) -> Result<(), OrderCommitPoolError> {
    let registry_id: Address = env.storage().instance().get(&DataKey::ParticipantRegistry).unwrap();
    let registry = ParticipantRegistryClient::new(env, &registry_id);
    let owner = registry.wallet_owner(submitter);
    if &owner != participant_id_hash {
        return Err(OrderCommitPoolError::ParticipantMismatch);
    }
    Ok(())
}

fn ensure_receipt_binding(
    env: &Env,
    proof_receipt: &ProofReceipt,
    participant_id_hash: &BytesN<32>,
    submitter: &Address,
    proof_type: ProofType,
) -> Result<(), OrderCommitPoolError> {
    if proof_receipt.proof_type != proof_type {
        return Err(OrderCommitPoolError::WrongProofType);
    }
    if &proof_receipt.participant_id_hash != participant_id_hash {
        return Err(OrderCommitPoolError::ProofParticipantMismatch);
    }
    if &proof_receipt.submitter != submitter {
        return Err(OrderCommitPoolError::ProofSubmitterMismatch);
    }
    let proof_gateway: Address = env.storage().instance().get(&DataKey::ProofGateway).unwrap();
    let gateway = ProofGatewayClient::new(env, &proof_gateway);
    if !gateway.is_receipt_usable(&proof_receipt.receipt_id) {
        return Err(OrderCommitPoolError::ProofReceiptNotUsable);
    }
    if env.ledger().sequence() > proof_receipt.expiry_ledger {
        return Err(OrderCommitPoolError::ProofExpired);
    }
    Ok(())
}

fn ensure_matchable_order(env: &Env, order: &OrderCommitmentRecord) -> Result<(), OrderCommitPoolError> {
    if order.status != OrderStatus::Active {
        return Err(OrderCommitPoolError::OrderNotActive);
    }
    if env.ledger().sequence() > order.expiry_ledger {
        return Err(OrderCommitPoolError::OrderExpired);
    }
    Ok(())
}

fn ensure_matcher_enabled(env: &Env, matcher: &Address) -> Result<(), OrderCommitPoolError> {
    let key = DataKey::Matcher(matcher.clone());
    let enabled = env.storage().persistent().get(&key).unwrap_or(false);
    if !enabled {
        return Err(OrderCommitPoolError::MatcherDisabled);
    }
    bump_persistent(env, &key);
    Ok(())
}

fn load_proof_receipt(env: &Env, proof_receipt_id: &BytesN<32>) -> Result<ProofReceipt, OrderCommitPoolError> {
    let proof_gateway: Address = env.storage().instance().get(&DataKey::ProofGateway).unwrap();
    let gateway = ProofGatewayClient::new(env, &proof_gateway);
    if !gateway.has_receipt(proof_receipt_id) {
        return Err(OrderCommitPoolError::ProofReceiptNotFound);
    }
    Ok(gateway.get_receipt(proof_receipt_id))
}

fn ensure_protocol_live(env: &Env) -> Result<(), OrderCommitPoolError> {
    let compliance_control: Address = env.storage().instance().get(&DataKey::ComplianceControl).unwrap();
    let compliance = ComplianceControlClient::new(env, &compliance_control);
    if compliance.is_globally_paused() {
        return Err(OrderCommitPoolError::ProtocolPaused);
    }
    Ok(())
}

fn ensure_participant_not_frozen(
    env: &Env,
    participant_id_hash: &BytesN<32>,
) -> Result<(), OrderCommitPoolError> {
    let compliance_control: Address = env.storage().instance().get(&DataKey::ComplianceControl).unwrap();
    let compliance = ComplianceControlClient::new(env, &compliance_control);
    if compliance.is_participant_frozen(participant_id_hash) {
        return Err(OrderCommitPoolError::ParticipantFrozen);
    }
    Ok(())
}

fn load_order(env: &Env, order_id: &BytesN<32>) -> Result<OrderCommitmentRecord, OrderCommitPoolError> {
    let key = DataKey::Order(order_id.clone());
    let order = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(OrderCommitPoolError::OrderNotFound)?;
    bump_persistent(env, &key);
    Ok(order)
}

fn save_order(env: &Env, order: &OrderCommitmentRecord) {
    let key = DataKey::Order(order.order_id.clone());
    env.storage().persistent().set(&key, order);
    bump_persistent(env, &key);
}

#[allow(clippy::too_many_arguments)]
fn derive_order_id(
    env: &Env,
    submitter: &Address,
    participant_id_hash: &BytesN<32>,
    instrument_id_hash: &BytesN<32>,
    batch_id: &BytesN<32>,
    side: &OrderSide,
    order_commitment: &BytesN<32>,
    cancel_nullifier: &BytesN<32>,
    expiry_ledger: u32,
) -> BytesN<32> {
    let mut material = Bytes::new(env);
    material.extend_from_slice(ORDER_DOMAIN);
    append_address(&mut material, submitter);
    append_address(&mut material, &env.current_contract_address());
    material.extend_from_slice(&participant_id_hash.to_array());
    material.extend_from_slice(&instrument_id_hash.to_array());
    material.extend_from_slice(&batch_id.to_array());
    material.extend_from_slice(&order_side_code(side).to_be_bytes());
    material.extend_from_slice(&order_commitment.to_array());
    material.extend_from_slice(&cancel_nullifier.to_array());
    material.extend_from_slice(&expiry_ledger.to_be_bytes());
    env.crypto().sha256(&material).into()
}

fn derive_execution_id(
    env: &Env,
    bid_order_id: &BytesN<32>,
    ask_order_id: &BytesN<32>,
    execution_commitment: &BytesN<32>,
    bid_execution_nullifier: &BytesN<32>,
    ask_execution_nullifier: &BytesN<32>,
) -> BytesN<32> {
    let mut material = Bytes::new(env);
    material.extend_from_slice(EXECUTION_DOMAIN);
    append_address(&mut material, &env.current_contract_address());
    material.extend_from_slice(&bid_order_id.to_array());
    material.extend_from_slice(&ask_order_id.to_array());
    material.extend_from_slice(&execution_commitment.to_array());
    material.extend_from_slice(&bid_execution_nullifier.to_array());
    material.extend_from_slice(&ask_execution_nullifier.to_array());
    env.crypto().sha256(&material).into()
}

fn append_address(material: &mut Bytes, address: &Address) {
    let address_bytes = address.to_string().to_bytes();
    material.extend_from_slice(&address_bytes.len().to_be_bytes());
    material.append(&address_bytes);
}

fn order_side_code(side: &OrderSide) -> u32 {
    match side {
        OrderSide::Bid => 1,
        OrderSide::Ask => 2,
    }
}

fn zero_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0; 32])
}

fn require_admin_auth(env: &Env, admin: &Address) -> Result<(), OrderCommitPoolError> {
    admin.require_auth();
    let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    if stored_admin != *admin {
        return Err(OrderCommitPoolError::Unauthorized);
    }
    Ok(())
}

fn require_operator_auth(env: &Env, operator: &Address) -> Result<(), OrderCommitPoolError> {
    operator.require_auth();
    let key = DataKey::Operator(operator.clone());
    let enabled = env.storage().persistent().get(&key).unwrap_or(false);
    if !enabled {
        return Err(OrderCommitPoolError::Unauthorized);
    }
    bump_persistent(env, &key);
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
mod tests;
