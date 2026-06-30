# Frontend Screen Content Spec

## Purpose

This document defines what each frontend page should show for the 10-page institutional demo.

This is not a design guide. It does not prescribe layout, colors, spacing, typography, or interactions beyond what data and UI elements must exist on each screen.

The objective is to make sure every page communicates the correct product story:

- institutional onboarding
- registered wallet control
- asset policy awareness
- proof-backed participation
- private order admission
- private execution
- real settlement
- compliance intervention
- selective disclosure

## Shared Data Conventions

Use these shared concepts consistently across all pages.

### Institution Labels

- `Alpha`: JPMorgan-style broker-dealer
- `Beta`: institutional counterparty
- `Gamma`: additional participant for batch settlement
- `Treasury`: issuer-side or DTC-controlled treasury / omnibus wallet
- `Compliance`: venue oversight operator
- `Matcher`: private matching operator
- `Settler`: settlement operator
- `Auditor / Regulator`: scoped disclosure recipient

### Asset Labels

- `DTCUST10Y-ENT`: Treasury entitlement asset
- `DTCSPY-ENT`: ETF-style entitlement asset
- `USDC`: settlement cash asset

### Standard Status Chips

These should exist as content labels somewhere in the UI when relevant:

- `Active`
- `Paused`
- `Frozen`
- `Approved`
- `Rejected`
- `Prepared`
- `Submitted`
- `Verified`
- `Usable`
- `Revoked`
- `Expired`
- `Pending`
- `Confirmed`
- `Failed`

### Standard Reference Fields

When available, show:

- participant ID hash
- wallet address
- contract ID
- receipt ID
- order ID
- execution ID
- settlement ID
- claim ID
- case ID
- tx hash
- ledger sequence
- timestamp

## Page 1: Market Overview

### Goal

Give the user the current protocol state and explain what market they are looking at.

### Must Show

- protocol name / environment label
- network name: `Stellar Testnet`
- latest ledger sequence
- protocol live status
- global pause status
- number of registered participants
- number of supported assets
- number of active proofs or recent proof receipts
- number of recent trades / settlements
- recent compliance actions count

### Market Summary Section

- list of active institutions
- list of supported assets
- last completed settlement
- last compliance action
- last disclosure action

### Story Summary Section

- short explanation that this is a private institutional market for regulated tokenized assets
- short explanation that trading is permissioned by participant, wallet, asset policy, and proof status

### Recommended Data Fields

- `protocolLive`
- `globalPause`
- `latestLedger`
- `participantsCount`
- `assetsCount`
- `recentSettlementId`
- `recentSettlementTxHash`
- `recentComplianceCaseId`

### SVG Assets Needed

- `market-network.svg`
- `institution-cluster.svg`
- `tokenized-market.svg`
- `settlement-flow-mini.svg`
- `shield-check.svg`

## Page 2: Participant Onboarding

### Goal

Show how institutions become approved market participants.

### Must Show

For each participant card:

- participant display name
- participant role
- participant ID hash
- legal entity hash
- jurisdiction hash
- credential root
- participant status
- KYC status
- sanctions status
- credential expiry ledger
- review case ID
- primary wallet
- total wallet count
- created ledger
- updated ledger

### Participant Roles To Show

- institution trader
- compliance operator
- matcher
- settlement operator
- issuer / treasury admin
- auditor / regulator

### Actions To Represent

- participant registered
- compliance state updated
- permissions updated
- wallet added

### Recommended Data Fields

- `participantIdHash`
- `role`
- `status`
- `kycStatus`
- `sanctionsStatus`
- `credentialExpiryLedger`
- `reviewCaseId`
- `walletCount`

### SVG Assets Needed

- `broker-dealer.svg`
- `compliance-officer.svg`
- `matcher-node.svg`
- `settlement-desk.svg`
- `issuer-admin.svg`
- `auditor.svg`
- `legal-entity-badge.svg`

## Page 3: Wallet Registration

### Goal

Show that the market only accepts approved wallets.

### Must Show

- connected wallet state
- wallet address
- wallet network
- whether wallet is registered
- mapped participant name
- mapped participant ID hash
- whether wallet is primary wallet
- wallet registration history or state summary

### Approval Logic Section

- registered wallet required: yes/no
- participant binding confirmed: yes/no
- frozen check result
- trade eligibility check result

### If Showing Multiple Wallets

For each wallet:

- address
- wallet type or label
- primary / secondary
- active / removed

### Recommended Data Fields

- `walletAddress`
- `walletRegistered`
- `walletOwnerParticipantIdHash`
- `isPrimaryWallet`
- `participantFrozen`
- `participantTradeEligible`

### SVG Assets Needed

- `wallet-connect.svg`
- `registered-wallet.svg`
- `wallet-approved.svg`
- `wallet-rejected.svg`
- `key-link.svg`

## Page 4: Asset Registry

### Goal

Show that assets are regulated policy objects, not generic tokens.

### Must Show

For each asset:

- asset display name
- asset symbol
- issuer
- SAC contract ID
- asset class
- asset status
- settlement enabled
- corporate actions enabled
- requires registered wallets
- requires issuer auth
- clawback enabled
- metadata hash
- issuer policy hash
- transfer class hash
- jurisdiction policy hash
- permissions hash

### Assets To Show

- `DTCUST10Y-ENT`
- `DTCSPY-ENT`
- `USDC`

### Explain Asset Purpose

- `DTCUST10Y-ENT`: tokenized Treasury entitlement
- `DTCSPY-ENT`: tokenized ETF-style entitlement
- `USDC`: settlement cash asset

### Recommended Data Fields

- `assetAddress`
- `assetClass`
- `status`
- `settlementEnabled`
- `corporateActionsEnabled`
- `requiresRegisteredWallets`
- `issuerPolicyHash`

### SVG Assets Needed

- `treasury-bond.svg`
- `etf-asset.svg`
- `usdc-coin.svg`
- `issuer-policy.svg`
- `regulated-asset.svg`

## Page 5: Proof Center

### Goal

Show that private participation is controlled by proof receipts.

### Must Show

For each proof type:

- proof type name
- participant name
- receipt ID
- verifier ID
- proof status
- submitter wallet
- participant binding
- statement hash
- portfolio / claim / execution commitment
- nonce
- expiry ledger
- policy version
- epoch ID
- required margin if relevant
- revocation status

### Proof Types To Show

- collateral sufficiency proof
- unencumbered lot proof
- private match proof
- batch netting proof
- entitlement claim proof

### Status Explanation

Show whether the proof is:

- prepared off-chain
- submitted on-chain
- verified
- usable
- revoked
- expired

### Recommended Data Fields

- `proofType`
- `receiptId`
- `verifierId`
- `statementHash`
- `participantIdHash`
- `submitter`
- `nonce`
- `expiryLedger`
- `usable`
- `revoked`

### SVG Assets Needed

- `proof-receipt.svg`
- `collateral-proof.svg`
- `unencumbered-proof.svg`
- `private-match-proof.svg`
- `batch-netting-proof.svg`
- `claim-proof.svg`

## Page 6: Private Order Entry

### Goal

Show order admission under compliance and proof constraints.

### Must Show

For each order ticket:

- participant name
- side: buy / sell
- asset or instrument
- batch ID
- order commitment
- collateral proof receipt ID
- encumbrance proof receipt ID
- cancel nullifier
- expiry ledger
- order status
- submitter wallet

### Validation Panel

- wallet registered
- participant active
- participant not frozen
- asset live
- asset not paused
- collateral proof usable
- encumbrance proof usable

### Order Outcomes

- order committed
- order cancelled
- order expired
- order blocked

### Recommended Data Fields

- `orderId`
- `participantName`
- `instrumentIdHash`
- `batchId`
- `orderCommitment`
- `collateralReceiptId`
- `encumbranceReceiptId`
- `cancelNullifier`
- `expiryLedger`
- `status`

### SVG Assets Needed

- `buy-order.svg`
- `sell-order.svg`
- `private-ticket.svg`
- `commitment-lock.svg`
- `cancel-nullifier.svg`

## Page 7: Match & Execution Room

### Goal

Show how two private orders become one private execution.

### Must Show

- bid order ID
- ask order ID
- batch ID
- instrument ID
- execution ID
- matcher identity
- private match proof receipt ID
- verifier ID
- execution commitment
- encrypted receipt hash
- bid execution nullifier
- ask execution nullifier
- execution recorded ledger

### Matching Preconditions

- both orders active
- same instrument
- same batch
- no self-trade
- nullifiers unused
- proof receipt usable

### Execution Outcome

- execution created
- execution rejected

### Recommended Data Fields

- `executionId`
- `bidOrderId`
- `askOrderId`
- `matcher`
- `proofReceiptId`
- `executionCommitment`
- `encryptedReceiptHash`
- `bidExecutionNullifier`
- `askExecutionNullifier`

### SVG Assets Needed

- `matching-engine.svg`
- `private-execution.svg`
- `encrypted-receipt.svg`
- `order-cross.svg`
- `execution-link.svg`

## Page 8: Settlement Console

### Goal

Show real asset and cash movement on-chain.

### Must Show

#### Direct Settlement Section

- settlement ID
- execution ID
- trade nullifier
- proof receipt ID
- settlement tx hash
- cash asset
- instrument asset
- buyer
- seller
- balances before
- balances after

#### Batch Settlement Section

- settlement ID
- batch ID
- execution A ID
- execution B ID
- trade nullifier A
- trade nullifier B
- settlement commitment
- net vector hash
- batch settlement tx hash
- transfer application tx hash
- balances before
- balances after

### Balance Display

Show both:

- `USDC`
- entitlement asset quantity

for:

- Alpha
- Beta
- Gamma if batch

### Recommended Data Fields

- `settlementId`
- `batchId`
- `settlementTxHash`
- `transferTxHash`
- `balancesBefore`
- `balancesAfter`
- `cashAsset`
- `instrumentAsset`

### SVG Assets Needed

- `dvp-settlement.svg`
- `batch-settlement.svg`
- `cash-leg.svg`
- `asset-leg.svg`
- `netting.svg`
- `balance-shift.svg`

## Page 9: Compliance Console

### Goal

Show that compliance is the control plane of the system.

### Must Show

#### Protocol Controls

- global pause status
- asset pause status by asset
- participant freeze status by participant

#### Verifier Controls

- verifier route enabled / disabled
- verifier policy enabled / disabled
- valid from ledger
- valid until ledger
- policy cutoff hash

#### Receipt Controls

- revoked receipt list
- revoked receipt reason code
- revoked receipt case ID

#### Operator Action Log

For each action:

- action ID
- action type
- operator address
- target hash
- reason code
- case ID
- metadata hash
- created ledger

#### Downstream Impact Panel

Show blocked effects such as:

- order rejected because participant frozen
- proof unusable because revoked
- settlement blocked because asset paused

### Recommended Data Fields

- `globalPause`
- `assetPauseMap`
- `participantFreezeMap`
- `verifierPolicies`
- `revokedReceipts`
- `operatorActions`
- `blockedActions`

### SVG Assets Needed

- `compliance-shield.svg`
- `global-pause.svg`
- `participant-freeze.svg`
- `asset-pause.svg`
- `receipt-revoke.svg`
- `case-file.svg`
- `blocked-action.svg`

## Page 10: Audit & Regulator Room

### Goal

Show selective disclosure and post-incident reconstruction.

### Must Show

#### Disclosure Grant Section

- scope hash
- grantee address
- encrypted key hash
- expiry ledger
- grant tx hash

#### Access Receipt Section

- accessor
- purpose code
- case ID
- blob hash
- access tx hash
- access timestamp or ledger

#### Timeline Section

Link the following in one case view:

- participant
- proof receipt
- order commit
- private execution
- settlement
- freeze / pause action
- disclosure grant
- access record

#### Optional Corporate Action Linkage

If included in the same case flow:

- event ID
- claim ID
- claim status
- payout tx hash

### Recommended Data Fields

- `scopeHash`
- `grantee`
- `encryptedKeyHash`
- `accessor`
- `purposeCode`
- `caseId`
- `blobHash`
- `linkedSettlementId`
- `linkedActionIds`

### SVG Assets Needed

- `audit-room.svg`
- `regulator-access.svg`
- `disclosure-key.svg`
- `timeline-chain.svg`
- `document-blob.svg`
- `case-reconstruction.svg`

## Shared Utility Components

These are not full pages, but the frontend will likely need them across screens.

### Status Components

- participant status badge
- asset status badge
- proof status badge
- tx status badge
- compliance status badge

### Reference Components

- hash display with copy action
- wallet address display
- tx hash display
- ledger display
- timestamp display

### SVG Assets Needed

- `hash-chip.svg`
- `tx-confirmed.svg`
- `tx-pending.svg`
- `tx-failed.svg`
- `ledger-block.svg`
- `address-badge.svg`

## SVG Asset Master List

Create these SVGs for the full demo:

- `market-network.svg`
- `institution-cluster.svg`
- `tokenized-market.svg`
- `settlement-flow-mini.svg`
- `shield-check.svg`
- `broker-dealer.svg`
- `compliance-officer.svg`
- `matcher-node.svg`
- `settlement-desk.svg`
- `issuer-admin.svg`
- `auditor.svg`
- `legal-entity-badge.svg`
- `wallet-connect.svg`
- `registered-wallet.svg`
- `wallet-approved.svg`
- `wallet-rejected.svg`
- `key-link.svg`
- `treasury-bond.svg`
- `etf-asset.svg`
- `usdc-coin.svg`
- `issuer-policy.svg`
- `regulated-asset.svg`
- `proof-receipt.svg`
- `collateral-proof.svg`
- `unencumbered-proof.svg`
- `private-match-proof.svg`
- `batch-netting-proof.svg`
- `claim-proof.svg`
- `buy-order.svg`
- `sell-order.svg`
- `private-ticket.svg`
- `commitment-lock.svg`
- `cancel-nullifier.svg`
- `matching-engine.svg`
- `private-execution.svg`
- `encrypted-receipt.svg`
- `order-cross.svg`
- `execution-link.svg`
- `dvp-settlement.svg`
- `batch-settlement.svg`
- `cash-leg.svg`
- `asset-leg.svg`
- `netting.svg`
- `balance-shift.svg`
- `compliance-shield.svg`
- `global-pause.svg`
- `participant-freeze.svg`
- `asset-pause.svg`
- `receipt-revoke.svg`
- `case-file.svg`
- `blocked-action.svg`
- `audit-room.svg`
- `regulator-access.svg`
- `disclosure-key.svg`
- `timeline-chain.svg`
- `document-blob.svg`
- `case-reconstruction.svg`
- `hash-chip.svg`
- `tx-confirmed.svg`
- `tx-pending.svg`
- `tx-failed.svg`
- `ledger-block.svg`
- `address-badge.svg`

## Minimum First Wave SVGs

If you want to prioritize, create these first:

- `treasury-bond.svg`
- `etf-asset.svg`
- `usdc-coin.svg`
- `broker-dealer.svg`
- `wallet-connect.svg`
- `proof-receipt.svg`
- `buy-order.svg`
- `sell-order.svg`
- `matching-engine.svg`
- `dvp-settlement.svg`
- `compliance-shield.svg`
- `participant-freeze.svg`
- `audit-room.svg`
- `timeline-chain.svg`

## Final Rule

Every page should answer one practical question:

- Overview: what market is this?
- Onboarding: who is allowed in?
- Wallets: which wallet is valid?
- Assets: what can move?
- Proofs: what was privately proven?
- Orders: who is trying to trade?
- Match: what execution was formed?
- Settlement: what moved on-chain?
- Compliance: who stopped what, and why?
- Audit: who can reconstruct the case?
