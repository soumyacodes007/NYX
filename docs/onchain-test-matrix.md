# On-Chain Test Matrix

This file tracks the execution order for the full 15-step on-chain validation plan and links the first live evidence captured on Stellar testnet.

## Current Deployment Base

- Manifest: `deployments/testnet-phase0-demo0628b.json`
- Phase 0 report: `deployments/reports/phase0-onchain-checks-demo0628b-2026-06-28t21-06-24-902z.json`
- Verification runner: `npm run stellar:phase0:verify`

## Status

1. `Done` Freeze a clean testnet environment
   - Captured latest ledger, live contract IDs, asset contract IDs, and wallet snapshots.

2. `Done` Validate Phase 0 state
   - Verified `AssetRegistry`, `ParticipantRegistry`, `ComplianceControl`, and `LegalStateRegistry` against live contract state.

3. `Done` Test raw asset behavior outside protocol
   - Executed real round-trip transfers for `USDC`, `DTCUST10Y-ENT`, and `DTCSPY-ENT` through an unregistered probe wallet.
   - Important finding: raw classic-asset transfers work once trustlines are present and the issuer authorizes them. Registered-wallet restrictions are therefore enforced by protocol contracts, not by the classic asset rail alone.

4. `Next` Test protocol contract deploy set
   - Deploy the remaining contracts on testnet using the live SAC addresses from the phase-0 manifest.
   - Target set:
     - `CollateralPolicy`
     - `ProofGateway`
     - `CctpIngressAdapter`
     - `EncumbranceRegistry`
     - `OrderCommitPool`
     - `SettlementNettingEngine`
     - `CorporateActionsEngine`
     - verifier contracts used by the proof types already implemented

5. `Pending` Test participant and compliance gates
   - Unregistered wallet rejection
   - Frozen participant rejection
   - Paused asset rejection
   - Global pause rejection
   - Expired / invalid compliance state rejection

6. `Pending` Test proof gateway on chain
   - Valid proof receipt
   - Reused nonce rejection
   - Wrong participant binding rejection
   - Revoked receipt rejection
   - Stale policy / disabled verifier rejection

7. `Pending` Test Phase 3 encumbrance on chain
   - Lock valid lot
   - Reject reused lot nullifier
   - Reject expired attestation
   - Release lot and confirm lifecycle state

8. `Pending` Test Phase 4 order flow on chain
   - Commit bid and ask
   - Cancel order
   - Reject reused cancel nullifier
   - Record valid private match execution
   - Reject wrong batch / wrong proof binding cases

9. `Pending` Test real asset movement before full netting
   - Wire settlement contracts to move live SAC balances
   - Execute a minimal live settlement path with USDC plus one entitlement asset

10. `Pending` Test Phase 5 batch settlement on chain
    - Valid batch settlement
    - Duplicate execution rejection
    - Duplicate trade nullifier rejection
    - Mismatched instrument rejection
    - Paused / frozen failure paths

11. `Pending` Test Phase 6 corporate actions on chain
    - Register event
    - Valid claim
    - Duplicate claim rejection
    - Claim window enforcement
    - Paid / reversed lifecycle as implemented

12. `Pending` Test Phase 7 / 7.5 audit and compliance
    - Disclosure grants and revocation
    - Access receipts
    - Operator action receipts
    - Freeze then verify downstream trade / settlement / claim blocks

13. `Pending` Test cross-chain ingress separately
    - Direct Stellar-side settlement first
    - Then CCTP watcher / forwarding / replay protection

14. `Pending` Run scenario matrix, not only the demo path
    - Happy path
    - Compliance-blocked path
    - Stale-proof path
    - Paused-market path
    - Duplicate-nullifier path
    - Corporate-action path
    - Disclosure / auditor path
    - Cross-chain ingress path

15. `Pending` Record evidence for every case
    - Tx hash
    - Contract ID
    - Expected result
    - Actual result
    - Balance diff
    - Contract state / event diff

## Immediate Next Build Task

The next concrete implementation task is a testnet deployment runner for step 4 that binds the remaining protocol contracts to:

- `USDC` SAC: `CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA`
- `DTCUST10Y-ENT` SAC: `CCNKHDFEYNERMDU44FZIHJJDZNZ2B5HQE744V3EMK2HS2QYVGW3JWRA5`
- `DTCSPY-ENT` SAC: `CD2BG2AWKSTKEORVCSKLEXLQIEWLW7SFPBF7WQL4UHGGFQOPCETITKJG`

After that deploy step, we can move directly into steps 5 through 10 with live contracts instead of simulation-only tests.
