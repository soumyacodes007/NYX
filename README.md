# zk-dtcc

Phase 0 through Phase 6 implementation for the ZK DTCC/Stellar prototype.

This workspace builds the normalization and CCTP ingress layers described in the plan:

- `AssetRegistry`: supported asset classes and issuer policy commitments.
- `ParticipantRegistry`: participant roles, registered wallets, and credential commitments.
- `LegalStateRegistry`: on-chain commitments tying an entitlement state to participant, wallet, asset, event date, and issuer policy.
- `CctpIngressAdapter`: parses raw Circle CCTP messages, verifies the configured forwarder payload, binds the forwarded Stellar recipient to the participant registry, normalizes CCTP's 6-decimal amount into Stellar's 7-decimal USDC amount, and records replay-safe mint receipts by nonce and session.
- `CollateralPolicy`: stores policy versioning, required margin, current pricing epoch, per-asset haircut/price metadata, and accepted verifier IDs for collateral sufficiency proofs.
- `ProofGateway`: derives statement hashes, routes proof verification to pluggable verifier contracts, checks policy freshness and participant bindings, and records replay-safe proof receipts.
- `EncumbranceRegistry`: records approved custodian availability attestations, enforces protocol-local lot locks keyed by nullifiers, and tracks release/expiry state for unencumbered-lot pledge scope.
- `OrderCommitPool`: stores private order commitments, enforces cancel/execution nullifier uniqueness, validates proof-backed bilateral matches, and records execution commitments without exposing plaintext order data.
- `UnencumberedLotVerifier`: a BN254 Groth16 verifier adapter for `ProofType::UnencumberedLot` that checks the gateway statement hash carried in the proof payload and verifies the proof on Soroban.
- `PrivateMatchVerifier`: a BN254 Groth16 verifier adapter for `ProofType::PrivateMatch` that checks a match proof payload against the gateway statement hash and validates the bilateral match proof on Soroban.
- `BatchNettingVerifier`: a BN254 Groth16 verifier adapter for `ProofType::BatchNetting` that checks a bounded batch-netting proof payload against the gateway statement hash and validates the Phase 5 netting proof on Soroban.
- `SettlementNettingEngine`: records proof-backed settlement batches, enforces same-batch/same-instrument execution grouping from the order pool, consumes trade nullifiers, and marks matched executions as settled.
- `EntitlementClaimVerifier`: a BN254 Groth16 verifier adapter for `ProofType::EntitlementClaim` that checks the gateway statement hash carried in a corporate-action claim proof and validates the Phase 6 entitlement proof on Soroban.
- `CorporateActionsEngine`: stores issuer-registered coupon/dividend event roots, enforces claim windows and participant-role checks, consumes event-specific claim nullifiers, and records proof-backed claim receipts.
- `circuits/unencumbered_lot.circom`: a working Circom 2 + Groth16 Phase 3 circuit proving lot inclusion in an attested Poseidon Merkle root, deriving a scope-bound lot nullifier, and carrying the gateway statement hash as public inputs.
- `circuits/private_match.circom`: a working Circom 2 + Groth16 Phase 4 circuit proving committed bid/ask orders clear on instrument, price, and quantity while binding the resulting execution commitment.
- `circuits/batch_netting.circom`: a working Circom 2 + Groth16 Phase 5 circuit for a bounded two-execution, three-participant MVP batch that recomputes hidden execution commitments, validates participant net deltas, derives settlement nullifiers, and binds a settlement commitment for on-chain recording.
- `circuits/entitlement_claim.circom`: a working Circom 2 + Groth16 Phase 6 circuit proving a participant was included in an issuer event snapshot, deriving an event-specific claim nullifier, recomputing the coupon/dividend claim amount, and binding the gateway statement hash for on-chain verification.

The design follows the Stellar asset guidance used in the plan:

- regulated assets stay modeled as Stellar assets bridged into Soroban via SAC addresses;
- USDC is represented as a SAC-backed asset class;
- legal identity and entitlement detail remain off-chain, while Soroban stores commitments and normalized lookup state.

Each contract uses constructor-based initialization, explicit auth, typed storage keys, and TTL extension for durable state.

For the Phase 3 proof flow, run:

```bash
npm install
npm run zk:phase3:prove
```

The script prefers the repo-local Circom 2 compiler at `.tools/circom2/bin/circom` when present, generates deterministic sample inputs, builds a witness, runs a local Groth16 setup, produces a proof, and verifies it.

To run the Phase 3 proof regression suite:

```bash
npm run zk:phase3:test
```

To run the Phase 4 proof regression suite:

```bash
npm run zk:phase4:test
```

To run the Phase 5 proof regression suite:

```bash
npm run zk:phase5:test
```

To run the Phase 6 proof regression suite:

```bash
npm run zk:phase6:test
```

To run the Soroban workspace tests, including the real Phase 3 BN254 verifier and `ProofGateway` integration path:

```bash
cargo test --offline
```
