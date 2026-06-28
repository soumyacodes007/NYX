# Compliance Hardening MVP

This document turns the Phase 7.5 compliance hardening idea into a buildable MVP scope.

## Goal

Upgrade the current privacy-first trading prototype into a compliance-aware institutional MVP with:

- emergency pause and freeze controls
- verifier and proof receipt governance
- scoped disclosure receipts for auditors and regulators
- richer participant and asset compliance state
- auditable operator actions for reversals, overrides, and manual interventions

## Current Baseline

The current repo already enforces:

- registered wallets and participant roles
- supported asset policy flags
- verifier allowlists and proof receipt recording
- nullifier replay protection
- role-based settlement and corporate-action claims

What is still missing is operational compliance: manual controls, exception handling, proof revocation, and auditor-grade disclosure logging.

## Contracts

### 1. `ComplianceControl`

Purpose:

- central emergency control plane
- protocol-wide pause and freeze checks
- auditable operator interventions

Storage keys:

- `Admin`
- `Operator(Address)`
- `GlobalPause`
- `AssetPause(Address)`
- `ParticipantFreeze(BytesN<32>)`
- `EmergencyAction(BytesN<32>)`
- `OperatorAction(BytesN<32>)`

Suggested records:

```rust
pub struct PauseState {
    pub paused: bool,
    pub reason_code: BytesN<32>,
    pub case_id: BytesN<32>,
    pub updated_ledger: u32,
}

pub struct ParticipantFreezeState {
    pub frozen: bool,
    pub reason_code: BytesN<32>,
    pub case_id: BytesN<32>,
    pub updated_ledger: u32,
}

pub enum EmergencyActionType {
    ForcedTransfer = 1,
    ForcedUnwind = 2,
    ClaimReversal = 3,
}

pub struct OperatorActionRecord {
    pub action_id: BytesN<32>,
    pub action_type: EmergencyActionType,
    pub operator: Address,
    pub target_hash: BytesN<32>,
    pub reason_code: BytesN<32>,
    pub case_id: BytesN<32>,
    pub metadata_hash: BytesN<32>,
    pub created_ledger: u32,
}
```

APIs:

- `set_global_pause(operator, paused, reason_code, case_id)`
- `set_asset_pause(operator, asset, paused, reason_code, case_id)`
- `set_participant_freeze(operator, participant_id_hash, frozen, reason_code, case_id)`
- `request_emergency_transfer(operator, asset, from_participant_id_hash, to_participant_id_hash, amount, reason_code, case_id, metadata_hash)`
- `request_emergency_unwind(operator, settlement_id, reason_code, case_id, metadata_hash)`
- `get_operator_action(action_id)`

Integration points:

- `CctpIngressAdapter`
- `OrderCommitPool`
- `SettlementNettingEngine`
- `CorporateActionsEngine`

Each of those contracts should query `ComplianceControl` before accepting a new high-risk action.

### 2. `ParticipantRegistry` extensions

Purpose:

- move from simple status to usable compliance state

New fields:

```rust
pub enum KycStatus {
    Approved = 1,
    Pending = 2,
    Expired = 3,
    Rejected = 4,
}

pub enum SanctionsStatus {
    Clear = 1,
    Review = 2,
    Blocked = 3,
}
```

Add to `ParticipantRecord`:

- `kyc_status`
- `sanctions_status`
- `credential_expiry_ledger`
- `review_case_id`
- `permissions_hash`

APIs:

- `set_compliance_state(participant_id_hash, kyc_status, sanctions_status, credential_expiry_ledger, review_case_id)`
- `set_permissions_hash(participant_id_hash, permissions_hash)`
- `is_participant_trade_eligible(participant_id_hash, asset)`

MVP rule:

- a participant is trade-eligible only if identity is active, KYC is approved, sanctions status is clear, credentials are not expired, and the participant is not frozen in `ComplianceControl`

### 3. `AssetRegistry` extensions

Purpose:

- separate "known asset" from "currently allowed for settlement or claims"

Add to `AssetRecord`:

- `settlement_enabled`
- `corporate_actions_enabled`
- `transfer_class_hash`
- `jurisdiction_policy_hash`
- `asset_permissions_hash`

APIs:

- `set_transfer_policy(asset, settlement_enabled, corporate_actions_enabled, jurisdiction_policy_hash, transfer_class_hash)`
- `set_asset_permissions_hash(asset, asset_permissions_hash)`
- `is_asset_settlement_enabled(asset)`
- `is_asset_corporate_actions_enabled(asset)`

MVP rule:

- asset registration is baseline support
- settlement and claims each require their own enable flags

### 4. `ProofGateway` extensions

Purpose:

- distinguish historical proof receipts from currently usable proof receipts

New storage:

- `VerifierPolicy(BytesN<32>)`
- `RevokedReceipt(BytesN<32>)`

Suggested record:

```rust
pub struct VerifierPolicy {
    pub verifier_id: BytesN<32>,
    pub enabled: bool,
    pub valid_from_ledger: u32,
    pub valid_until_ledger: u32,
    pub policy_cutoff_hash: BytesN<32>,
    pub updated_ledger: u32,
}
```

APIs:

- `set_verifier_policy(verifier_id, enabled, valid_from_ledger, valid_until_ledger, policy_cutoff_hash)`
- `revoke_receipt(receipt_id, reason_code, case_id)`
- `is_receipt_usable(receipt_id)`

MVP rule:

- do not delete historical receipts
- allow them to remain visible while becoming unusable for future orders, settlement, or claims

### 5. `AuditDisclosureRegistry`

Purpose:

- make regulator and auditor access measurable and immutable

Storage keys:

- `Blob(BytesN<32>)`
- `Grant(BytesN<32>)`
- `AccessReceipt(BytesN<32>)`
- `ViewKeyCommitment(BytesN<32>)`
- `OperatorActionLink(BytesN<32>)`

Suggested records:

```rust
pub struct DisclosureBlob {
    pub blob_hash: BytesN<32>,
    pub blob_type: u32,
    pub owner_scope_hash: BytesN<32>,
    pub metadata_hash: BytesN<32>,
    pub created_ledger: u32,
}

pub struct DisclosureGrant {
    pub grant_id: BytesN<32>,
    pub scope_hash: BytesN<32>,
    pub grantee: Address,
    pub encrypted_key_hash: BytesN<32>,
    pub purpose_code: BytesN<32>,
    pub case_id: BytesN<32>,
    pub expiry_ledger: u32,
    pub active: bool,
}

pub struct AccessReceipt {
    pub receipt_id: BytesN<32>,
    pub scope_hash: BytesN<32>,
    pub accessor: Address,
    pub purpose_code: BytesN<32>,
    pub case_id: BytesN<32>,
    pub blob_hash: BytesN<32>,
    pub access_ledger: u32,
}
```

APIs:

- `register_blob(blob_hash, blob_type, owner_scope_hash, metadata_hash)`
- `grant(scope_hash, grantee, encrypted_key_hash, expiry_ledger, purpose_code, case_id)`
- `revoke_grant(grant_id, case_id)`
- `record_access(scope_hash, accessor, purpose_code, case_id, blob_hash)`
- `link_operator_action(action_id, scope_hash, blob_hash)`

MVP rule:

- every disclosure action creates an access receipt
- revocation affects future access, not historical receipts

### 6. `CorporateActionsEngine` extensions

Purpose:

- separate claim acceptance from payout completion and reversal

Add to event/claim records:

- `claim_status`
- `payment_batch_id`
- `reversal_reference`
- `withholding_policy_hash`

APIs:

- `mark_claim_paid(claim_id, payment_batch_id, case_id)`
- `reverse_claim(claim_id, reversal_reference, case_id)`
- `set_withholding_policy(event_id, withholding_policy_hash)`

MVP rule:

- proof-backed claim acceptance does not imply payout finality

## Flow-Level Enforcement

### Trade path

1. Trader submits order.
2. `OrderCommitPool` checks:
   - participant eligibility in `ParticipantRegistry`
   - participant freeze in `ComplianceControl`
   - proof receipt usability in `ProofGateway`
   - relevant asset policy in `AssetRegistry`
3. Match is recorded only if all gates pass.
4. Settlement checks the same controls again at batch time.

### Corporate action path

1. Issuer registers event.
2. Claimant submits proof-backed claim.
3. `CorporateActionsEngine` checks:
   - participant eligibility
   - claim window
   - asset action enablement
   - proof receipt usability
   - participant freeze
4. Claim moves from `Recorded` to `Paid` later.
5. Reversal, if needed, produces an operator action record.

### Audit path

1. Compliance operator creates a disclosure grant.
2. Off-chain package is prepared and encrypted.
3. Auditor or regulator accesses it.
4. `AuditDisclosureRegistry.record_access` emits an immutable receipt.

## Minimal MVP Permission Model

- `institution trader`
  - commit order
  - cancel order
  - claim entitlement
  - request access to own records
- `compliance operator`
  - pause protocol
  - freeze participant
  - pause asset
  - revoke grant
  - revoke proof receipt
  - create operator action records
- `matcher`
  - record private match
- `settlement operator`
  - settle proof-backed batch
- `issuer/DTC admin`
  - manage asset policy
  - register corporate actions
  - mark claim payment state
- `auditor`
  - consume scoped disclosure grants
- `regulator`
  - consume scoped disclosure grants

## Off-Chain Services

### `compliance-console`

- creates case IDs
- stores encrypted operator notes
- submits freeze, pause, revocation, and override actions

### `policy-publisher`

- publishes verifier policy cutoff manifests
- publishes participant and asset permission manifests
- hashes those manifests for onchain reference

### `disclosure-packager`

- builds reveal bundle for a scope
- encrypts package for grantee
- submits access receipt linkbacks

## Test Plan

Unit tests:

- pause/freeze toggles
- verifier cutoff windows
- receipt revocation
- grant creation and revocation
- claim payout and reversal states

Integration tests:

- successful trade, then participant freeze blocks settlement
- successful claim, then payout mark and reversal receipt
- grant active allows access receipt
- revoked grant blocks later access

Negative tests:

- expired participant credentials
- blocked sanctions status
- paused asset used in settlement
- revoked proof receipt used in order commit
- frozen participant attempts claim

## Delivery Order

1. Build `ComplianceControl`
2. Extend `ParticipantRegistry`
3. Extend `AssetRegistry`
4. Extend `ProofGateway`
5. Build `AuditDisclosureRegistry`
6. Extend `CorporateActionsEngine`
7. Wire checks into ingress, order, settlement, and claim paths

## Deliberate Deferrals

These are out of MVP scope:

- automated sanctions oracle ingestion
- legal workflow engine
- tax withholding calculation engine
- full forced-transfer execution instead of request/receipt pattern
- recursive or ZK-enforced disclosure consistency on every access
