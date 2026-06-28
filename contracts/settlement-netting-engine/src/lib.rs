#![no_std]

use order_commit_pool::OrderCommitPoolClient;
use participant_registry::ParticipantRegistryClient;
use proof_gateway::ProofGatewayClient;
use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, Bytes, BytesN,
    Env,
};
use zkdtcc_types::{ParticipantRole, ParticipantStatus, ProofReceipt, ProofType, SettlementBatchRecord};

const INSTANCE_BUMP_THRESHOLD: u32 = 17_280;
const INSTANCE_BUMP_TO: u32 = 518_400;
const PERSISTENT_BUMP_THRESHOLD: u32 = 17_280;
const PERSISTENT_BUMP_TO: u32 = 518_400;
const SETTLEMENT_DOMAIN: &[u8] = b"zkdtcc:settlement-batch:v1";

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Operator(Address),
    Settler(Address),
    ParticipantRegistry,
    ProofGateway,
    OrderCommitPool,
    TradeNullifier(BytesN<32>),
    SettledExecution(BytesN<32>),
    Batch(BytesN<32>),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum SettlementNettingEngineError {
    Unauthorized = 1,
    SettlerDisabled = 2,
    ParticipantMismatch = 3,
    WrongParticipantRole = 4,
    ParticipantNotActive = 5,
    ProofReceiptNotFound = 6,
    WrongProofType = 7,
    ProofSubmitterMismatch = 8,
    ProofVerifierMismatch = 9,
    ProofExpired = 10,
    SettlementCommitmentMismatch = 11,
    ExecutionNotFound = 12,
    OrderNotFound = 13,
    BatchMismatch = 14,
    InstrumentMismatch = 15,
    ExecutionAlreadySettled = 16,
    TradeNullifierUsed = 17,
    SettlementExists = 18,
    DuplicateExecution = 19,
    DuplicateTradeNullifier = 20,
    BatchNotFound = 21,
}

#[contractevent(topics = ["operator_set"])]
pub struct OperatorSetEvent {
    pub operator: Address,
    pub enabled: bool,
}

#[contractevent(topics = ["settler_set"])]
pub struct SettlerSetEvent {
    pub settler: Address,
    pub enabled: bool,
}

#[contractevent(topics = ["batch_settled"])]
pub struct BatchSettledEvent {
    pub settlement_id: BytesN<32>,
    pub batch_id: BytesN<32>,
    pub proof_receipt_id: BytesN<32>,
}

#[contract]
pub struct SettlementNettingEngine;

#[contractimpl]
impl SettlementNettingEngine {
    pub fn __constructor(
        env: Env,
        admin: Address,
        participant_registry: Address,
        proof_gateway: Address,
        order_commit_pool: Address,
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
            .set(&DataKey::OrderCommitPool, &order_commit_pool);
        bump_instance(&env);
    }

    pub fn set_operator(
        env: Env,
        admin: Address,
        operator: Address,
        enabled: bool,
    ) -> Result<(), SettlementNettingEngineError> {
        require_admin_auth(&env, &admin)?;
        let key = DataKey::Operator(operator.clone());
        env.storage().persistent().set(&key, &enabled);
        bump_persistent(&env, &key);
        bump_instance(&env);
        OperatorSetEvent { operator, enabled }.publish(&env);
        Ok(())
    }

    pub fn set_settler(
        env: Env,
        operator: Address,
        settler: Address,
        enabled: bool,
    ) -> Result<(), SettlementNettingEngineError> {
        require_operator_auth(&env, &operator)?;
        let key = DataKey::Settler(settler.clone());
        env.storage().persistent().set(&key, &enabled);
        bump_persistent(&env, &key);
        bump_instance(&env);
        SettlerSetEvent { settler, enabled }.publish(&env);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build_settlement_id(
        env: Env,
        proof_receipt_id: BytesN<32>,
        settlement_commitment: BytesN<32>,
        execution_a_id: BytesN<32>,
        execution_b_id: BytesN<32>,
        trade_nullifier_a: BytesN<32>,
        trade_nullifier_b: BytesN<32>,
    ) -> BytesN<32> {
        derive_settlement_id(
            &env,
            &proof_receipt_id,
            &settlement_commitment,
            &execution_a_id,
            &execution_b_id,
            &trade_nullifier_a,
            &trade_nullifier_b,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn settle_batch(
        env: Env,
        settler: Address,
        verifier_id: BytesN<32>,
        proof_receipt_id: BytesN<32>,
        settlement_commitment: BytesN<32>,
        net_vector_hash: BytesN<32>,
        execution_a_id: BytesN<32>,
        execution_b_id: BytesN<32>,
        trade_nullifier_a: BytesN<32>,
        trade_nullifier_b: BytesN<32>,
    ) -> Result<SettlementBatchRecord, SettlementNettingEngineError> {
        settler.require_auth();
        ensure_settler_enabled(&env, &settler)?;

        if execution_a_id == execution_b_id {
            return Err(SettlementNettingEngineError::DuplicateExecution);
        }
        if trade_nullifier_a == trade_nullifier_b {
            return Err(SettlementNettingEngineError::DuplicateTradeNullifier);
        }

        let settler_participant_id_hash = load_settler_participant_id(&env, &settler)?;
        let proof_receipt = load_proof_receipt(&env, &proof_receipt_id)?;
        ensure_batch_receipt(
            &env,
            &proof_receipt,
            &settler,
            &settler_participant_id_hash,
            &verifier_id,
            &settlement_commitment,
        )?;

        let order_pool_id: Address = env.storage().instance().get(&DataKey::OrderCommitPool).unwrap();
        let order_pool = OrderCommitPoolClient::new(&env, &order_pool_id);
        if !order_pool.has_execution(&execution_a_id) || !order_pool.has_execution(&execution_b_id) {
            return Err(SettlementNettingEngineError::ExecutionNotFound);
        }

        let execution_a = order_pool.get_execution(&execution_a_id);
        let execution_b = order_pool.get_execution(&execution_b_id);
        if is_execution_settled_internal(&env, &execution_a_id)
            || is_execution_settled_internal(&env, &execution_b_id)
        {
            return Err(SettlementNettingEngineError::ExecutionAlreadySettled);
        }

        let trade_nullifier_key_a = DataKey::TradeNullifier(trade_nullifier_a.clone());
        let trade_nullifier_key_b = DataKey::TradeNullifier(trade_nullifier_b.clone());
        if env.storage().persistent().has(&trade_nullifier_key_a)
            || env.storage().persistent().has(&trade_nullifier_key_b)
        {
            return Err(SettlementNettingEngineError::TradeNullifierUsed);
        }

        let a_bid_order = load_order(&order_pool, &execution_a.bid_order_id)?;
        let a_ask_order = load_order(&order_pool, &execution_a.ask_order_id)?;
        let b_bid_order = load_order(&order_pool, &execution_b.bid_order_id)?;
        let b_ask_order = load_order(&order_pool, &execution_b.ask_order_id)?;

        let batch_id = a_bid_order.batch_id.clone();
        if a_ask_order.batch_id != batch_id
            || b_bid_order.batch_id != batch_id
            || b_ask_order.batch_id != batch_id
            || execution_a.batch_id != batch_id
            || execution_b.batch_id != batch_id
        {
            return Err(SettlementNettingEngineError::BatchMismatch);
        }

        let instrument_id_hash = a_bid_order.instrument_id_hash.clone();
        if a_ask_order.instrument_id_hash != instrument_id_hash
            || b_bid_order.instrument_id_hash != instrument_id_hash
            || b_ask_order.instrument_id_hash != instrument_id_hash
        {
            return Err(SettlementNettingEngineError::InstrumentMismatch);
        }

        let settlement_id = derive_settlement_id(
            &env,
            &proof_receipt_id,
            &settlement_commitment,
            &execution_a_id,
            &execution_b_id,
            &trade_nullifier_a,
            &trade_nullifier_b,
        );
        let batch_key = DataKey::Batch(settlement_id.clone());
        if env.storage().persistent().has(&batch_key) {
            return Err(SettlementNettingEngineError::SettlementExists);
        }

        let record = SettlementBatchRecord {
            settlement_id: settlement_id.clone(),
            batch_id: batch_id.clone(),
            instrument_id_hash,
            settler,
            verifier_id,
            proof_receipt_id: proof_receipt_id.clone(),
            settlement_commitment,
            net_vector_hash,
            execution_a_id: execution_a_id.clone(),
            execution_a_commitment: execution_a.execution_commitment,
            execution_b_id: execution_b_id.clone(),
            execution_b_commitment: execution_b.execution_commitment,
            trade_nullifier_a: trade_nullifier_a.clone(),
            trade_nullifier_b: trade_nullifier_b.clone(),
            recorded_ledger: env.ledger().sequence(),
        };

        env.storage().persistent().set(&trade_nullifier_key_a, &settlement_id);
        env.storage().persistent().set(&trade_nullifier_key_b, &settlement_id);
        env.storage()
            .persistent()
            .set(&DataKey::SettledExecution(execution_a_id), &settlement_id);
        env.storage()
            .persistent()
            .set(&DataKey::SettledExecution(execution_b_id), &settlement_id);
        env.storage().persistent().set(&batch_key, &record);

        bump_persistent(&env, &trade_nullifier_key_a);
        bump_persistent(&env, &trade_nullifier_key_b);
        bump_persistent(&env, &DataKey::SettledExecution(record.execution_a_id.clone()));
        bump_persistent(&env, &DataKey::SettledExecution(record.execution_b_id.clone()));
        bump_persistent(&env, &batch_key);
        bump_instance(&env);

        BatchSettledEvent {
            settlement_id,
            batch_id,
            proof_receipt_id,
        }
        .publish(&env);

        Ok(record)
    }

    pub fn get_batch(
        env: Env,
        settlement_id: BytesN<32>,
    ) -> Result<SettlementBatchRecord, SettlementNettingEngineError> {
        let key = DataKey::Batch(settlement_id);
        let record = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(SettlementNettingEngineError::BatchNotFound)?;
        bump_persistent(&env, &key);
        bump_instance(&env);
        Ok(record)
    }

    pub fn has_batch(env: Env, settlement_id: BytesN<32>) -> bool {
        let key = DataKey::Batch(settlement_id);
        let exists = env.storage().persistent().has(&key);
        if exists {
            bump_persistent(&env, &key);
        }
        bump_instance(&env);
        exists
    }

    pub fn is_trade_nullifier_used(env: Env, trade_nullifier: BytesN<32>) -> bool {
        let key = DataKey::TradeNullifier(trade_nullifier);
        let exists = env.storage().persistent().has(&key);
        if exists {
            bump_persistent(&env, &key);
        }
        bump_instance(&env);
        exists
    }

    pub fn is_execution_settled(env: Env, execution_id: BytesN<32>) -> bool {
        let settled = is_execution_settled_internal(&env, &execution_id);
        bump_instance(&env);
        settled
    }
}

fn load_settler_participant_id(
    env: &Env,
    settler: &Address,
) -> Result<BytesN<32>, SettlementNettingEngineError> {
    let participant_registry: Address = env
        .storage()
        .instance()
        .get(&DataKey::ParticipantRegistry)
        .unwrap();
    let registry = ParticipantRegistryClient::new(env, &participant_registry);
    if !registry.is_wallet_registered(settler) {
        return Err(SettlementNettingEngineError::ParticipantMismatch);
    }
    let participant_id_hash = registry.wallet_owner(settler);
    let participant = registry.get_participant(&participant_id_hash);
    if participant.role != ParticipantRole::SettlementOperator {
        return Err(SettlementNettingEngineError::WrongParticipantRole);
    }
    if participant.status != ParticipantStatus::Active {
        return Err(SettlementNettingEngineError::ParticipantNotActive);
    }
    Ok(participant_id_hash)
}

fn ensure_batch_receipt(
    env: &Env,
    proof_receipt: &ProofReceipt,
    settler: &Address,
    settler_participant_id_hash: &BytesN<32>,
    verifier_id: &BytesN<32>,
    settlement_commitment: &BytesN<32>,
) -> Result<(), SettlementNettingEngineError> {
    if proof_receipt.proof_type != ProofType::BatchNetting {
        return Err(SettlementNettingEngineError::WrongProofType);
    }
    if &proof_receipt.submitter != settler {
        return Err(SettlementNettingEngineError::ProofSubmitterMismatch);
    }
    if &proof_receipt.participant_id_hash != settler_participant_id_hash {
        return Err(SettlementNettingEngineError::ParticipantMismatch);
    }
    if &proof_receipt.verifier_id != verifier_id {
        return Err(SettlementNettingEngineError::ProofVerifierMismatch);
    }
    if &proof_receipt.portfolio_commitment != settlement_commitment {
        return Err(SettlementNettingEngineError::SettlementCommitmentMismatch);
    }
    if env.ledger().sequence() > proof_receipt.expiry_ledger {
        return Err(SettlementNettingEngineError::ProofExpired);
    }
    Ok(())
}

fn ensure_settler_enabled(env: &Env, settler: &Address) -> Result<(), SettlementNettingEngineError> {
    let key = DataKey::Settler(settler.clone());
    let enabled = env.storage().persistent().get(&key).unwrap_or(false);
    if !enabled {
        return Err(SettlementNettingEngineError::SettlerDisabled);
    }
    bump_persistent(env, &key);
    Ok(())
}

fn load_proof_receipt(
    env: &Env,
    proof_receipt_id: &BytesN<32>,
) -> Result<ProofReceipt, SettlementNettingEngineError> {
    let proof_gateway: Address = env.storage().instance().get(&DataKey::ProofGateway).unwrap();
    let gateway = ProofGatewayClient::new(env, &proof_gateway);
    if !gateway.has_receipt(proof_receipt_id) {
        return Err(SettlementNettingEngineError::ProofReceiptNotFound);
    }
    Ok(gateway.get_receipt(proof_receipt_id))
}

fn load_order(
    order_pool: &OrderCommitPoolClient,
    order_id: &BytesN<32>,
) -> Result<zkdtcc_types::OrderCommitmentRecord, SettlementNettingEngineError> {
    if !order_pool.has_order(order_id) {
        return Err(SettlementNettingEngineError::OrderNotFound);
    }
    Ok(order_pool.get_order(order_id))
}

fn derive_settlement_id(
    env: &Env,
    proof_receipt_id: &BytesN<32>,
    settlement_commitment: &BytesN<32>,
    execution_a_id: &BytesN<32>,
    execution_b_id: &BytesN<32>,
    trade_nullifier_a: &BytesN<32>,
    trade_nullifier_b: &BytesN<32>,
) -> BytesN<32> {
    let mut material = Bytes::new(env);
    material.extend_from_slice(SETTLEMENT_DOMAIN);
    append_address(&mut material, &env.current_contract_address());
    material.extend_from_slice(&proof_receipt_id.to_array());
    material.extend_from_slice(&settlement_commitment.to_array());
    material.extend_from_slice(&execution_a_id.to_array());
    material.extend_from_slice(&execution_b_id.to_array());
    material.extend_from_slice(&trade_nullifier_a.to_array());
    material.extend_from_slice(&trade_nullifier_b.to_array());
    env.crypto().sha256(&material).into()
}

fn append_address(material: &mut Bytes, address: &Address) {
    let address_bytes = address.to_string().to_bytes();
    material.extend_from_slice(&address_bytes.len().to_be_bytes());
    material.append(&address_bytes);
}

fn is_execution_settled_internal(env: &Env, execution_id: &BytesN<32>) -> bool {
    let key = DataKey::SettledExecution(execution_id.clone());
    let settled = env.storage().persistent().has(&key);
    if settled {
        bump_persistent(env, &key);
    }
    settled
}

fn require_admin_auth(env: &Env, admin: &Address) -> Result<(), SettlementNettingEngineError> {
    admin.require_auth();
    let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    if stored_admin != *admin {
        return Err(SettlementNettingEngineError::Unauthorized);
    }
    Ok(())
}

fn require_operator_auth(env: &Env, operator: &Address) -> Result<(), SettlementNettingEngineError> {
    operator.require_auth();
    let key = DataKey::Operator(operator.clone());
    let enabled = env.storage().persistent().get(&key).unwrap_or(false);
    if !enabled {
        return Err(SettlementNettingEngineError::Unauthorized);
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
