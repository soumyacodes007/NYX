# Circuits

This directory contains the active Circom proof paths for the project.

Current circuit:

- `unencumbered_lot.circom`
  - proves a selected lot leaf is included in an attested Poseidon Merkle root
  - binds the lot leaf to `participantIdHash` and `assetIdHash`
  - derives a scope-specific lot nullifier from `scopeHash`, `reasonHash`, `proofNonce`, `lotIdHash`, and `lotSalt`
  - carries the `ProofGateway` statement hash as two 128-bit public limbs so the on-chain verifier can bind the proof to a specific receipt context
- `private_match.circom`
  - proves committed bid and ask orders share the same instrument commitment
  - proves `bidLimitPrice >= askLimitPrice`
  - proves the clear price is inside the crossed spread
  - proves the clear quantity fits inside both committed order quantities
  - derives a hidden execution commitment from both participants, instrument, clear price, clear quantity, and execution salt
  - carries the `ProofGateway` statement hash as two 128-bit public limbs for verifier binding
- `batch_netting.circom`
  - proves two hidden Phase 4 execution commitments belong to the same instrument
  - proves three participant slots carry the correct net quantity and net cash deltas for the batch
  - derives deterministic trade nullifiers from each execution commitment and a batch salt
  - derives a settlement commitment from both execution commitments, the net vector hash, and both trade nullifiers
  - carries the `ProofGateway` statement hash as two 128-bit public limbs for verifier binding
- `entitlement_claim.circom`
  - proves an entitlement leaf belongs to an issuer/DTC corporate-action snapshot root
  - binds the entitlement to `participantIdHash`, `assetIdHash`, and `eventIdHash`
  - recomputes the payout as `entitlementQuantity * payoutRate`
  - derives a hidden claim commitment and an event-specific claim nullifier from private claim salt
  - carries the `ProofGateway` statement hash as two 128-bit public limbs for verifier binding

Public inputs:

- `participantIdHash`
- `assetIdHash`
- `availabilityRoot`
- `scopeHash`
- `reasonHash`
- `proofNonce`
- `statementHashHi`
- `statementHashLo`
- `lotNullifier`

Public inputs for `private_match.circom`:

- `bidOrderCommitment`
- `askOrderCommitment`
- `instrumentIdHash`
- `executionCommitment`
- `statementHashHi`
- `statementHashLo`

Public inputs for `batch_netting.circom`:

- `executionCommitmentA`
- `executionCommitmentB`
- `settlementCommitment`
- `tradeNullifierA`
- `tradeNullifierB`
- `statementHashHi`
- `statementHashLo`

Public inputs for `entitlement_claim.circom`:

- `participantIdHash`
- `assetIdHash`
- `eventIdHash`
- `eventRoot`
- `claimCommitment`
- `claimNullifier`
- `claimAmount`
- `statementHashHi`
- `statementHashLo`

Private witness:

- `lotIdHash`
- `quantity`
- `lotSalt`
- `pathElements[4]`
- `pathIndices[4]`

Private witness for `private_match.circom`:

- `bidParticipantIdHash`
- `bidLimitPrice`
- `bidQuantity`
- `bidOrderSalt`
- `bidCollateralProofReceiptId`
- `bidEncumbranceProofReceiptId`
- `askParticipantIdHash`
- `askLimitPrice`
- `askQuantity`
- `askOrderSalt`
- `askCollateralProofReceiptId`
- `askEncumbranceProofReceiptId`
- `clearPrice`
- `clearQuantity`
- `executionSalt`

Private witness for `batch_netting.circom`:

- `instrumentIdHash`
- `tradeBidParticipantIdHash[2]`
- `tradeAskParticipantIdHash[2]`
- `tradeClearPrice[2]`
- `tradeClearQuantity[2]`
- `tradeExecutionSalt[2]`
- `slotParticipantIdHash[3]`
- `slotNetQty[3]`
- `slotNetCash[3]`
- `batchSalt`

Private witness for `entitlement_claim.circom`:

- `entitlementQuantity`
- `payoutRate`
- `snapshotSalt`
- `claimSalt`
- `pathElements[4]`
- `pathIndices[4]`

Local flow:

```bash
npm install
npm run zk:phase3:prove
```

That command:

1. compiles the Circom circuit
2. generates deterministic sample inputs
3. computes a witness
4. runs a local Groth16 setup
5. generates a proof
6. verifies the proof

Artifacts are written to `circuits/artifacts/unencumbered_lot/`.

For the Phase 5 proof flow, run:

```bash
npm run zk:phase5:prove
```

Artifacts are written to `circuits/artifacts/batch_netting/` unless a test namespace overrides that path.

For the Phase 6 proof flow, run:

```bash
npm run zk:phase6:prove
```

Artifacts are written to `circuits/artifacts/entitlement_claim/` unless a test namespace overrides that path.
