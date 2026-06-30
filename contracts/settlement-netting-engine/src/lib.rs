#![no_std]

use soroban_sdk::{
    contract, contractclient, contracterror, contractevent, contractimpl, contracttype, Address,
    token::TokenClient, Bytes, BytesN, Env,
};
use zkdtcc_types::{
    BatchTransferRecord, ExecutionSettlementRecord, ParticipantRole, ParticipantStatus,
    ProofReceipt, ProofType, SettlementBatchRecord,
};

#[contractclient(name = "ParticipantRegistryClient")]
pub trait ParticipantRegistryContract {
    fn is_wallet_registered(env: Env, wallet: Address) -> bool;
    fn is_participant_trade_eligible(env: Env, participant_id_hash: BytesN<32>, asset: Address) -> bool;
    fn wallet_owner(env: Env, wallet: Address) -> BytesN<32>;
    fn get_participant(env: Env, participant_id_hash: BytesN<32>) -> zkdtcc_types::ParticipantRecord;
}

#[contractclient(name = "ProofGatewayClient")]
pub trait ProofGatewayContract {
    fn has_receipt(env: Env, receipt_id: BytesN<32>) -> bool;
    fn get_receipt(env: Env, receipt_id: BytesN<32>) -> ProofReceipt;
    fn is_receipt_usable(env: Env, receipt_id: BytesN<32>) -> bool;
}

#[contractclient(name = "ComplianceControlClient")]
pub trait ComplianceControlContract {
    fn is_globally_paused(env: Env) -> bool;
    fn is_asset_paused(env: Env, asset: Address) -> bool;
    fn is_participant_frozen(env: Env, participant_id_hash: BytesN<32>) -> bool;
}

#[contractclient(name = "OrderCommitPoolClient")]
pub trait OrderCommitPoolContract {
    fn has_execution(env: Env, execution_id: BytesN<32>) -> bool;
    fn get_execution(env: Env, execution_id: BytesN<32>) -> zkdtcc_types::PrivateMatchExecution;
    fn has_order(env: Env, order_id: BytesN<32>) -> bool;
    fn get_order(env: Env, order_id: BytesN<32>) -> zkdtcc_types::OrderCommitmentRecord;
    fn has_instrument_asset(env: Env, instrument_id_hash: BytesN<32>) -> bool;
    fn get_instrument_asset(env: Env, instrument_id_hash: BytesN<32>) -> Address;
}

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
    ComplianceControl,
    OrderCommitPool,
    TradeNullifier(BytesN<32>),
    SettledExecution(BytesN<32>),
    Batch(BytesN<32>),
    DirectSettlement(BytesN<32>),
    BatchTransfer(BytesN<32>),
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
    ProtocolPaused = 22,
    ParticipantFrozen = 23,
    ProofReceiptNotUsable = 24,
    InstrumentAssetNotFound = 25,
    AssetPaused = 26,
    InvalidAmount = 27,
    BatchTransfersAlreadyApplied = 28,
    ExecutionSettlementNotFound = 29,
    BatchTransferNotFound = 30,
    ParticipantIneligible = 31,
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

#[contractevent(topics = ["execution_settled_dvp"])]
pub struct ExecutionSettledDvpEvent {
    pub settlement_id: BytesN<32>,
    pub execution_id: BytesN<32>,
    pub trade_nullifier: BytesN<32>,
}

#[contractevent(topics = ["batch_transfers_applied"])]
pub struct BatchTransfersAppliedEvent {
    pub settlement_id: BytesN<32>,
    pub batch_id: BytesN<32>,
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
        compliance_control: Address,
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
            .set(&DataKey::ComplianceControl, &compliance_control);
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
        ensure_protocol_live(&env)?;
        ensure_settler_enabled(&env, &settler)?;

        if execution_a_id == execution_b_id {
            return Err(SettlementNettingEngineError::DuplicateExecution);
        }
        if trade_nullifier_a == trade_nullifier_b {
            return Err(SettlementNettingEngineError::DuplicateTradeNullifier);
        }

        let settler_participant_id_hash = load_settler_participant_id(&env, &settler)?;
        ensure_participant_not_frozen(&env, &settler_participant_id_hash)?;
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
        ensure_participant_not_frozen(&env, &a_bid_order.participant_id_hash)?;
        ensure_participant_not_frozen(&env, &a_ask_order.participant_id_hash)?;
        ensure_participant_not_frozen(&env, &b_bid_order.participant_id_hash)?;
        ensure_participant_not_frozen(&env, &b_ask_order.participant_id_hash)?;

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

    pub fn settle_execution_dvp(
        env: Env,
        settler: Address,
        execution_id: BytesN<32>,
        trade_nullifier: BytesN<32>,
        cash_asset: Address,
        asset_amount: i128,
        cash_amount: i128,
    ) -> Result<ExecutionSettlementRecord, SettlementNettingEngineError> {
        settler.require_auth();
        ensure_protocol_live(&env)?;
        ensure_settler_enabled(&env, &settler)?;
        if asset_amount <= 0 || cash_amount <= 0 {
            return Err(SettlementNettingEngineError::InvalidAmount);
        }

        let settler_participant_id_hash = load_settler_participant_id(&env, &settler)?;
        ensure_participant_not_frozen(&env, &settler_participant_id_hash)?;

        let order_pool_id: Address = env.storage().instance().get(&DataKey::OrderCommitPool).unwrap();
        let order_pool = OrderCommitPoolClient::new(&env, &order_pool_id);
        if !order_pool.has_execution(&execution_id) {
            return Err(SettlementNettingEngineError::ExecutionNotFound);
        }
        if is_execution_settled_internal(&env, &execution_id) {
            return Err(SettlementNettingEngineError::ExecutionAlreadySettled);
        }

        let trade_nullifier_key = DataKey::TradeNullifier(trade_nullifier.clone());
        if env.storage().persistent().has(&trade_nullifier_key) {
            return Err(SettlementNettingEngineError::TradeNullifierUsed);
        }

        let execution = order_pool.get_execution(&execution_id);
        let bid_order = load_order(&order_pool, &execution.bid_order_id)?;
        let ask_order = load_order(&order_pool, &execution.ask_order_id)?;
        let instrument_asset = load_instrument_asset(&order_pool, &bid_order.instrument_id_hash)?;
        ensure_asset_live(&env, &instrument_asset)?;
        ensure_asset_live(&env, &cash_asset)?;
        ensure_participant_not_frozen(&env, &bid_order.participant_id_hash)?;
        ensure_participant_not_frozen(&env, &ask_order.participant_id_hash)?;
        ensure_participant_trade_eligible(&env, &bid_order.participant_id_hash, &instrument_asset)?;
        ensure_participant_trade_eligible(&env, &ask_order.participant_id_hash, &instrument_asset)?;
        ensure_participant_trade_eligible(&env, &bid_order.participant_id_hash, &cash_asset)?;
        ensure_participant_trade_eligible(&env, &ask_order.participant_id_hash, &cash_asset)?;

        apply_execution_transfers(
            &env,
            &instrument_asset,
            &cash_asset,
            &bid_order.submitter,
            &ask_order.submitter,
            asset_amount,
            cash_amount,
        );

        let settlement_id = derive_direct_settlement_id(
            &env,
            &execution_id,
            &trade_nullifier,
            &instrument_asset,
            &cash_asset,
            asset_amount,
            cash_amount,
        );
        let record = ExecutionSettlementRecord {
            settlement_id: settlement_id.clone(),
            execution_id: execution_id.clone(),
            instrument_id_hash: bid_order.instrument_id_hash,
            instrument_asset,
            cash_asset,
            buyer: bid_order.submitter,
            seller: ask_order.submitter,
            trade_nullifier: trade_nullifier.clone(),
            asset_amount,
            cash_amount,
            settler,
            recorded_ledger: env.ledger().sequence(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::DirectSettlement(execution_id.clone()), &record);
        env.storage().persistent().set(&trade_nullifier_key, &settlement_id);
        env.storage()
            .persistent()
            .set(&DataKey::SettledExecution(execution_id.clone()), &settlement_id);
        bump_persistent(&env, &DataKey::DirectSettlement(execution_id.clone()));
        bump_persistent(&env, &trade_nullifier_key);
        bump_persistent(&env, &DataKey::SettledExecution(execution_id.clone()));
        bump_instance(&env);

        ExecutionSettledDvpEvent {
            settlement_id,
            execution_id,
            trade_nullifier,
        }
        .publish(&env);
        Ok(record)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn settle_batch_with_transfers(
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
        cash_asset: Address,
        execution_a_asset_amount: i128,
        execution_a_cash_amount: i128,
        execution_b_asset_amount: i128,
        execution_b_cash_amount: i128,
    ) -> Result<SettlementBatchRecord, SettlementNettingEngineError> {
        if execution_a_asset_amount <= 0
            || execution_a_cash_amount <= 0
            || execution_b_asset_amount <= 0
            || execution_b_cash_amount <= 0
        {
            return Err(SettlementNettingEngineError::InvalidAmount);
        }

        let record = Self::settle_batch(
            env.clone(),
            settler,
            verifier_id,
            proof_receipt_id,
            settlement_commitment,
            net_vector_hash,
            execution_a_id.clone(),
            execution_b_id.clone(),
            trade_nullifier_a,
            trade_nullifier_b,
        )?;

        apply_batch_transfers_internal(
            &env,
            &record,
            &cash_asset,
            execution_a_asset_amount,
            execution_a_cash_amount,
            execution_b_asset_amount,
            execution_b_cash_amount,
        )?;
        Ok(record)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn apply_batch_transfers(
        env: Env,
        settler: Address,
        settlement_id: BytesN<32>,
        cash_asset: Address,
        execution_a_asset_amount: i128,
        execution_a_cash_amount: i128,
        execution_b_asset_amount: i128,
        execution_b_cash_amount: i128,
    ) -> Result<BatchTransferRecord, SettlementNettingEngineError> {
        settler.require_auth();
        ensure_protocol_live(&env)?;
        ensure_settler_enabled(&env, &settler)?;
        if execution_a_asset_amount <= 0
            || execution_a_cash_amount <= 0
            || execution_b_asset_amount <= 0
            || execution_b_cash_amount <= 0
        {
            return Err(SettlementNettingEngineError::InvalidAmount);
        }

        let settler_participant_id_hash = load_settler_participant_id(&env, &settler)?;
        ensure_participant_not_frozen(&env, &settler_participant_id_hash)?;

        let record = Self::get_batch(env.clone(), settlement_id)?;
        if record.settler != settler {
            return Err(SettlementNettingEngineError::Unauthorized);
        }

        apply_batch_transfers_internal(
            &env,
            &record,
            &cash_asset,
            execution_a_asset_amount,
            execution_a_cash_amount,
            execution_b_asset_amount,
            execution_b_cash_amount,
        )
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

    pub fn get_execution_settlement(
        env: Env,
        execution_id: BytesN<32>,
    ) -> Result<ExecutionSettlementRecord, SettlementNettingEngineError> {
        let key = DataKey::DirectSettlement(execution_id);
        let record = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(SettlementNettingEngineError::ExecutionSettlementNotFound)?;
        bump_persistent(&env, &key);
        bump_instance(&env);
        Ok(record)
    }

    pub fn get_batch_transfer(
        env: Env,
        settlement_id: BytesN<32>,
    ) -> Result<BatchTransferRecord, SettlementNettingEngineError> {
        let key = DataKey::BatchTransfer(settlement_id);
        let record = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(SettlementNettingEngineError::BatchTransferNotFound)?;
        bump_persistent(&env, &key);
        bump_instance(&env);
        Ok(record)
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
    let proof_gateway: Address = env.storage().instance().get(&DataKey::ProofGateway).unwrap();
    let gateway = ProofGatewayClient::new(env, &proof_gateway);
    if !gateway.is_receipt_usable(&proof_receipt.receipt_id) {
        return Err(SettlementNettingEngineError::ProofReceiptNotUsable);
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

fn ensure_protocol_live(env: &Env) -> Result<(), SettlementNettingEngineError> {
    let compliance_control: Address = env.storage().instance().get(&DataKey::ComplianceControl).unwrap();
    let compliance = ComplianceControlClient::new(env, &compliance_control);
    if compliance.is_globally_paused() {
        return Err(SettlementNettingEngineError::ProtocolPaused);
    }
    Ok(())
}

fn ensure_participant_not_frozen(
    env: &Env,
    participant_id_hash: &BytesN<32>,
) -> Result<(), SettlementNettingEngineError> {
    let compliance_control: Address = env.storage().instance().get(&DataKey::ComplianceControl).unwrap();
    let compliance = ComplianceControlClient::new(env, &compliance_control);
    if compliance.is_participant_frozen(participant_id_hash) {
        return Err(SettlementNettingEngineError::ParticipantFrozen);
    }
    Ok(())
}

fn ensure_asset_live(env: &Env, asset: &Address) -> Result<(), SettlementNettingEngineError> {
    let compliance_control: Address = env.storage().instance().get(&DataKey::ComplianceControl).unwrap();
    let compliance = ComplianceControlClient::new(env, &compliance_control);
    if compliance.is_asset_paused(asset) {
        return Err(SettlementNettingEngineError::AssetPaused);
    }
    Ok(())
}

fn ensure_participant_trade_eligible(
    env: &Env,
    participant_id_hash: &BytesN<32>,
    asset: &Address,
) -> Result<(), SettlementNettingEngineError> {
    let participant_registry: Address = env
        .storage()
        .instance()
        .get(&DataKey::ParticipantRegistry)
        .unwrap();
    let registry = ParticipantRegistryClient::new(env, &participant_registry);
    if !registry.is_participant_trade_eligible(participant_id_hash, asset) {
        return Err(SettlementNettingEngineError::ParticipantIneligible);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn apply_batch_transfers_internal(
    env: &Env,
    record: &SettlementBatchRecord,
    cash_asset: &Address,
    execution_a_asset_amount: i128,
    execution_a_cash_amount: i128,
    execution_b_asset_amount: i128,
    execution_b_cash_amount: i128,
) -> Result<BatchTransferRecord, SettlementNettingEngineError> {
    let transfer_key = DataKey::BatchTransfer(record.settlement_id.clone());
    if env.storage().persistent().has(&transfer_key) {
        return Err(SettlementNettingEngineError::BatchTransfersAlreadyApplied);
    }

    let order_pool_id: Address = env.storage().instance().get(&DataKey::OrderCommitPool).unwrap();
    let order_pool = OrderCommitPoolClient::new(env, &order_pool_id);
    let execution_a = order_pool.get_execution(&record.execution_a_id);
    let execution_b = order_pool.get_execution(&record.execution_b_id);
    let a_bid_order = load_order(&order_pool, &execution_a.bid_order_id)?;
    let a_ask_order = load_order(&order_pool, &execution_a.ask_order_id)?;
    let b_bid_order = load_order(&order_pool, &execution_b.bid_order_id)?;
    let b_ask_order = load_order(&order_pool, &execution_b.ask_order_id)?;
    let instrument_asset = load_instrument_asset(&order_pool, &record.instrument_id_hash)?;

    ensure_asset_live(env, &instrument_asset)?;
    ensure_asset_live(env, cash_asset)?;
    ensure_participant_not_frozen(env, &a_bid_order.participant_id_hash)?;
    ensure_participant_not_frozen(env, &a_ask_order.participant_id_hash)?;
    ensure_participant_not_frozen(env, &b_bid_order.participant_id_hash)?;
    ensure_participant_not_frozen(env, &b_ask_order.participant_id_hash)?;
    ensure_participant_trade_eligible(env, &a_bid_order.participant_id_hash, &instrument_asset)?;
    ensure_participant_trade_eligible(env, &a_ask_order.participant_id_hash, &instrument_asset)?;
    ensure_participant_trade_eligible(env, &b_bid_order.participant_id_hash, &instrument_asset)?;
    ensure_participant_trade_eligible(env, &b_ask_order.participant_id_hash, &instrument_asset)?;
    ensure_participant_trade_eligible(env, &a_bid_order.participant_id_hash, cash_asset)?;
    ensure_participant_trade_eligible(env, &a_ask_order.participant_id_hash, cash_asset)?;
    ensure_participant_trade_eligible(env, &b_bid_order.participant_id_hash, cash_asset)?;
    ensure_participant_trade_eligible(env, &b_ask_order.participant_id_hash, cash_asset)?;

    apply_execution_transfers(
        env,
        &instrument_asset,
        cash_asset,
        &a_bid_order.submitter,
        &a_ask_order.submitter,
        execution_a_asset_amount,
        execution_a_cash_amount,
    );
    apply_execution_transfers(
        env,
        &instrument_asset,
        cash_asset,
        &b_bid_order.submitter,
        &b_ask_order.submitter,
        execution_b_asset_amount,
        execution_b_cash_amount,
    );

    let transfer_record = BatchTransferRecord {
        settlement_id: record.settlement_id.clone(),
        instrument_asset,
        cash_asset: cash_asset.clone(),
        execution_a_buyer: a_bid_order.submitter,
        execution_a_seller: a_ask_order.submitter,
        execution_a_asset_amount,
        execution_a_cash_amount,
        execution_b_buyer: b_bid_order.submitter,
        execution_b_seller: b_ask_order.submitter,
        execution_b_asset_amount,
        execution_b_cash_amount,
        recorded_ledger: env.ledger().sequence(),
    };
    env.storage().persistent().set(&transfer_key, &transfer_record);
    bump_persistent(env, &transfer_key);
    bump_instance(env);
    BatchTransfersAppliedEvent {
        settlement_id: record.settlement_id.clone(),
        batch_id: record.batch_id.clone(),
    }
    .publish(env);
    Ok(transfer_record)
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

fn load_instrument_asset(
    order_pool: &OrderCommitPoolClient,
    instrument_id_hash: &BytesN<32>,
) -> Result<Address, SettlementNettingEngineError> {
    if !order_pool.has_instrument_asset(instrument_id_hash) {
        return Err(SettlementNettingEngineError::InstrumentAssetNotFound);
    }
    Ok(order_pool.get_instrument_asset(instrument_id_hash))
}

fn apply_execution_transfers(
    env: &Env,
    instrument_asset: &Address,
    cash_asset: &Address,
    buyer: &Address,
    seller: &Address,
    asset_amount: i128,
    cash_amount: i128,
) {
    let spender = env.current_contract_address();
    TokenClient::new(env, instrument_asset).transfer_from(&spender, seller, buyer, &asset_amount);
    TokenClient::new(env, cash_asset).transfer_from(&spender, buyer, seller, &cash_amount);
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

fn derive_direct_settlement_id(
    env: &Env,
    execution_id: &BytesN<32>,
    trade_nullifier: &BytesN<32>,
    instrument_asset: &Address,
    cash_asset: &Address,
    asset_amount: i128,
    cash_amount: i128,
) -> BytesN<32> {
    let mut material = Bytes::new(env);
    material.extend_from_slice(b"zkdtcc:execution-dvp:v1");
    append_address(&mut material, &env.current_contract_address());
    material.extend_from_slice(&execution_id.to_array());
    material.extend_from_slice(&trade_nullifier.to_array());
    append_address(&mut material, instrument_asset);
    append_address(&mut material, cash_asset);
    material.extend_from_slice(&asset_amount.to_be_bytes());
    material.extend_from_slice(&cash_amount.to_be_bytes());
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
