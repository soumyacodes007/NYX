# Frontend Demo Spec

## Purpose

This document defines the production-style demo for the zk-DTCC Stellar MVP. The goal is to present the system as a compliance-first institutional market workflow, not as a circuit showcase or a generic blockchain app.

The demo story must prove five things:

1. Institutions are explicitly onboarded and approved before they can act.
2. Assets are policy-controlled and cannot move freely like retail tokens.
3. Private proofs enable order admission and settlement without exposing sensitive data.
4. Compliance can intervene at any point and block downstream activity.
5. Auditors and regulators can reconstruct scoped records without making the whole market public.

## Product Positioning

This is a private institutional market operating layer for tokenized regulated assets on Stellar.

The frontend should make the user feel like they are operating:

- a regulated venue
- a compliance control plane
- a private settlement rail
- an audit and disclosure system

It should not feel like:

- a crypto wallet demo
- a blockchain explorer
- a ZK math dashboard

## Demo Entities

### Institutions

- `Alpha`: JPMorgan-style broker-dealer / institutional trading participant
- `Beta`: second broker-dealer / institutional counterparty
- `Gamma`: additional participant used in batch settlement
- `Treasury`: issuer-side or DTC-controlled inventory / payout wallet
- `Compliance Operator`: venue or market oversight desk
- `Matcher`: private matching service
- `Settler`: settlement operator
- `Auditor / Regulator`: scoped disclosure recipient

### Assets

- `DTCUST10Y-ENT`: DTC-style tokenized entitlement representing a U.S. Treasury-style security
- `DTCSPY-ENT`: DTC-style tokenized entitlement representing an ETF-style security for simpler audience explanation
- `USDC`: settlement cash asset

### On-Chain Systems

- `ParticipantRegistry`
- `AssetRegistry`
- `ComplianceControl`
- `ProofGateway`
- `CctpIngressAdapter`
- `EncumbranceRegistry`
- `OrderCommitPool`
- `SettlementNettingEngine`
- `CorporateActionsEngine`
- `AuditDisclosureRegistry`

## Demo Principles

### Real

These should be real and live in the demo:

- wallet connection
- wallet signatures
- contract invocation on Stellar testnet
- tx hash generation
- participant registry lookups
- proof receipt submission and receipt usability checks
- order commit and match recording
- settlement state transitions
- real asset movement for settlement paths
- compliance freeze / pause enforcement
- audit disclosure grants and access logs
- corporate-action payout movement

### Precomputed But Real

These should be precomputed off-chain and then submitted / verified live:

- collateral sufficiency proofs
- unencumbered lot proofs
- private match proofs
- batch netting proofs
- entitlement claim proofs

### Mocked Or Demo-Grade

These are acceptable to mock or simplify for the MVP demo:

- real DTCC / LedgerScan integration
- real issuer event feed
- real sanctions oracle
- real institutional case-management backend
- live proof generation for heavy circuits
- full legal documentation blobs
- cross-chain CCTP mint flow during the live demo

The demo must clearly treat these as infrastructure placeholders, not as fake live behavior.

## Full Demo Story

The recommended story is a 10-step institutional operating flow:

1. Compliance onboards Alpha and Beta as approved participants.
2. Alpha connects an approved wallet.
3. The venue shows Alpha is allowed to trade approved DTC assets.
4. Alpha and Beta each have valid private proof receipts prepared.
5. Alpha submits a private buy order.
6. Beta submits a private sell order.
7. The matcher records a private execution.
8. The settlement engine settles the trade and moves `USDC` and the entitlement asset on-chain.
9. Compliance freezes Alpha under a case after a post-trade event.
10. Auditor / regulator receives scoped access to reconstruct the incident.

An optional second branch can show:

- a batch settlement involving Alpha, Beta, and Gamma
- a corporate-action claim and payout

## Page Architecture

The product demo should use 10 pages.

### 1. Market Overview

#### Purpose

Establish the institutional context and current market state.

#### Core UI

- protocol status banner
- participant counts
- supported assets
- latest ledger / network info
- recent market actions
- compliance status summary

#### User Actions

- jump to participant onboarding
- jump to trading flow
- jump to compliance console

#### Data Sources

- indexer
- chain read service

#### Real vs Mocked

- real network status
- real participant / asset counts
- mocked explanatory labels and product copy

### 2. Participant Onboarding

#### Purpose

Show that institutions are not anonymous wallets; they are registered legal participants.

#### Core UI

- participant card for Alpha
- participant card for Beta
- participant ID hash
- legal entity hash
- jurisdiction hash
- role
- KYC status
- sanctions status
- credential expiry
- review case ID

#### User Actions

- approve participant
- suspend participant
- update compliance state

#### Data Sources

- `ParticipantRegistry`
- compliance metadata service

#### Real vs Mocked

- real participant registry state
- mocked readable institution names and explanatory legal metadata text

### 3. Wallet Registration

#### Purpose

Show that only approved wallets can act for approved institutions.

#### Core UI

- connect wallet panel
- primary wallet mapping
- registered-wallet badge
- wallet ownership proof via participant registry
- additional wallet assignment history

#### User Actions

- connect trader wallet
- register additional wallet
- set primary wallet

#### Data Sources

- `ParticipantRegistry`
- wallet adapter

#### Real vs Mocked

- real wallet connect and signatures
- real wallet registration state
- mocked institution branding around the wallet

### 4. Asset Registry

#### Purpose

Show that assets are policy objects, not free-floating tokens.

#### Core UI

- asset cards for `DTCUST10Y-ENT`, `DTCSPY-ENT`, `USDC`
- settlement enabled flag
- corporate actions enabled flag
- requires registered wallets flag
- issuer policy hash
- transfer class hash
- jurisdiction policy hash

#### User Actions

- view policy details
- pause asset from compliance console link

#### Data Sources

- `AssetRegistry`

#### Real vs Mocked

- real on-chain asset policy state
- mocked plain-English asset descriptions

### 5. Proof Center

#### Purpose

Show that private eligibility is enforced through proofs without turning the demo into a prover dashboard.

#### Core UI

- proof cards by type
- status: `prepared`, `submitted`, `verified`, `usable`, `revoked`, `expired`
- receipt ID
- participant binding
- verifier ID
- expiry ledger
- policy version / epoch binding

#### User Actions

- fetch proof bundle
- submit proof receipt
- inspect receipt
- revoke receipt from compliance role

#### Data Sources

- proof artifact service
- `ProofGateway`
- `CollateralPolicy`

#### Real vs Mocked

- real receipt submission and usability checks
- precomputed proof bundles
- mocked human-readable proof explanations

### 6. Private Order Entry

#### Purpose

Show order admission under compliance and proof controls.

#### Core UI

- order ticket for Alpha
- order ticket for Beta
- asset selector
- side
- hidden commitment ID
- collateral receipt reference
- encumbrance receipt reference
- cancel nullifier state
- expiry ledger

#### User Actions

- submit Alpha order
- submit Beta order
- cancel order

#### Data Sources

- `OrderCommitPool`
- `ProofGateway`
- `ParticipantRegistry`
- `ComplianceControl`

#### Real vs Mocked

- real order commit transactions
- precomputed proofs
- fixed demo scenarios for quantity / price semantics

### 7. Match & Execution Room

#### Purpose

Show the private execution event created from two admitted orders.

#### Core UI

- bid order ID
- ask order ID
- batch ID
- execution ID
- encrypted receipt hash
- execution nullifiers
- private match proof receipt

#### User Actions

- trigger match
- inspect execution payload

#### Data Sources

- `OrderCommitPool`
- proof artifact service

#### Real vs Mocked

- real match recording on-chain
- precomputed private match proof
- mocked readable “price / size” labels if plaintext is intentionally hidden

### 8. Settlement Console

#### Purpose

Show real value movement and finality.

#### Core UI

- bilateral settlement section
- batch settlement section
- balances before
- balances after
- settlement tx hash
- transfer tx hash
- settlement ID
- batch ID

#### User Actions

- settle direct DvP trade
- settle batch
- apply batch transfers

#### Data Sources

- `SettlementNettingEngine`
- token balances from RPC / Horizon
- indexer

#### Real vs Mocked

- fully real on-chain settlement transitions
- fully real asset movement
- precomputed batch netting proof

### 9. Compliance Console

#### Purpose

This is the hero page. It proves the product is a compliance operating system, not just a private exchange.

#### Core UI

- global pause control
- asset pause control
- participant freeze / unfreeze
- verifier policy enable / disable
- proof receipt revoke
- operator action log
- case IDs
- downstream impact panel

#### User Actions

- freeze Alpha
- unfreeze Alpha
- pause `DTCUST10Y-ENT`
- revoke a proof receipt
- disable a verifier policy

#### Data Sources

- `ComplianceControl`
- `ParticipantRegistry`
- `AssetRegistry`
- `ProofGateway`
- compliance case service

#### Real vs Mocked

- real on-chain pause / freeze / revoke actions
- real downstream blocked transaction attempts
- mocked human case notes

### 10. Audit & Regulator Room

#### Purpose

Show selective disclosure and incident reconstruction.

#### Core UI

- disclosure scope
- grantee identity
- case-linked access receipts
- blob hash records
- linked participant / trade / settlement / freeze timeline
- optional corporate-action claim record

#### User Actions

- grant disclosure
- record access
- open case timeline

#### Data Sources

- `AuditDisclosureRegistry`
- compliance case service
- disclosure metadata service
- indexer

#### Real vs Mocked

- real on-chain grant / access records
- mocked encrypted file contents and reconstructed document viewer

## Recommended Backend Services

The off-chain system should be split into the following services.

### 1. Frontend API Server

Responsibilities:

- serve page data
- aggregate chain state for UI
- call proof artifact service
- forward scenario actions

### 2. Chain Indexer

Responsibilities:

- subscribe to contract events
- normalize participant, proof, order, settlement, claim, and audit state
- expose query APIs for UI timelines and dashboards

### 3. Proof Artifact Service

Responsibilities:

- store precomputed proof bundles
- map demo scenario actions to proof payloads
- return proof metadata and public inputs

### 4. Demo Scenario Orchestrator

Responsibilities:

- drive repeatable Alpha/Beta/Gamma flows
- advance scenario stage
- reset demo state
- prevent operator mistakes during live presentation

### 5. Compliance Case Service

Responsibilities:

- store case metadata
- attach readable incident labels to on-chain case IDs
- store operator notes and review context

### 6. Disclosure Metadata Service

Responsibilities:

- track blob hashes
- track disclosure package metadata
- attach off-chain audit package references to on-chain grants

### 7. Corporate Action Snapshot Service

Responsibilities:

- prepare event manifests
- generate snapshot metadata
- attach claim package context

### 8. Wallet / Session Layer

Responsibilities:

- maintain connected wallet session state
- show signer role
- coordinate signing prompts

## Backend API Shape

Suggested API groups:

- `GET /api/overview`
- `GET /api/participants`
- `GET /api/participants/:id`
- `GET /api/assets`
- `GET /api/proofs`
- `POST /api/proofs/:scenario/prepare`
- `POST /api/orders/alpha`
- `POST /api/orders/beta`
- `POST /api/matches/:scenario/execute`
- `POST /api/settlements/direct`
- `POST /api/settlements/batch`
- `POST /api/compliance/freeze`
- `POST /api/compliance/unfreeze`
- `POST /api/compliance/pause-global`
- `POST /api/compliance/pause-asset`
- `POST /api/compliance/revoke-receipt`
- `GET /api/audit/cases/:caseId`
- `POST /api/audit/grants`

The API should not fabricate on-chain success. It should return:

- `pending`
- `submitted`
- `confirmed`
- `failed`

along with tx hashes.

## Demo Runbook

### Phase A: Setup

1. Open Market Overview
2. Confirm protocol is live
3. Confirm Alpha and Beta are approved

### Phase B: Onboarding

4. Open Participant Onboarding
5. Show Alpha legal/compliance state
6. Open Wallet Registration
7. Connect Alpha wallet

### Phase C: Trading Admission

8. Open Asset Registry
9. Show `DTCUST10Y-ENT` and `USDC` are approved
10. Open Proof Center
11. Show Alpha and Beta proof receipts are prepared and usable

### Phase D: Trade Flow

12. Open Private Order Entry
13. Submit Alpha buy order
14. Submit Beta sell order
15. Open Match & Execution Room
16. Trigger match

### Phase E: Settlement

17. Open Settlement Console
18. Settle direct trade
19. Show before / after balances
20. Optionally run batch settlement path

### Phase F: Compliance Intervention

21. Open Compliance Console
22. Freeze Alpha with case ID
23. Retry Alpha order submission
24. Show rejection

### Phase G: Audit

25. Open Audit & Regulator Room
26. Grant scoped disclosure for Alpha case
27. Show access receipt and linked trade / settlement / freeze timeline

## What Must Be Fixed Before Frontend Demo Lock

- all page actions must resolve to real tx hashes when marked live
- proof artifact service must use the same public inputs as the on-chain script path
- settlement page must display real balances from chain reads
- compliance page must show at least one real blocked downstream action
- audit page must show linked on-chain action records, not static text only

## What Can Stay Simplified For MVP

- no live heavy proof generation in the browser
- no arbitrary user-generated trade parameters
- no real external sanctions service
- no full DTCC or issuer production integration
- no free-form market making or open orderbook

## Final UX Message

The frontend should leave the viewer with one conclusion:

Private trading is the feature, but compliance control is the product.
