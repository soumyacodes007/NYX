#![no_std]

use soroban_sdk::{contracttype, Address, BytesN};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssetClass {
    DtcEntitlement = 1,
    UsdcSac = 2,
    MockRegulated = 3,
    Sep57TrexLike = 4,
    Other = 5,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssetStatus {
    Pending = 1,
    Active = 2,
    Suspended = 3,
    Retired = 4,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParticipantRole {
    InstitutionTrader = 1,
    ComplianceOperator = 2,
    Matcher = 3,
    SettlementOperator = 4,
    IssuerOrDtcAdmin = 5,
    Auditor = 6,
    Regulator = 7,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParticipantStatus {
    Pending = 1,
    Active = 2,
    Suspended = 3,
    Revoked = 4,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KycStatus {
    Approved = 1,
    Pending = 2,
    Expired = 3,
    Rejected = 4,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SanctionsStatus {
    Clear = 1,
    Review = 2,
    Blocked = 3,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegalStateStatus {
    Active = 1,
    Superseded = 2,
    Archived = 3,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProofType {
    Eligibility = 1,
    CollateralSufficiency = 2,
    UnencumberedLot = 3,
    PrivateMatch = 4,
    BatchNetting = 5,
    EntitlementClaim = 6,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrderSide {
    Bid = 1,
    Ask = 2,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrderStatus {
    Active = 1,
    Cancelled = 2,
    Matched = 3,
    Expired = 4,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CorporateActionType {
    Coupon = 1,
    Dividend = 2,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CorporateActionStatus {
    Active = 1,
    Closed = 2,
    Cancelled = 3,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CorporateActionClaimStatus {
    Recorded = 1,
    Paid = 2,
    Reversed = 3,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComplianceActionType {
    Generic = 1,
    ForcedTransferRequest = 2,
    ForcedUnwindRequest = 3,
    ClaimReversal = 4,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetRecord {
    pub asset_id_hash: BytesN<32>,
    pub issuer: Address,
    pub asset_class: AssetClass,
    pub status: AssetStatus,
    pub uses_sac: bool,
    pub requires_registered_wallets: bool,
    pub requires_issuer_auth: bool,
    pub clawback_enabled: bool,
    pub metadata_hash: BytesN<32>,
    pub issuer_policy_hash: BytesN<32>,
    pub settlement_enabled: bool,
    pub corporate_actions_enabled: bool,
    pub transfer_class_hash: BytesN<32>,
    pub jurisdiction_policy_hash: BytesN<32>,
    pub asset_permissions_hash: BytesN<32>,
    pub created_ledger: u32,
    pub updated_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParticipantRecord {
    pub primary_wallet: Address,
    pub role: ParticipantRole,
    pub status: ParticipantStatus,
    pub credential_root: BytesN<32>,
    pub legal_entity_hash: BytesN<32>,
    pub jurisdiction_hash: BytesN<32>,
    pub kyc_status: KycStatus,
    pub sanctions_status: SanctionsStatus,
    pub credential_expiry_ledger: u32,
    pub review_case_id: BytesN<32>,
    pub permissions_hash: BytesN<32>,
    pub wallet_count: u32,
    pub created_ledger: u32,
    pub updated_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalStateRecord {
    pub participant_id_hash: BytesN<32>,
    pub wallet: Address,
    pub entitlement_id_hash: BytesN<32>,
    pub asset: Address,
    pub event_date: u64,
    pub issuer_policy_hash: BytesN<32>,
    pub state_commitment: BytesN<32>,
    pub status: LegalStateStatus,
    pub created_ledger: u32,
    pub updated_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CctpMintReceipt {
    pub source_domain: u32,
    pub destination_domain: u32,
    pub nonce: BytesN<32>,
    pub forward_recipient: Address,
    pub usdc_asset: Address,
    pub amount_6_decimals: i128,
    pub amount_7_decimals: i128,
    pub session_id: BytesN<32>,
    pub message_hash: BytesN<32>,
    pub attestation_hash: BytesN<32>,
    pub recorded_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollateralPolicySummary {
    pub policy_version: u32,
    pub current_epoch: u64,
    pub required_margin: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollateralAssetPolicy {
    pub asset: Address,
    pub decimals: u32,
    pub haircut_bps: u32,
    pub price: i128,
    pub price_epoch: u64,
    pub enabled: bool,
    pub updated_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProofVerifierRoute {
    pub verifier_id: BytesN<32>,
    pub verifier: Address,
    pub enabled: bool,
    pub updated_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProofVerifierPolicy {
    pub verifier_id: BytesN<32>,
    pub enabled: bool,
    pub valid_from_ledger: u32,
    pub valid_until_ledger: u32,
    pub policy_cutoff_hash: BytesN<32>,
    pub updated_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProofReceipt {
    pub receipt_id: BytesN<32>,
    pub proof_type: ProofType,
    pub participant_id_hash: BytesN<32>,
    pub submitter: Address,
    pub verifier_id: BytesN<32>,
    pub statement_hash: BytesN<32>,
    pub portfolio_commitment: BytesN<32>,
    pub nonce: BytesN<32>,
    pub policy_version: u32,
    pub epoch_id: u64,
    pub required_margin: i128,
    pub expiry_ledger: u32,
    pub recorded_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncumbranceAttestor {
    pub attestor_id: BytesN<32>,
    pub public_key: BytesN<32>,
    pub enabled: bool,
    pub updated_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AvailabilityAttestation {
    pub attestation_id: BytesN<32>,
    pub attestor_id: BytesN<32>,
    pub participant_id_hash: BytesN<32>,
    pub asset: Address,
    pub availability_root: BytesN<32>,
    pub scope_hash: BytesN<32>,
    pub issued_at_ledger: u32,
    pub expiry_ledger: u32,
    pub recorded_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncumbranceLock {
    pub lock_id: BytesN<32>,
    pub lot_nullifier: BytesN<32>,
    pub participant_id_hash: BytesN<32>,
    pub submitter: Address,
    pub asset: Address,
    pub attestation_id: BytesN<32>,
    pub proof_receipt_id: BytesN<32>,
    pub availability_root: BytesN<32>,
    pub scope_hash: BytesN<32>,
    pub reason_hash: BytesN<32>,
    pub quantity: i128,
    pub expiry_ledger: u32,
    pub released: bool,
    pub release_reference: BytesN<32>,
    pub created_ledger: u32,
    pub updated_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderCommitmentRecord {
    pub order_id: BytesN<32>,
    pub participant_id_hash: BytesN<32>,
    pub submitter: Address,
    pub instrument_id_hash: BytesN<32>,
    pub batch_id: BytesN<32>,
    pub side: OrderSide,
    pub order_commitment: BytesN<32>,
    pub collateral_proof_receipt_id: BytesN<32>,
    pub encumbrance_proof_receipt_id: BytesN<32>,
    pub expiry_ledger: u32,
    pub status: OrderStatus,
    pub cancel_nullifier: BytesN<32>,
    pub matched_execution_id: BytesN<32>,
    pub created_ledger: u32,
    pub updated_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivateMatchExecution {
    pub execution_id: BytesN<32>,
    pub batch_id: BytesN<32>,
    pub bid_order_id: BytesN<32>,
    pub ask_order_id: BytesN<32>,
    pub matcher: Address,
    pub verifier_id: BytesN<32>,
    pub proof_receipt_id: BytesN<32>,
    pub execution_commitment: BytesN<32>,
    pub encrypted_receipt_hash: BytesN<32>,
    pub bid_execution_nullifier: BytesN<32>,
    pub ask_execution_nullifier: BytesN<32>,
    pub recorded_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettlementBatchRecord {
    pub settlement_id: BytesN<32>,
    pub batch_id: BytesN<32>,
    pub instrument_id_hash: BytesN<32>,
    pub settler: Address,
    pub verifier_id: BytesN<32>,
    pub proof_receipt_id: BytesN<32>,
    pub settlement_commitment: BytesN<32>,
    pub net_vector_hash: BytesN<32>,
    pub execution_a_id: BytesN<32>,
    pub execution_a_commitment: BytesN<32>,
    pub execution_b_id: BytesN<32>,
    pub execution_b_commitment: BytesN<32>,
    pub trade_nullifier_a: BytesN<32>,
    pub trade_nullifier_b: BytesN<32>,
    pub recorded_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionSettlementRecord {
    pub settlement_id: BytesN<32>,
    pub execution_id: BytesN<32>,
    pub instrument_id_hash: BytesN<32>,
    pub instrument_asset: Address,
    pub cash_asset: Address,
    pub buyer: Address,
    pub seller: Address,
    pub trade_nullifier: BytesN<32>,
    pub asset_amount: i128,
    pub cash_amount: i128,
    pub settler: Address,
    pub recorded_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchTransferRecord {
    pub settlement_id: BytesN<32>,
    pub instrument_asset: Address,
    pub cash_asset: Address,
    pub execution_a_buyer: Address,
    pub execution_a_seller: Address,
    pub execution_a_asset_amount: i128,
    pub execution_a_cash_amount: i128,
    pub execution_b_buyer: Address,
    pub execution_b_seller: Address,
    pub execution_b_asset_amount: i128,
    pub execution_b_cash_amount: i128,
    pub recorded_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorporateActionEventRecord {
    pub event_id: BytesN<32>,
    pub asset: Address,
    pub payout_asset: Address,
    pub issuer: Address,
    pub verifier_id: BytesN<32>,
    pub action_type: CorporateActionType,
    pub status: CorporateActionStatus,
    pub event_root: BytesN<32>,
    pub manifest_hash: BytesN<32>,
    pub metadata_hash: BytesN<32>,
    pub record_date: u64,
    pub ex_date: u64,
    pub payable_date: u64,
    pub claim_start_ledger: u32,
    pub claim_end_ledger: u32,
    pub payout_rate: i128,
    pub withholding_policy_hash: BytesN<32>,
    pub created_ledger: u32,
    pub updated_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorporateActionClaimRecord {
    pub claim_id: BytesN<32>,
    pub event_id: BytesN<32>,
    pub claimant: Address,
    pub participant_id_hash: BytesN<32>,
    pub verifier_id: BytesN<32>,
    pub proof_receipt_id: BytesN<32>,
    pub claim_commitment: BytesN<32>,
    pub claim_nullifier: BytesN<32>,
    pub disclosed_entitlement_quantity: i128,
    pub disclosed_claim_amount: i128,
    pub claim_status: CorporateActionClaimStatus,
    pub payment_batch_id: BytesN<32>,
    pub reversal_reference: BytesN<32>,
    pub recorded_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PauseState {
    pub paused: bool,
    pub reason_code: BytesN<32>,
    pub case_id: BytesN<32>,
    pub updated_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParticipantFreezeState {
    pub frozen: bool,
    pub reason_code: BytesN<32>,
    pub case_id: BytesN<32>,
    pub updated_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComplianceOperatorActionRecord {
    pub action_id: BytesN<32>,
    pub action_type: ComplianceActionType,
    pub operator: Address,
    pub target_hash: BytesN<32>,
    pub reason_code: BytesN<32>,
    pub case_id: BytesN<32>,
    pub metadata_hash: BytesN<32>,
    pub created_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevokedProofReceipt {
    pub receipt_id: BytesN<32>,
    pub reason_code: BytesN<32>,
    pub case_id: BytesN<32>,
    pub revoked_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisclosureBlob {
    pub blob_hash: BytesN<32>,
    pub blob_type: u32,
    pub owner_scope_hash: BytesN<32>,
    pub metadata_hash: BytesN<32>,
    pub created_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisclosureGrant {
    pub grant_id: BytesN<32>,
    pub scope_hash: BytesN<32>,
    pub grantee: Address,
    pub encrypted_key_hash: BytesN<32>,
    pub purpose_code: BytesN<32>,
    pub case_id: BytesN<32>,
    pub expiry_ledger: u32,
    pub active: bool,
    pub created_ledger: u32,
    pub updated_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisclosureAccessReceipt {
    pub receipt_id: BytesN<32>,
    pub scope_hash: BytesN<32>,
    pub accessor: Address,
    pub purpose_code: BytesN<32>,
    pub case_id: BytesN<32>,
    pub blob_hash: BytesN<32>,
    pub access_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewKeyCommitment {
    pub scope_hash: BytesN<32>,
    pub commitment_hash: BytesN<32>,
    pub updated_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperatorActionLinkRecord {
    pub action_id: BytesN<32>,
    pub scope_hash: BytesN<32>,
    pub blob_hash: BytesN<32>,
    pub linked_ledger: u32,
}
