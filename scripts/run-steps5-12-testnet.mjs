#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { createHash, createPrivateKey, sign as cryptoSign } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

const args = parseArgs(process.argv.slice(2));
const onlyStep = args["only-step"] ? Number(args["only-step"]) : null;
const phase0Path = path.resolve(args.phase0 ?? "deployments/testnet-phase0-demo0628b.json");
const protocolPath = path.resolve(args.protocol ?? "deployments/testnet-protocol-demo0628b.json");
const outDir = path.resolve(args["report-dir"] ?? "deployments/reports");
mkdirSync(outDir, { recursive: true });

const phase0 = JSON.parse(readFileSync(phase0Path, "utf8"));
const protocol = JSON.parse(readFileSync(protocolPath, "utf8"));
const network = protocol.network ?? phase0.network ?? "testnet";
const horizonUrl = network === "testnet"
  ? "https://horizon-testnet.stellar.org"
  : fail(`unsupported network ${network}`);

const wallets = phase0.wallets;
const participants = Object.fromEntries(phase0.participants.map((p) => [p.key, p]));
const usdc = phase0.assets.usdc;
const dtcust10y = phase0.assets.demo.find((asset) => asset.key === "dtcust10y_ent");
if (!dtcust10y) {
  fail("missing dtcust10y entitlement in phase0 manifest");
}

const contracts = Object.fromEntries(
  Object.entries(protocol.contracts).map(([key, value]) => [key, value.contractId]),
);
const verifiers = Object.fromEntries(
  Object.entries(protocol.verifiers).map(([key, value]) => [key, value.contractId]),
);
const verifierIds = protocol.config.proofGateway.verifiers;
const proofSummary = viewContract(contracts.collateralPolicy, "get_policy_summary", []);
const currentLedger = fetchLatestLedger().sequence;

const gammaWallet = {
  name: `zkdtcc-${phase0.namespace}-gamma`,
  address: null,
};
const probeWallet = wallets.probe ?? {
  name: `zkdtcc-${phase0.namespace}-probe`,
  address: addressOf(`zkdtcc-${phase0.namespace}-probe`),
};

const gammaParticipant = {
  key: "gamma",
  role: 1,
  participantIdHash: hashObject({
    namespace: phase0.namespace,
    kind: "participant-id",
    key: "gamma",
  }),
  credentialRoot: hashObject({
    namespace: phase0.namespace,
    kind: "credential-root",
    key: "gamma",
  }),
  legalEntityHash: hashObject({
    namespace: phase0.namespace,
    kind: "legal-entity",
    key: "gamma",
  }),
  jurisdictionHash: hashObject({
    namespace: phase0.namespace,
    kind: "jurisdiction",
    key: "gamma",
  }),
  permissionsHash: participants.alpha.permissionsHash,
};
const issuerParticipant = {
  key: "issuer",
  role: 5,
  participantIdHash: hashObject({
    namespace: phase0.namespace,
    kind: "participant-id",
    key: "issuer",
  }),
  credentialRoot: hashObject({
    namespace: phase0.namespace,
    kind: "credential-root",
    key: "issuer",
  }),
  legalEntityHash: hashObject({
    namespace: phase0.namespace,
    kind: "legal-entity",
    key: "issuer",
  }),
  jurisdictionHash: hashObject({
    namespace: phase0.namespace,
    kind: "jurisdiction",
    key: "issuer",
  }),
};

const instrumentIdHash = hashObject({
  namespace: phase0.namespace,
  kind: "instrument-id",
  asset: dtcust10y.key,
});

const results = {
  phase0Path,
  protocolPath,
  network,
  namespace: phase0.namespace,
  startedAt: new Date().toISOString(),
  contracts,
  verifiers,
  steps: {},
};
let liveNonceCounter = 0;

async function main() {
  log("ensuring probe and gamma identities");
  ensureIdentityAccount(probeWallet.name, true);
  gammaWallet.address = await ensureGammaReady();
  log("wiring order pool instrument mapping and funding balances");
  await ensureInstrumentMapping();
  await ensureUsdcFunding();
  await normalizeTradingEnvironment();

  if (onlyStep === 7) {
    log("running direct step 7 encumbrance test");
    const alphaUnencumbered = await createUnencumberedReceipt(participants.alpha, "step7-direct-alpha-unenc");
    const encumbrance = await runStep7Encumbrance(alphaUnencumbered);
    results.steps.step7 = encumbrance.report;
    results.completedAt = new Date().toISOString();
    const reportPath = path.join(
      outDir,
      `step7-onchain-${phase0.namespace}-${timestampTag()}.json`,
    );
    writeFileSync(reportPath, JSON.stringify(results, null, 2));
    console.log(reportPath);
    return;
  }
  if (onlyStep === 8) {
    log("running direct step 8 order flow test");
    const proofFixtures = await createTradingProofFixtures("step8-direct");
    const orderFlow = await runStep8OrderFlow(proofFixtures.alpha, proofFixtures.beta);
    results.steps.step8 = orderFlow.report;
    return writeSingleStepReport("step8", results);
  }
  if (onlyStep === 9) {
    log("running direct step 9 settlement test");
    const proofFixtures = await createTradingProofFixtures("step9-direct");
    const orderFlow = await runStep8OrderFlow(proofFixtures.alpha, proofFixtures.beta);
    results.steps.step8 = orderFlow.report;
    results.steps.step9 = await runStep9DirectSettlement(orderFlow.execution);
    return writeSingleStepReport("step9", results);
  }
  if (onlyStep === 10) {
    log("running direct step 10 batch settlement test");
    const proofFixtures = await createTradingProofFixtures("step10-direct");
    results.steps.step10 = await runStep10BatchSettlement(
      proofFixtures.alpha,
      proofFixtures.beta,
      proofFixtures.gamma,
    );
    return writeSingleStepReport("step10", results);
  }
  if (onlyStep === 11) {
    log("running direct step 11 corporate actions test");
    results.steps.step11 = await runStep11CorporateActions();
    return writeSingleStepReport("step11", results);
  }
  if (onlyStep === 12) {
    log("running direct step 12 audit/freeze test");
    const proofFixtures = await createTradingProofFixtures("step12-direct");
    const orderFlow = await runStep8OrderFlow(proofFixtures.alpha, proofFixtures.beta);
    results.steps.step8 = orderFlow.report;
    results.steps.step12 = await runStep12AuditAndFreeze(orderFlow, proofFixtures.alpha);
    return writeSingleStepReport("step12", results);
  }

  log("running step 5 compliance gates");
  results.steps.step5 = await runStep5ComplianceGates();
  log("running step 6 proof gateway checks");
  const proofFixtures = await runStep6ProofGatewayChecks();
  results.steps.step6 = proofFixtures.stepReport;

  log("running step 7 encumbrance");
  const encumbrance = await runStep7Encumbrance(proofFixtures.alpha.unencumbered);
  results.steps.step7 = encumbrance.report;

  log("running step 8 order flow");
  const orderFlow = await runStep8OrderFlow(proofFixtures.alpha, proofFixtures.beta);
  results.steps.step8 = orderFlow.report;

  log("running step 9 direct settlement");
  results.steps.step9 = await runStep9DirectSettlement(orderFlow.execution);
  log("running step 10 batch settlement");
  results.steps.step10 = await runStep10BatchSettlement(
    proofFixtures.alpha,
    proofFixtures.beta,
    proofFixtures.gamma,
  );
  log("running step 11 corporate actions");
  results.steps.step11 = await runStep11CorporateActions();
  log("running step 12 audit and freeze");
  results.steps.step12 = await runStep12AuditAndFreeze(orderFlow, proofFixtures.alpha);

  results.completedAt = new Date().toISOString();
  const reportPath = path.join(
    outDir,
    `steps5-12-onchain-${phase0.namespace}-${timestampTag()}.json`,
  );
  writeFileSync(reportPath, JSON.stringify(results, null, 2));
  console.log(reportPath);
}

function writeSingleStepReport(stepName, payload) {
  payload.completedAt = new Date().toISOString();
  const reportPath = path.join(
    outDir,
    `${stepName}-onchain-${phase0.namespace}-${timestampTag()}.json`,
  );
  writeFileSync(reportPath, JSON.stringify(payload, null, 2));
  console.log(reportPath);
}

async function ensureGammaReady() {
  ensureIdentityAccount(gammaWallet.name, true);
  const address = addressOf(gammaWallet.name);
  gammaWallet.address = address;

  if (!hasTrustline(address, usdc.assetString)) {
    ensureTrustline(gammaWallet.name, usdc.assetString);
  }
  if (!hasTrustline(address, dtcust10y.assetString)) {
    ensureTrustline(gammaWallet.name, dtcust10y.assetString);
  }
  if (!isTrustlineAuthorized(gammaWallet.address, dtcust10y.assetString)) {
    authorizeTrustline(dtcust10y.assetString, gammaWallet.address);
  }

  if (!hasParticipant(gammaParticipant.participantIdHash)) {
    invokeContract(
      phase0.contracts.participantRegistry.contractId,
      "register_participant",
      [
        "--operator",
        wallets.compliance.address,
        "--participant_id_hash",
        gammaParticipant.participantIdHash,
        "--primary_wallet",
        gammaWallet.address,
        "--role",
        String(gammaParticipant.role),
        "--credential_root",
        gammaParticipant.credentialRoot,
        "--legal_entity_hash",
        gammaParticipant.legalEntityHash,
        "--jurisdiction_hash",
        gammaParticipant.jurisdictionHash,
      ],
      { sourceName: wallets.compliance.name },
    );
    invokeContract(
      phase0.contracts.participantRegistry.contractId,
      "set_permissions_hash",
      [
        "--operator",
        wallets.compliance.address,
        "--participant_id_hash",
        gammaParticipant.participantIdHash,
        "--permissions_hash",
        gammaParticipant.permissionsHash,
      ],
      { sourceName: wallets.compliance.name },
    );
  }

  return address;
}

async function normalizeTradingEnvironment() {
  ensureParticipantRecord(issuerParticipant, wallets.issuer.address);
  const restoredExpiry = fetchLatestLedger().sequence + 500_000;
  setGlobalPause(
    false,
    hashObject({ kind: "normalize", field: "global-unpause" }),
    hashObject({ kind: "normalize", field: "global-case" }),
  );
  setAssetPause(
    dtcust10y.sacContractId,
    false,
    hashObject({ kind: "normalize", field: "asset-unpause" }),
    hashObject({ kind: "normalize", field: "asset-case" }),
  );
  for (const participantIdHash of [
    participants.alpha.participantIdHash,
    participants.beta.participantIdHash,
    gammaParticipant.participantIdHash,
  ]) {
    setParticipantFreeze(
      participantIdHash,
      false,
      hashObject({ kind: "normalize", participantIdHash, field: "unfreeze-reason" }),
      hashObject({ kind: "normalize", participantIdHash, field: "unfreeze-case" }),
    );
    setComplianceState(
      participantIdHash,
      1,
      1,
      restoredExpiry,
      hashObject({ kind: "normalize", participantIdHash, field: "review-case" }),
    );
  }
}

function ensureParticipantRecord(participant, primaryWallet) {
  if (hasParticipant(participant.participantIdHash)) {
    return;
  }
  invokeContract(
    phase0.contracts.participantRegistry.contractId,
    "register_participant",
    [
      "--operator",
      wallets.compliance.address,
      "--participant_id_hash",
      participant.participantIdHash,
      "--primary_wallet",
      primaryWallet,
      "--role",
      String(participant.role),
      "--credential_root",
      participant.credentialRoot,
      "--legal_entity_hash",
      participant.legalEntityHash,
      "--jurisdiction_hash",
      participant.jurisdictionHash,
    ],
    { sourceName: wallets.compliance.name },
  );
}

function ensureInstrumentMapping() {
  invokeContract(
    contracts.orderCommitPool,
    "set_instrument_asset",
    [
      "--operator",
      wallets.compliance.address,
      "--instrument_id_hash",
      instrumentIdHash,
      "--asset",
      dtcust10y.sacContractId,
    ],
    { sourceName: wallets.compliance.name },
  );
}

function ensureUsdcFunding() {
  const alphaBalance = BigInt(stripQuotes(String(viewContract(
    usdc.sacContractId,
    "balance",
    ["--id", wallets.alpha.address],
  ))));
  const gammaBalance = BigInt(stripQuotes(String(viewContract(
    usdc.sacContractId,
    "balance",
    ["--id", gammaWallet.address],
  ))));
  const alphaTarget = 12_000_000n;
  const gammaTarget = 6_000_000n;

  if (alphaBalance < alphaTarget) {
    payClassicAsset(
      wallets.treasury.name,
      wallets.alpha.address,
      usdc.assetString,
      alphaTarget - alphaBalance,
    );
  }
  if (gammaBalance < gammaTarget) {
    payClassicAsset(
      wallets.treasury.name,
      gammaWallet.address,
      usdc.assetString,
      gammaTarget - gammaBalance,
    );
  }
}

async function runStep5ComplianceGates() {
  const cases = [];
  const randomCommitment = hashObject({ kind: "step5", case: "order-commitment" });
  const randomReceipt = hashObject({ kind: "step5", case: "receipt" });
  const randomNullifier = hashObject({ kind: "step5", case: "nullifier" });
  const expiryLedger = String(fetchLatestLedger().sequence + 20_000);

  cases.push(await expectFailure("unregistered wallet rejection", () =>
    invokeContract(
      contracts.orderCommitPool,
      "commit_order",
      [
        "--submitter",
        probeWallet.address,
        "--participant_id_hash",
        participants.alpha.participantIdHash,
        "--instrument_id_hash",
        instrumentIdHash,
        "--batch_id",
        hashObject({ kind: "step5", case: "probe-batch" }),
        "--side",
        "1",
        "--order_commitment",
        randomCommitment,
        "--collateral_proof_receipt_id",
        randomReceipt,
        "--encumbrance_proof_receipt_id",
        randomReceipt,
        "--cancel_nullifier",
        randomNullifier,
        "--expiry_ledger",
        expiryLedger,
      ],
      { sourceName: probeWallet.name },
    ),
  ));

  setParticipantFreeze(
    participants.beta.participantIdHash,
    true,
    hashObject({ kind: "step5", case: "freeze-reason" }),
    hashObject({ kind: "step5", case: "freeze-case" }),
  );
  cases.push(await expectFailure("frozen participant rejection", () =>
    invokeContract(
      contracts.orderCommitPool,
      "commit_order",
      [
        "--submitter",
        wallets.beta.address,
        "--participant_id_hash",
        participants.beta.participantIdHash,
        "--instrument_id_hash",
        instrumentIdHash,
        "--batch_id",
        hashObject({ kind: "step5", case: "frozen-batch" }),
        "--side",
        "2",
        "--order_commitment",
        hashObject({ kind: "step5", case: "frozen-order" }),
        "--collateral_proof_receipt_id",
        randomReceipt,
        "--encumbrance_proof_receipt_id",
        randomReceipt,
        "--cancel_nullifier",
        hashObject({ kind: "step5", case: "frozen-nullifier" }),
        "--expiry_ledger",
        expiryLedger,
      ],
      { sourceName: wallets.beta.name },
    ),
  ));
  setParticipantFreeze(
    participants.beta.participantIdHash,
    false,
    hashObject({ kind: "step5", case: "unfreeze-reason" }),
    hashObject({ kind: "step5", case: "unfreeze-case" }),
  );

  setAssetPause(
    dtcust10y.sacContractId,
    true,
    hashObject({ kind: "step5", case: "pause-reason" }),
    hashObject({ kind: "step5", case: "pause-case" }),
  );
  cases.push(await expectFailure("paused asset rejection", () =>
    invokeContract(
      contracts.orderCommitPool,
      "commit_order",
      [
        "--submitter",
        wallets.alpha.address,
        "--participant_id_hash",
        participants.alpha.participantIdHash,
        "--instrument_id_hash",
        instrumentIdHash,
        "--batch_id",
        hashObject({ kind: "step5", case: "paused-batch" }),
        "--side",
        "1",
        "--order_commitment",
        hashObject({ kind: "step5", case: "paused-order" }),
        "--collateral_proof_receipt_id",
        randomReceipt,
        "--encumbrance_proof_receipt_id",
        randomReceipt,
        "--cancel_nullifier",
        hashObject({ kind: "step5", case: "paused-nullifier" }),
        "--expiry_ledger",
        expiryLedger,
      ],
      { sourceName: wallets.alpha.name },
    ),
  ));
  setAssetPause(
    dtcust10y.sacContractId,
    false,
    hashObject({ kind: "step5", case: "unpause-reason" }),
    hashObject({ kind: "step5", case: "unpause-case" }),
  );

  setGlobalPause(
    true,
    hashObject({ kind: "step5", case: "global-reason" }),
    hashObject({ kind: "step5", case: "global-case" }),
  );
  cases.push(await expectFailure("global pause rejection", () =>
    invokeContract(
      contracts.orderCommitPool,
      "commit_order",
      [
        "--submitter",
        wallets.alpha.address,
        "--participant_id_hash",
        participants.alpha.participantIdHash,
        "--instrument_id_hash",
        instrumentIdHash,
        "--batch_id",
        hashObject({ kind: "step5", case: "global-batch" }),
        "--side",
        "1",
        "--order_commitment",
        hashObject({ kind: "step5", case: "global-order" }),
        "--collateral_proof_receipt_id",
        randomReceipt,
        "--encumbrance_proof_receipt_id",
        randomReceipt,
        "--cancel_nullifier",
        hashObject({ kind: "step5", case: "global-nullifier" }),
        "--expiry_ledger",
        expiryLedger,
      ],
      { sourceName: wallets.alpha.name },
    ),
  ));
  setGlobalPause(
    false,
    hashObject({ kind: "step5", case: "unglobal-reason" }),
    hashObject({ kind: "step5", case: "unglobal-case" }),
  );

  const expiredLedger = fetchLatestLedger().sequence - 1;
  setComplianceState(
    participants.beta.participantIdHash,
    1,
    1,
    expiredLedger,
    hashObject({ kind: "step5", case: "expired-review" }),
  );
  cases.push(await expectFailure("expired compliance rejection", () =>
    invokeContract(
      contracts.orderCommitPool,
      "commit_order",
      [
        "--submitter",
        wallets.beta.address,
        "--participant_id_hash",
        participants.beta.participantIdHash,
        "--instrument_id_hash",
        instrumentIdHash,
        "--batch_id",
        hashObject({ kind: "step5", case: "expired-batch" }),
        "--side",
        "2",
        "--order_commitment",
        hashObject({ kind: "step5", case: "expired-order" }),
        "--collateral_proof_receipt_id",
        randomReceipt,
        "--encumbrance_proof_receipt_id",
        randomReceipt,
        "--cancel_nullifier",
        hashObject({ kind: "step5", case: "expired-nullifier" }),
        "--expiry_ledger",
        expiryLedger,
      ],
      { sourceName: wallets.beta.name },
    ),
  ));
  setComplianceState(
    participants.beta.participantIdHash,
    1,
    1,
    fetchLatestLedger().sequence + 500_000,
    hashObject({ kind: "step5", case: "restored-review" }),
  );

  return { cases };
}

async function runStep6ProofGatewayChecks() {
  const stepReport = { checks: [] };
  const alpha = {
    collateral: await createCollateralReceipt(participants.alpha, "step6-alpha-collateral"),
    unencumbered: await createUnencumberedReceipt(participants.alpha, "step6-alpha-unenc"),
  };
  const beta = {
    collateral: await createCollateralReceipt(participants.beta, "step6-beta-collateral"),
    unencumbered: await createUnencumberedReceipt(participants.beta, "step6-beta-unenc"),
  };
  const gammaLiveParticipant = {
    ...gammaParticipant,
    wallet: gammaWallet,
  };
  const gamma = {
    collateral: await createCollateralReceipt(gammaLiveParticipant, "step6-gamma-collateral"),
    unencumbered: await createUnencumberedReceipt(gammaLiveParticipant, "step6-gamma-unenc"),
  };

  stepReport.checks.push({
    name: "valid collateral receipt",
    receiptId: alpha.collateral.receipt.receipt_id,
  });

  stepReport.checks.push(await expectFailure("reused nonce rejection", () =>
    submitProofReceipt(
      participants.alpha,
      "2",
      verifierIds.collateralSufficiency,
      alpha.collateral.bundle.portfolioCommitmentHex,
      alpha.collateral.nonceHex,
      alpha.collateral.expiryLedger,
      alpha.collateral.bundle.proofPayloadHex,
    ),
  ));

  stepReport.checks.push(await expectFailure("wrong participant binding rejection", () =>
    submitProofReceipt(
      {
        ...participants.beta,
        wallet: wallets.beta,
      },
      "2",
      verifierIds.collateralSufficiency,
      alpha.collateral.bundle.portfolioCommitmentHex,
      hashObject({ kind: "step6", case: "wrong-binding-nonce" }),
      alpha.collateral.expiryLedger,
      alpha.collateral.bundle.proofPayloadHex,
      participants.alpha.participantIdHash,
    ),
  ));

  const revoked = invokeContract(
    contracts.proofGateway,
    "revoke_receipt",
    [
      "--operator",
      wallets.compliance.address,
      "--receipt_id",
      alpha.collateral.receipt.receipt_id,
      "--reason_code",
      hashObject({ kind: "step6", case: "revoke-reason" }),
      "--case_id",
      hashObject({ kind: "step6", case: "revoke-case" }),
    ],
    { sourceName: wallets.compliance.name },
  );
  const usableAfterRevoke = viewContract(
    contracts.proofGateway,
    "is_receipt_usable",
    ["--receipt_id", alpha.collateral.receipt.receipt_id],
  );
  stepReport.checks.push({
    name: "revoked receipt unusable",
    txHash: revoked.txHash,
    usableAfterRevoke,
  });

  invokeContract(
    contracts.proofGateway,
    "set_verifier_policy",
    [
      "--operator",
      wallets.compliance.address,
      "--verifier_id",
      verifierIds.collateralSufficiency.verifierId,
      "--enabled",
      "false",
      "--valid_from_ledger",
      String(protocol.config.proofGateway.validFromLedger),
      "--valid_until_ledger",
      String(protocol.config.proofGateway.validUntilLedger),
      "--policy_cutoff_hash",
      verifierIds.collateralSufficiency.policyCutoffHash,
    ],
    { sourceName: wallets.compliance.name },
  );
  stepReport.checks.push(await expectFailure("disabled verifier policy rejection", async () => {
    const pending = await createCollateralBundleOnly(participants.beta, "step6-disabled-collateral");
    return submitProofReceipt(
      participants.beta,
      "2",
      verifierIds.collateralSufficiency,
      pending.bundle.portfolioCommitmentHex,
      pending.nonceHex,
      pending.expiryLedger,
      pending.bundle.proofPayloadHex,
    );
  }));
  invokeContract(
    contracts.proofGateway,
    "set_verifier_policy",
    [
      "--operator",
      wallets.compliance.address,
      "--verifier_id",
      verifierIds.collateralSufficiency.verifierId,
      "--enabled",
      "true",
      "--valid_from_ledger",
      String(protocol.config.proofGateway.validFromLedger),
      "--valid_until_ledger",
      String(protocol.config.proofGateway.validUntilLedger),
      "--policy_cutoff_hash",
      verifierIds.collateralSufficiency.policyCutoffHash,
    ],
    { sourceName: wallets.compliance.name },
  );

  return { stepReport, alpha, beta, gamma };
}

async function createTradingProofFixtures(labelPrefix) {
  const gammaLiveParticipant = {
    ...gammaParticipant,
    wallet: gammaWallet,
  };
  return {
    alpha: {
      collateral: await createCollateralReceipt(participants.alpha, `${labelPrefix}-alpha-collateral`),
      unencumbered: await createUnencumberedReceipt(participants.alpha, `${labelPrefix}-alpha-unenc`),
    },
    beta: {
      collateral: await createCollateralReceipt(participants.beta, `${labelPrefix}-beta-collateral`),
      unencumbered: await createUnencumberedReceipt(participants.beta, `${labelPrefix}-beta-unenc`),
    },
    gamma: {
      collateral: await createCollateralReceipt(gammaLiveParticipant, `${labelPrefix}-gamma-collateral`),
      unencumbered: await createUnencumberedReceipt(gammaLiveParticipant, `${labelPrefix}-gamma-unenc`),
    },
  };
}

async function runStep7Encumbrance(alphaUnencumbered) {
  const participant = participants.alpha;
  const scopeHash = hashObject({ kind: "step7", scope: "alpha-dtcust10y" });
  const liveLedger = fetchLatestLedger().sequence;
  const issuedAtLedger = Math.max(1, liveLedger - 5);
  const expiryLedger = liveLedger + 50_000;
  const runTag = hashObject({
    kind: "step7",
    receipt: alphaUnencumbered.receipt.receipt_id,
    ledger: liveLedger,
  });
  const attestationHash = viewContract(
    contracts.encumbranceRegistry,
    "build_attestation_hash",
    [
      "--attestor_id",
      protocol.attestor.attestorId,
      "--participant_id_hash",
      participant.participantIdHash,
      "--asset",
      dtcust10y.sacContractId,
      "--availability_root",
      alphaUnencumbered.bundle.availabilityRootHex,
      "--scope_hash",
      scopeHash,
      "--issued_at_ledger",
      String(issuedAtLedger),
      "--expiry_ledger",
      String(expiryLedger),
    ],
  );
  const signature = signDigestHex(protocol.attestor.name, attestationHash);

  const attestation = invokeContract(
    contracts.encumbranceRegistry,
    "publish_attestation",
    [
      "--attestor_id",
      protocol.attestor.attestorId,
      "--participant_id_hash",
      participant.participantIdHash,
      "--asset",
      dtcust10y.sacContractId,
      "--availability_root",
      alphaUnencumbered.bundle.availabilityRootHex,
      "--scope_hash",
      scopeHash,
      "--issued_at_ledger",
      String(issuedAtLedger),
      "--expiry_ledger",
      String(expiryLedger),
      "--signature",
      signature,
    ],
    { sourceName: wallets.compliance.name },
  );
  const attestationRecord = parseJsonish(attestation.raw);
  const attestationState = waitForState(
    "step7 attestation visibility",
    () => viewContract(
      contracts.encumbranceRegistry,
      "get_attestation",
      ["--attestation_id", attestationRecord.attestation_id],
    ),
  );
  waitForLedgerAtLeast(
    "step7 attestation finality",
    Number(attestationState.recorded_ledger ?? issuedAtLedger) + 1,
  );
  const lotNullifier = hashObject({ kind: "step7", runTag, lot: "alpha-1" });
  const releaseReference = hashObject({ kind: "step7", runTag, release: "alpha-1" });
  const lock = invokeWithDependencyRetry(
    contracts.encumbranceRegistry,
    "lock_lot",
    [
      "--submitter",
      wallets.alpha.address,
      "--participant_id_hash",
      participant.participantIdHash,
      "--asset",
      dtcust10y.sacContractId,
      "--attestation_id",
      attestationRecord.attestation_id,
      "--proof_receipt_id",
      alphaUnencumbered.receipt.receipt_id,
      "--lot_nullifier",
      lotNullifier,
      "--scope_hash",
      scopeHash,
      "--reason_hash",
      hashObject({ kind: "step7", runTag, reason: "hold" }),
      "--quantity",
      "10000000",
      "--expiry_ledger",
      String(fetchLatestLedger().sequence + 20_000),
    ],
    { sourceName: wallets.alpha.name },
    {
      retryErrorFragment: "Error(Contract, #7)",
      dependencyLabel: "step7 attestation dependency",
      dependencyFn: () => viewContract(
        contracts.encumbranceRegistry,
        "get_attestation",
        ["--attestation_id", attestationRecord.attestation_id],
      ),
    },
  );
  const lockRecord = parseJsonish(lock.raw);
  const lockState = waitForState(
    "step7 lock visibility",
    () => viewContract(
      contracts.encumbranceRegistry,
      "get_lock",
      ["--lot_nullifier", lockRecord.lot_nullifier],
    ),
  );
  waitForLedgerAtLeast(
    "step7 lock finality",
    Number(lockState.updated_ledger ?? lockState.created_ledger ?? liveLedger) + 1,
  );
  const released = invokeWithDependencyRetry(
    contracts.encumbranceRegistry,
    "release_lot",
    [
      "--actor",
      wallets.alpha.address,
      "--lot_nullifier",
      lockRecord.lot_nullifier,
      "--release_reference",
      releaseReference,
    ],
    { sourceName: wallets.alpha.name },
    {
      retryErrorFragment: "Error(Contract, #13)",
      dependencyLabel: "step7 lock dependency",
      dependencyFn: () => viewContract(
        contracts.encumbranceRegistry,
        "get_lock",
        ["--lot_nullifier", lockRecord.lot_nullifier],
      ),
    },
  );

  return {
    report: {
      attestationId: attestationRecord.attestation_id,
      lockId: lockRecord.lock_id,
      txHashes: {
        publishAttestation: attestation.txHash,
        lockLot: lock.txHash,
        releaseLot: released.txHash,
      },
    },
  };
}

async function runStep8OrderFlow(alphaProofs, betaProofs) {
  const runTag = hashObject({ kind: "step8", ledger: fetchLatestLedger().sequence, now_ms: Date.now() });
  const batchId = hashObject({ kind: "step8", runTag, batch: "match-1" });
  const cancelOrder = invokeContract(
    contracts.orderCommitPool,
    "commit_order",
    [
      "--submitter",
      wallets.alpha.address,
      "--participant_id_hash",
      participants.alpha.participantIdHash,
      "--instrument_id_hash",
      instrumentIdHash,
      "--batch_id",
      hashObject({ kind: "step8", runTag, batch: "cancel" }),
      "--side",
      "1",
      "--order_commitment",
      hashObject({ kind: "step8", runTag, order: "cancel" }),
      "--collateral_proof_receipt_id",
      alphaProofs.collateral.receipt.receipt_id,
      "--encumbrance_proof_receipt_id",
      alphaProofs.unencumbered.receipt.receipt_id,
      "--cancel_nullifier",
      hashObject({ kind: "step8", runTag, cancel: "slot" }),
      "--expiry_ledger",
      String(fetchLatestLedger().sequence + 20_000),
    ],
    { sourceName: wallets.alpha.name },
  );
  const cancelRecord = parseJsonish(cancelOrder.raw);
  const cancelled = invokeContract(
    contracts.orderCommitPool,
    "cancel_order",
    [
      "--submitter",
      wallets.alpha.address,
      "--order_id",
      cancelRecord.order_id,
      "--cancel_nullifier",
      cancelRecord.cancel_nullifier,
    ],
    { sourceName: wallets.alpha.name },
  );
  const reusedCancel = await expectFailure("reused cancel nullifier rejection", () =>
    invokeContract(
      contracts.orderCommitPool,
      "cancel_order",
      [
        "--submitter",
        wallets.alpha.address,
        "--order_id",
        cancelRecord.order_id,
        "--cancel_nullifier",
        cancelRecord.cancel_nullifier,
      ],
      { sourceName: wallets.alpha.name },
    ),
  );

  const privateMatch = await createPrivateMatchReceipt(
    participants.alpha,
    participants.beta,
    alphaProofs,
    betaProofs,
    "step8-private-match",
  );

  const bidOrder = invokeContract(
    contracts.orderCommitPool,
    "commit_order",
    [
      "--submitter",
      wallets.alpha.address,
      "--participant_id_hash",
      participants.alpha.participantIdHash,
      "--instrument_id_hash",
      instrumentIdHash,
      "--batch_id",
      batchId,
      "--side",
      "1",
      "--order_commitment",
      privateMatch.bundle.bidOrderCommitmentHex,
      "--collateral_proof_receipt_id",
      alphaProofs.collateral.receipt.receipt_id,
      "--encumbrance_proof_receipt_id",
      alphaProofs.unencumbered.receipt.receipt_id,
      "--cancel_nullifier",
      hashObject({ kind: "step8", runTag, cancel: "bid" }),
      "--expiry_ledger",
      String(privateMatch.expiryLedger),
    ],
    { sourceName: wallets.alpha.name },
  );
  const askOrder = invokeContract(
    contracts.orderCommitPool,
    "commit_order",
    [
      "--submitter",
      wallets.beta.address,
      "--participant_id_hash",
      participants.beta.participantIdHash,
      "--instrument_id_hash",
      instrumentIdHash,
      "--batch_id",
      batchId,
      "--side",
      "2",
      "--order_commitment",
      privateMatch.bundle.askOrderCommitmentHex,
      "--collateral_proof_receipt_id",
      betaProofs.collateral.receipt.receipt_id,
      "--encumbrance_proof_receipt_id",
      betaProofs.unencumbered.receipt.receipt_id,
      "--cancel_nullifier",
      hashObject({ kind: "step8", runTag, cancel: "ask" }),
      "--expiry_ledger",
      String(privateMatch.expiryLedger),
    ],
    { sourceName: wallets.beta.name },
  );
  const bidRecord = parseJsonish(bidOrder.raw);
  const askRecord = parseJsonish(askOrder.raw);
  const execution = invokeContract(
    contracts.orderCommitPool,
    "match_orders",
    [
      "--matcher",
      wallets.matcher.address,
      "--verifier_id",
      verifierIds.privateMatch.verifierId,
      "--proof_receipt_id",
      privateMatch.receipt.receipt_id,
      "--bid_order_id",
      bidRecord.order_id,
      "--ask_order_id",
      askRecord.order_id,
      "--execution_commitment",
      privateMatch.bundle.executionCommitmentHex,
      "--encrypted_receipt_hash",
      hashObject({ kind: "step8", runTag, exec: "encrypted-receipt" }),
      "--bid_execution_nullifier",
      hashObject({ kind: "step8", runTag, exec: "bid-nullifier" }),
      "--ask_execution_nullifier",
      hashObject({ kind: "step8", runTag, exec: "ask-nullifier" }),
    ],
    { sourceName: wallets.matcher.name },
  );
  const executionRecord = parseJsonish(execution.raw);

  return {
    execution: executionRecord,
    report: {
      cancelledOrderId: cancelRecord.order_id,
      executionId: executionRecord.execution_id,
      txHashes: {
        commitCancelledOrder: cancelOrder.txHash,
        cancelOrder: cancelled.txHash,
        commitBid: bidOrder.txHash,
        commitAsk: askOrder.txHash,
        matchOrders: execution.txHash,
      },
      expectedFailures: [reusedCancel],
    },
  };
}

async function runStep9DirectSettlement(execution) {
  const before = readBalances([wallets.alpha.address, wallets.beta.address], [
    usdc.sacContractId,
    dtcust10y.sacContractId,
  ]);
  approveToken(dtcust10y.sacContractId, wallets.beta, contracts.settlementNettingEngine, "10000000");
  approveToken(usdc.sacContractId, wallets.alpha, contracts.settlementNettingEngine, "2000000");
  const settled = invokeContract(
    contracts.settlementNettingEngine,
    "settle_execution_dvp",
    [
      "--settler",
      wallets.settler.address,
      "--execution_id",
      execution.execution_id,
      "--trade_nullifier",
      hashObject({ kind: "step9", trade: "nullifier" }),
      "--cash_asset",
      usdc.sacContractId,
      "--asset_amount",
      "10000000",
      "--cash_amount",
      "2000000",
    ],
    { sourceName: wallets.settler.name },
  );
  const after = readBalances([wallets.alpha.address, wallets.beta.address], [
    usdc.sacContractId,
    dtcust10y.sacContractId,
  ]);
  const settlement = parseJsonish(settled.raw);
  return {
    settlementId: settlement.settlement_id,
    txHash: settled.txHash,
    balancesBefore: before,
    balancesAfter: after,
  };
}

async function runStep10BatchSettlement(alphaProofs, betaProofs, gammaProofs) {
  const runTag = hashObject({ kind: "step10", ledger: fetchLatestLedger().sequence, now_ms: Date.now() });
  const options = {
    participantAIdHashHex: with0x(participants.alpha.participantIdHash),
    participantBIdHashHex: with0x(participants.beta.participantIdHash),
    participantCIdHashHex: with0x(gammaParticipant.participantIdHash),
    instrumentIdHashHex: with0x(instrumentIdHash),
    tradeABidCollateralProofReceiptIdHex: with0x(alphaProofs.collateral.receipt.receipt_id),
    tradeABidEncumbranceProofReceiptIdHex: with0x(alphaProofs.unencumbered.receipt.receipt_id),
    tradeAAskCollateralProofReceiptIdHex: with0x(betaProofs.collateral.receipt.receipt_id),
    tradeAAskEncumbranceProofReceiptIdHex: with0x(betaProofs.unencumbered.receipt.receipt_id),
    tradeBBidCollateralProofReceiptIdHex: with0x(gammaProofs.collateral.receipt.receipt_id),
    tradeBBidEncumbranceProofReceiptIdHex: with0x(gammaProofs.unencumbered.receipt.receipt_id),
    tradeBAskCollateralProofReceiptIdHex: with0x(alphaProofs.collateral.receipt.receipt_id),
    tradeBAskEncumbranceProofReceiptIdHex: with0x(alphaProofs.unencumbered.receipt.receipt_id),
  };
  const batchProof = await createBatchNettingReceipt(options, "step10-batch");

  const tradeAMatch = await createPrivateMatchReceiptFromBatchTrade("A", options, batchProof.bundle);
  const tradeBMatch = await createPrivateMatchReceiptFromBatchTrade("B", options, batchProof.bundle);

  const orderABid = invokeContract(
    contracts.orderCommitPool,
    "commit_order",
    [
      "--submitter",
      wallets.alpha.address,
      "--participant_id_hash",
      participants.alpha.participantIdHash,
      "--instrument_id_hash",
      instrumentIdHash,
      "--batch_id",
      batchProof.bundle.batchIdHex,
      "--side",
      "1",
      "--order_commitment",
      batchProof.bundle.tradeA.bidOrderCommitmentHex,
      "--collateral_proof_receipt_id",
      alphaProofs.collateral.receipt.receipt_id,
      "--encumbrance_proof_receipt_id",
      alphaProofs.unencumbered.receipt.receipt_id,
      "--cancel_nullifier",
      hashObject({ kind: "step10", runTag, order: "a-bid" }),
      "--expiry_ledger",
      String(batchProof.expiryLedger),
    ],
    { sourceName: wallets.alpha.name },
  );
  const orderAAsk = invokeContract(
    contracts.orderCommitPool,
    "commit_order",
    [
      "--submitter",
      wallets.beta.address,
      "--participant_id_hash",
      participants.beta.participantIdHash,
      "--instrument_id_hash",
      instrumentIdHash,
      "--batch_id",
      batchProof.bundle.batchIdHex,
      "--side",
      "2",
      "--order_commitment",
      batchProof.bundle.tradeA.askOrderCommitmentHex,
      "--collateral_proof_receipt_id",
      betaProofs.collateral.receipt.receipt_id,
      "--encumbrance_proof_receipt_id",
      betaProofs.unencumbered.receipt.receipt_id,
      "--cancel_nullifier",
      hashObject({ kind: "step10", runTag, order: "a-ask" }),
      "--expiry_ledger",
      String(batchProof.expiryLedger),
    ],
    { sourceName: wallets.beta.name },
  );
  const orderBBid = invokeContract(
    contracts.orderCommitPool,
    "commit_order",
    [
      "--submitter",
      gammaWallet.address,
      "--participant_id_hash",
      gammaParticipant.participantIdHash,
      "--instrument_id_hash",
      instrumentIdHash,
      "--batch_id",
      batchProof.bundle.batchIdHex,
      "--side",
      "1",
      "--order_commitment",
      batchProof.bundle.tradeB.bidOrderCommitmentHex,
      "--collateral_proof_receipt_id",
      gammaProofs.collateral.receipt.receipt_id,
      "--encumbrance_proof_receipt_id",
      gammaProofs.unencumbered.receipt.receipt_id,
      "--cancel_nullifier",
      hashObject({ kind: "step10", runTag, order: "b-bid" }),
      "--expiry_ledger",
      String(batchProof.expiryLedger),
    ],
    { sourceName: gammaWallet.name },
  );
  const orderBAsk = invokeContract(
    contracts.orderCommitPool,
    "commit_order",
    [
      "--submitter",
      wallets.alpha.address,
      "--participant_id_hash",
      participants.alpha.participantIdHash,
      "--instrument_id_hash",
      instrumentIdHash,
      "--batch_id",
      batchProof.bundle.batchIdHex,
      "--side",
      "2",
      "--order_commitment",
      batchProof.bundle.tradeB.askOrderCommitmentHex,
      "--collateral_proof_receipt_id",
      alphaProofs.collateral.receipt.receipt_id,
      "--encumbrance_proof_receipt_id",
      alphaProofs.unencumbered.receipt.receipt_id,
      "--cancel_nullifier",
      hashObject({ kind: "step10", runTag, order: "b-ask" }),
      "--expiry_ledger",
      String(batchProof.expiryLedger),
    ],
    { sourceName: wallets.alpha.name },
  );

  const aBid = parseJsonish(orderABid.raw);
  const aAsk = parseJsonish(orderAAsk.raw);
  const bBid = parseJsonish(orderBBid.raw);
  const bAsk = parseJsonish(orderBAsk.raw);

  const executionA = invokeContract(
    contracts.orderCommitPool,
    "match_orders",
    [
      "--matcher",
      wallets.matcher.address,
      "--verifier_id",
      verifierIds.privateMatch.verifierId,
      "--proof_receipt_id",
      tradeAMatch.receipt.receipt_id,
      "--bid_order_id",
      aBid.order_id,
      "--ask_order_id",
      aAsk.order_id,
      "--execution_commitment",
      batchProof.bundle.executionACommitmentHex,
      "--encrypted_receipt_hash",
      hashObject({ kind: "step10", runTag, exec: "a-encrypted-receipt" }),
      "--bid_execution_nullifier",
      hashObject({ kind: "step10", runTag, exec: "a-bid-nullifier" }),
      "--ask_execution_nullifier",
      hashObject({ kind: "step10", runTag, exec: "a-ask-nullifier" }),
    ],
    { sourceName: wallets.matcher.name },
  );
  const executionB = invokeContract(
    contracts.orderCommitPool,
    "match_orders",
    [
      "--matcher",
      wallets.matcher.address,
      "--verifier_id",
      verifierIds.privateMatch.verifierId,
      "--proof_receipt_id",
      tradeBMatch.receipt.receipt_id,
      "--bid_order_id",
      bBid.order_id,
      "--ask_order_id",
      bAsk.order_id,
      "--execution_commitment",
      batchProof.bundle.executionBCommitmentHex,
      "--encrypted_receipt_hash",
      hashObject({ kind: "step10", runTag, exec: "b-encrypted-receipt" }),
      "--bid_execution_nullifier",
      hashObject({ kind: "step10", runTag, exec: "b-bid-nullifier" }),
      "--ask_execution_nullifier",
      hashObject({ kind: "step10", runTag, exec: "b-ask-nullifier" }),
    ],
    { sourceName: wallets.matcher.name },
  );
  const execA = parseJsonish(executionA.raw);
  const execB = parseJsonish(executionB.raw);

  approveToken(dtcust10y.sacContractId, wallets.beta, contracts.settlementNettingEngine, "6000000");
  approveToken(dtcust10y.sacContractId, wallets.alpha, contracts.settlementNettingEngine, "4000000");
  approveToken(usdc.sacContractId, wallets.alpha, contracts.settlementNettingEngine, "9000000");
  approveToken(usdc.sacContractId, gammaWallet, contracts.settlementNettingEngine, "5000000");

  const before = readBalances(
    [wallets.alpha.address, wallets.beta.address, gammaWallet.address],
    [usdc.sacContractId, dtcust10y.sacContractId],
  );
  const settled = invokeContract(
    contracts.settlementNettingEngine,
    "settle_batch",
    [
      "--settler",
      wallets.settler.address,
      "--verifier_id",
      verifierIds.batchNetting.verifierId,
      "--proof_receipt_id",
      batchProof.receipt.receipt_id,
      "--settlement_commitment",
      batchProof.bundle.settlementCommitmentHex,
      "--net_vector_hash",
      batchProof.bundle.netVectorHashHex,
      "--execution_a_id",
      execA.execution_id,
      "--execution_b_id",
      execB.execution_id,
      "--trade_nullifier_a",
      batchProof.bundle.tradeNullifierAHex,
      "--trade_nullifier_b",
      batchProof.bundle.tradeNullifierBHex,
    ],
    { sourceName: wallets.settler.name },
  );
  const settlementRecord = parseJsonish(settled.raw);
  const applied = invokeContract(
    contracts.settlementNettingEngine,
    "apply_batch_transfers",
    [
      "--settler",
      wallets.settler.address,
      "--settlement_id",
      settlementRecord.settlement_id,
      "--cash_asset",
      usdc.sacContractId,
      "--execution_a_asset_amount",
      "6000000",
      "--execution_a_cash_amount",
      "9000000",
      "--execution_b_asset_amount",
      "4000000",
      "--execution_b_cash_amount",
      "5000000",
    ],
    { sourceName: wallets.settler.name },
  );
  const after = readBalances(
    [wallets.alpha.address, wallets.beta.address, gammaWallet.address],
    [usdc.sacContractId, dtcust10y.sacContractId],
  );

  return {
    mode: "settle_batch_plus_apply_transfers",
    settlementTxHash: settled.txHash,
    transferTxHash: applied.txHash,
    balancesBefore: before,
    balancesAfter: after,
  };
}

async function runStep11CorporateActions() {
  const runTag = hashObject({ kind: "step11", ledger: fetchLatestLedger().sequence, now_ms: Date.now() });
  const claimProof = await createEntitlementClaimReceipt(participants.alpha, `step11-claim-${runTag}`);
  const event = invokeContract(
    contracts.corporateActionsEngine,
    "register_event",
    [
      "--issuer",
      wallets.issuer.address,
      "--event_id",
      claimProof.bundle.eventIdHashHex,
      "--verifier_id",
      verifierIds.entitlementClaim.verifierId,
      "--asset",
      dtcust10y.sacContractId,
      "--payout_asset",
      usdc.sacContractId,
      "--action_type",
      "2",
      "--event_root",
      claimProof.bundle.eventRootHex,
      "--manifest_hash",
      hashObject({ kind: "step11", runTag, field: "manifest" }),
      "--metadata_hash",
      hashObject({ kind: "step11", runTag, field: "metadata" }),
      "--record_date",
      "1700000000",
      "--ex_date",
      "1699000000",
      "--payable_date",
      "1701000000",
      "--claim_start_ledger",
      "0",
      "--claim_end_ledger",
      String(fetchLatestLedger().sequence + 50_000),
      "--payout_rate",
      "25",
    ],
    { sourceName: wallets.issuer.name },
  );
  const eventRecord = parseJsonish(event.raw);
  const claim = invokeContract(
    contracts.corporateActionsEngine,
    "claim",
    [
      "--claimant",
      wallets.alpha.address,
      "--proof_receipt_id",
      claimProof.receipt.receipt_id,
      "--event_id",
      eventRecord.event_id,
      "--claim_commitment",
      claimProof.bundle.claimCommitmentHex,
      "--claim_nullifier",
      claimProof.bundle.claimNullifierHex,
      "--disclosed_entitlement_quantity",
      claimProof.bundle.entitlementQuantity,
      "--disclosed_claim_amount",
      claimProof.bundle.claimAmount,
    ],
    { sourceName: wallets.alpha.name },
  );
  const claimRecord = parseJsonish(claim.raw);
  approveToken(usdc.sacContractId, wallets.treasury, contracts.corporateActionsEngine, claimProof.bundle.claimAmount);
  const before = readBalances([wallets.treasury.address, wallets.alpha.address], [usdc.sacContractId]);
  const paid = invokeContract(
    contracts.corporateActionsEngine,
    "mark_claim_paid_with_transfer",
    [
      "--operator",
      wallets.compliance.address,
      "--claim_id",
      claimRecord.claim_id,
      "--payment_batch_id",
      hashObject({ kind: "step11", runTag, payment: "batch" }),
      "--payout_source",
      wallets.treasury.address,
    ],
    { sourceName: wallets.compliance.name },
  );
  const after = readBalances([wallets.treasury.address, wallets.alpha.address], [usdc.sacContractId]);
  return {
    eventId: eventRecord.event_id,
    claimId: claimRecord.claim_id,
    txHashes: {
      registerEvent: event.txHash,
      claim: claim.txHash,
      pay: paid.txHash,
    },
    balancesBefore: before,
    balancesAfter: after,
  };
}

async function runStep12AuditAndFreeze(orderFlow, alphaProofs) {
  const runTag = hashObject({ kind: "step12", ledger: fetchLatestLedger().sequence, now_ms: Date.now() });
  const scopeHash = hashObject({ kind: "step12", runTag, scope: "audit-alpha" });
  const blobHash = hashObject({ kind: "step12", runTag, blob: "disclosure" });
  const caseId = hashObject({ kind: "step12", runTag, case: "audit" });

  const registerBlob = invokeContract(
    contracts.auditDisclosureRegistry,
    "register_blob",
    [
      "--operator",
      wallets.compliance.address,
      "--blob_hash",
      blobHash,
      "--blob_type",
      "7",
      "--owner_scope_hash",
      scopeHash,
      "--metadata_hash",
      hashObject({ kind: "step12", runTag, field: "metadata" }),
    ],
    { sourceName: wallets.compliance.name },
  );
  const grant = invokeContract(
    contracts.auditDisclosureRegistry,
    "grant",
    [
      "--operator",
      wallets.compliance.address,
      "--scope_hash",
      scopeHash,
      "--grantee",
      probeWallet.address,
      "--encrypted_key_hash",
      hashObject({ kind: "step12", runTag, field: "enc-key" }),
      "--expiry_ledger",
      String(fetchLatestLedger().sequence + 20_000),
      "--purpose_code",
      hashObject({ kind: "step12", runTag, field: "purpose" }),
      "--case_id",
      caseId,
    ],
    { sourceName: wallets.compliance.name },
  );
  const access = invokeContract(
    contracts.auditDisclosureRegistry,
    "record_access",
    [
      "--accessor",
      probeWallet.address,
      "--scope_hash",
      scopeHash,
      "--purpose_code",
      hashObject({ kind: "step12", runTag, field: "purpose" }),
      "--case_id",
      caseId,
      "--blob_hash",
      blobHash,
    ],
    { sourceName: probeWallet.name },
  );

  const freezeAction = setParticipantFreeze(
    participants.alpha.participantIdHash,
    true,
    hashObject({ kind: "step12", runTag, field: "freeze-reason" }),
    hashObject({ kind: "step12", runTag, field: "freeze-case" }),
  );
  const link = invokeContract(
    contracts.auditDisclosureRegistry,
    "link_operator_action",
    [
      "--operator",
      wallets.compliance.address,
      "--action_id",
      freezeAction.txHash,
      "--scope_hash",
      scopeHash,
      "--blob_hash",
      blobHash,
    ],
    { sourceName: wallets.compliance.name },
  );

  const blocked = await expectFailure("frozen trader blocked downstream", () =>
    invokeContract(
      contracts.orderCommitPool,
      "commit_order",
      [
        "--submitter",
        wallets.alpha.address,
        "--participant_id_hash",
        participants.alpha.participantIdHash,
        "--instrument_id_hash",
        instrumentIdHash,
        "--batch_id",
        hashObject({ kind: "step12", runTag, batch: "blocked" }),
        "--side",
        "1",
        "--order_commitment",
        hashObject({ kind: "step12", runTag, order: "blocked" }),
        "--collateral_proof_receipt_id",
        alphaProofs.collateral.receipt.receipt_id,
        "--encumbrance_proof_receipt_id",
        alphaProofs.unencumbered.receipt.receipt_id,
        "--cancel_nullifier",
        hashObject({ kind: "step12", runTag, cancel: "blocked" }),
        "--expiry_ledger",
        String(fetchLatestLedger().sequence + 20_000),
      ],
      { sourceName: wallets.alpha.name },
    ),
  );
  setParticipantFreeze(
    participants.alpha.participantIdHash,
    false,
    hashObject({ kind: "step12", runTag, field: "unfreeze-reason" }),
    hashObject({ kind: "step12", runTag, field: "unfreeze-case" }),
  );

  return {
    txHashes: {
      registerBlob: registerBlob.txHash,
      grant: grant.txHash,
      access: access.txHash,
      freeze: freezeAction.txHash,
      linkAction: link.txHash,
    },
    expectedFailures: [blocked],
  };
}

async function createCollateralBundleOnly(participant, label) {
  const nonceHex = createLiveNonce(label, { proofType: "2", participant: participant.key ?? participant.participantIdHash });
  const expiryLedger = fetchLatestLedger().sequence + 50_000;
  const fixtureOptions = {
    participantIdHashHex: with0x(participant.participantIdHash),
  };
  const provisional = runProofScript(
    "scripts/generate-collateral-sufficiency-proof.mjs",
    "__PHASE2_BUNDLE__",
    [
      DUMMY_STATEMENT_HASH,
      `${label}-provisional`,
      JSON.stringify(fixtureOptions),
    ],
  );
  const statementHash = buildStatementHash(
    "2",
    participant.participantIdHash,
    participant.wallet.address,
    nonceHex,
    expiryLedger,
    provisional.bundle.portfolioCommitmentHex,
  );
  const finalBundle = runProofScript(
    "scripts/generate-collateral-sufficiency-proof.mjs",
    "__PHASE2_BUNDLE__",
    [
      statementHash,
      `${label}-final`,
      JSON.stringify(fixtureOptions),
    ],
  );
  return { bundle: finalBundle.bundle, nonceHex, expiryLedger };
}

async function createCollateralReceipt(participant, label) {
  const pending = await createCollateralBundleOnly(participant, label);
  return {
    ...pending,
    receipt: submitProofReceipt(
      participant,
      "2",
      verifierIds.collateralSufficiency,
      pending.bundle.portfolioCommitmentHex,
      pending.nonceHex,
      pending.expiryLedger,
      pending.bundle.proofPayloadHex,
    ),
  };
}

async function createUnencumberedReceipt(participant, label) {
  const nonceHex = createLiveNonce(label, { proofType: "3", participant: participant.key ?? participant.participantIdHash });
  const expiryLedger = fetchLatestLedger().sequence + 50_000;
  const provisional = runProofScript(
    "scripts/generate-unencumbered-proof.mjs",
    "__PHASE3_BUNDLE__",
    [
      DUMMY_STATEMENT_HASH,
      `${label}-provisional`,
      with0x(participant.participantIdHash),
    ],
  );
  const statementHash = buildStatementHash(
    "3",
    participant.participantIdHash,
    participant.wallet.address,
    nonceHex,
    expiryLedger,
    provisional.bundle.availabilityRootHex,
  );
  const finalBundle = runProofScript(
    "scripts/generate-unencumbered-proof.mjs",
    "__PHASE3_BUNDLE__",
    [
      statementHash,
      `${label}-final`,
      with0x(participant.participantIdHash),
    ],
  );
  return {
    bundle: finalBundle.bundle,
    nonceHex,
    expiryLedger,
    receipt: submitProofReceipt(
      participant,
      "3",
      verifierIds.unencumberedLot,
      finalBundle.bundle.availabilityRootHex,
      nonceHex,
      expiryLedger,
      finalBundle.bundle.proofPayloadHex,
    ),
  };
}

async function createPrivateMatchReceipt(bidParticipant, askParticipant, bidProofs, askProofs, label) {
  const matcherParticipant = participants.matcher;
  const nonceHex = createLiveNonce(label, {
    proofType: "4",
    matcher: matcherParticipant.key ?? matcherParticipant.participantIdHash,
    bidParticipant: bidParticipant.key ?? bidParticipant.participantIdHash,
    askParticipant: askParticipant.key ?? askParticipant.participantIdHash,
  });
  const expiryLedger = fetchLatestLedger().sequence + 50_000;
  const fixtureOptions = {
    bidParticipantIdHashHex: with0x(bidParticipant.participantIdHash),
    askParticipantIdHashHex: with0x(askParticipant.participantIdHash),
    instrumentIdHashHex: with0x(instrumentIdHash),
    bidCollateralProofReceiptIdHex: with0x(bidProofs.collateral.receipt.receipt_id),
    bidEncumbranceProofReceiptIdHex: with0x(bidProofs.unencumbered.receipt.receipt_id),
    askCollateralProofReceiptIdHex: with0x(askProofs.collateral.receipt.receipt_id),
    askEncumbranceProofReceiptIdHex: with0x(askProofs.unencumbered.receipt.receipt_id),
  };
  const provisional = runProofScript(
    "scripts/generate-private-match-proof.mjs",
    "__PHASE4_BUNDLE__",
    [
      DUMMY_STATEMENT_HASH,
      `${label}-provisional`,
      JSON.stringify(fixtureOptions),
    ],
  );
  const statementHash = buildStatementHash(
    "4",
    matcherParticipant.participantIdHash,
    wallets.matcher.address,
    nonceHex,
    expiryLedger,
    provisional.bundle.executionCommitmentHex,
  );
  const finalBundle = runProofScript(
    "scripts/generate-private-match-proof.mjs",
    "__PHASE4_BUNDLE__",
    [
      statementHash,
      `${label}-final`,
      JSON.stringify(fixtureOptions),
    ],
  );
  return {
    bundle: finalBundle.bundle,
    expiryLedger,
    receipt: submitProofReceipt(
      matcherParticipant,
      "4",
      verifierIds.privateMatch,
      finalBundle.bundle.executionCommitmentHex,
      nonceHex,
      expiryLedger,
      finalBundle.bundle.proofPayloadHex,
      matcherParticipant.participantIdHash,
      wallets.matcher,
    ),
  };
}

async function createBatchNettingReceipt(options, label) {
  const settlerParticipant = participants.settler;
  const nonceHex = createLiveNonce(label, {
    proofType: "5",
    settler: settlerParticipant.key ?? settlerParticipant.participantIdHash,
  });
  const expiryLedger = fetchLatestLedger().sequence + 50_000;
  const provisional = runProofScript(
    "scripts/generate-batch-netting-proof.mjs",
    "__PHASE5_BUNDLE__",
    [
      DUMMY_STATEMENT_HASH,
      `${label}-provisional`,
      JSON.stringify(options),
    ],
  );
  const statementHash = buildStatementHash(
    "5",
    settlerParticipant.participantIdHash,
    wallets.settler.address,
    nonceHex,
    expiryLedger,
    provisional.bundle.settlementCommitmentHex,
  );
  const finalBundle = runProofScript(
    "scripts/generate-batch-netting-proof.mjs",
    "__PHASE5_BUNDLE__",
    [
      statementHash,
      `${label}-final`,
      JSON.stringify(options),
    ],
  );
  return {
    bundle: finalBundle.bundle,
    expiryLedger,
    receipt: submitProofReceipt(
      settlerParticipant,
      "5",
      verifierIds.batchNetting,
      finalBundle.bundle.settlementCommitmentHex,
      nonceHex,
      expiryLedger,
      finalBundle.bundle.proofPayloadHex,
      settlerParticipant.participantIdHash,
      wallets.settler,
    ),
  };
}

async function createPrivateMatchReceiptFromBatchTrade(side, batchOptions, batchBundle) {
  const isA = side === "A";
  const label = `step10-private-${side.toLowerCase()}`;
  const bidParticipant = isA ? participants.alpha : gammaParticipant;
  const askParticipant = isA ? participants.beta : participants.alpha;
  const matcherParticipant = participants.matcher;
  const nonceHex = createLiveNonce(label, {
    proofType: "4",
    side,
    matcher: matcherParticipant.key ?? matcherParticipant.participantIdHash,
  });
  const expiryLedger = fetchLatestLedger().sequence + 50_000;
  const fixtureOptions = {
    bidParticipantIdHashHex: with0x(bidParticipant.participantIdHash),
    askParticipantIdHashHex: with0x(askParticipant.participantIdHash),
    instrumentIdHashHex: with0x(instrumentIdHash),
    bidCollateralProofReceiptIdHex: isA
      ? batchOptions.tradeABidCollateralProofReceiptIdHex
      : batchOptions.tradeBBidCollateralProofReceiptIdHex,
    bidEncumbranceProofReceiptIdHex: isA
      ? batchOptions.tradeABidEncumbranceProofReceiptIdHex
      : batchOptions.tradeBBidEncumbranceProofReceiptIdHex,
    askCollateralProofReceiptIdHex: isA
      ? batchOptions.tradeAAskCollateralProofReceiptIdHex
      : batchOptions.tradeBAskCollateralProofReceiptIdHex,
    askEncumbranceProofReceiptIdHex: isA
      ? batchOptions.tradeAAskEncumbranceProofReceiptIdHex
      : batchOptions.tradeBAskEncumbranceProofReceiptIdHex,
    bidLimitPrice: isA ? 101 : 111,
    askLimitPrice: isA ? 99 : 109,
    clearPrice: isA ? 100 : 110,
    clearQuantity: isA ? 10 : 6,
    bidOrderSaltHex: isA
      ? "0x707788889999aaaabbbbccccddddeeeeffff0000111122223333444455556666"
      : "0xe0eeffff0000111122223333444455556666777788889999aaaabbbbccccdddd",
    askOrderSaltHex: isA
      ? "0x80889999aaaabbbbccccddddeeeeffff00001111222233334444555566667777"
      : "0xf0ff0000111122223333444455556666777788889999aaaabbbbccccddddeeee",
    executionSaltHex: isA
      ? "0x9099aaaabbbbccccddddeeeeffff000011112222333344445555666677778888"
      : "0x111122223333444455556666777788889999aaaabbbbccccddddeeeeffff0000",
  };
  const commitmentHex = isA ? batchBundle.executionACommitmentHex : batchBundle.executionBCommitmentHex;
  const statementHash = buildStatementHash(
    "4",
    matcherParticipant.participantIdHash,
    wallets.matcher.address,
    nonceHex,
    expiryLedger,
    commitmentHex,
  );
  const finalBundle = runProofScript(
    "scripts/generate-private-match-proof.mjs",
    "__PHASE4_BUNDLE__",
    [
      statementHash,
      `${label}-final`,
      JSON.stringify(fixtureOptions),
    ],
  );
  return {
    receipt: submitProofReceipt(
      matcherParticipant,
      "4",
      verifierIds.privateMatch,
      commitmentHex,
      nonceHex,
      expiryLedger,
      finalBundle.bundle.proofPayloadHex,
      matcherParticipant.participantIdHash,
      wallets.matcher,
    ),
  };
}

async function createEntitlementClaimReceipt(participant, label) {
  const nonceHex = createLiveNonce(label, { proofType: "6", participant: participant.key ?? participant.participantIdHash });
  const expiryLedger = fetchLatestLedger().sequence + 50_000;
  const fixtureOptions = {
    participantIdHashHex: with0x(participant.participantIdHash),
    assetIdHashHex: with0x(hashObject({ kind: "step11", field: "asset-id" })),
    eventIdHashHex: with0x(hashObject({ kind: "step11", field: "event-id" })),
  };
  const provisional = runProofScript(
    "scripts/generate-entitlement-claim-proof.mjs",
    "__PHASE6_BUNDLE__",
    [
      DUMMY_STATEMENT_HASH,
      `${label}-provisional`,
      JSON.stringify(fixtureOptions),
    ],
  );
  const statementHash = buildStatementHash(
    "6",
    participant.participantIdHash,
    participant.wallet.address,
    nonceHex,
    expiryLedger,
    provisional.bundle.claimCommitmentHex,
  );
  const finalBundle = runProofScript(
    "scripts/generate-entitlement-claim-proof.mjs",
    "__PHASE6_BUNDLE__",
    [
      statementHash,
      `${label}-final`,
      JSON.stringify(fixtureOptions),
    ],
  );
  return {
    bundle: finalBundle.bundle,
    receipt: submitProofReceipt(
      participant,
      "6",
      verifierIds.entitlementClaim,
      finalBundle.bundle.claimCommitmentHex,
      nonceHex,
      expiryLedger,
      finalBundle.bundle.proofPayloadHex,
    ),
  };
}

function submitProofReceipt(
  participant,
  proofType,
  verifierConfig,
  portfolioCommitmentHex,
  nonceHex,
  expiryLedger,
  proofPayloadHex,
  participantIdOverride = participant.participantIdHash,
  walletOverride = participant.wallet,
) {
  const output = invokeContract(
    contracts.proofGateway,
    "verify_and_record",
    [
      "--submitter",
      walletOverride.address,
      "--participant_id_hash",
      participantIdOverride,
      "--proof_type",
      proofType,
      "--verifier_id",
      verifierConfig.verifierId,
      "--portfolio_commitment",
      portfolioCommitmentHex,
      "--nonce",
      nonceHex,
      "--expiry_ledger",
      String(expiryLedger),
      "--policy_version",
      String(proofSummary.policy_version),
      "--epoch_id",
      String(proofSummary.current_epoch),
      "--required_margin",
      String(proofSummary.required_margin),
      "--proof",
      proofPayloadHex,
    ],
    { sourceName: walletOverride.name },
  );
  return parseJsonish(output.raw);
}

function buildStatementHash(
  proofType,
  participantIdHash,
  submitterAddress,
  nonceHex,
  expiryLedger,
  portfolioCommitmentHex,
) {
  return stripQuotes(viewContract(
    contracts.proofGateway,
    "build_statement_hash",
    [
      "--proof_type",
      proofType,
      "--participant_id_hash",
      participantIdHash,
      "--submitter",
      submitterAddress,
      "--nonce",
      nonceHex,
      "--expiry_ledger",
      String(expiryLedger),
      "--policy_version",
      String(proofSummary.policy_version),
      "--epoch_id",
      String(proofSummary.current_epoch),
      "--portfolio_commitment",
      portfolioCommitmentHex,
      "--required_margin",
      String(proofSummary.required_margin),
    ],
  ));
}

function runProofScript(scriptPath, marker, argsList) {
  const output = execFileSync("node", [scriptPath, ...argsList], {
    cwd: process.cwd(),
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  const line = output.split("\n").find((entry) => entry.startsWith(marker));
  if (!line) {
    fail(`missing ${marker} in ${scriptPath} output`);
  }
  return JSON.parse(line.slice(marker.length));
}

function approveToken(tokenId, wallet, spender, amount) {
  invokeContract(
    tokenId,
    "approve",
    [
      "--from",
      wallet.address,
      "--spender",
      spender,
      "--amount",
      amount,
      "--expiration_ledger",
      String(fetchLatestLedger().sequence + 20_000),
    ],
    { sourceName: wallet.name },
  );
}

function payClassicAsset(sourceName, destination, assetString, rawAmount) {
  runStellar([
    "tx",
    "new",
    "payment",
    "--source",
    sourceName,
    "--network",
    network,
    "--destination",
    destination,
    "--asset",
    assetString,
    "--amount",
    String(rawAmount),
  ]);
}

function ensureTrustline(sourceName, assetString) {
  try {
    runStellar([
      "tx",
      "new",
      "change-trust",
      "--source",
      sourceName,
      "--network",
      network,
      "--line",
      assetString,
    ]);
  } catch (error) {
    const message = errorText(error);
    if (
      !message.includes("already exist")
      && !message.includes("op_already_exists")
      && !(message.includes("transaction submission timeout") && hasTrustline(addressOf(sourceName), assetString))
    ) {
      throw error;
    }
  }
}

function authorizeTrustline(assetString, trustor) {
  try {
    runStellar([
      "tx",
      "new",
      "set-trustline-flags",
      "--source",
      wallets.issuer.name,
      "--network",
      network,
      "--trustor",
      trustor,
      "--asset",
      assetString,
      "--set-authorize",
    ]);
  } catch (error) {
    if (
      !errorText(error).includes("transaction submission timeout")
      || !isTrustlineAuthorized(trustor, assetString)
    ) {
      throw error;
    }
  }
}

function readBalances(addresses, tokenIds) {
  return Object.fromEntries(
    addresses.map((address) => [
      address,
      Object.fromEntries(
        tokenIds.map((tokenId) => [
          tokenId,
          parseJsonish(viewContract(tokenId, "balance", ["--id", address])),
        ]),
      ),
    ]),
  );
}

function invokeContractWithTimeoutCheck(
  contractId,
  functionName,
  contractArgs,
  options,
  verifyFn,
) {
  for (let invokeAttempt = 1; invokeAttempt <= 3; invokeAttempt += 1) {
    try {
      return invokeContract(contractId, functionName, contractArgs, options);
    } catch (error) {
      if (errorText(error).includes("transaction submission timeout")) {
        for (let pollAttempt = 1; pollAttempt <= 6; pollAttempt += 1) {
          sleep(2_000 * pollAttempt);
          try {
            if (verifyFn()) {
              return {
                contractId,
                functionName,
                txHash: parseTxHash(errorText(error)),
                raw: "timeout-state-confirmed",
              };
            }
          } catch {
            // Keep polling until the network reflects the state or we exhaust the window.
          }
        }
        if (invokeAttempt < 3) {
          log(`retrying timed-out admin mutation: ${functionName} (${invokeAttempt}/3)`);
          continue;
        }
      }
      throw error;
    }
  }
  fail(`admin mutation failed after retries: ${functionName} on ${contractId}`);
}

function setParticipantFreeze(participantIdHash, frozen, reasonCode, caseId) {
  return invokeContractWithTimeoutCheck(
    phase0.contracts.complianceControl.contractId,
    "set_participant_freeze",
    [
      "--operator",
      wallets.compliance.address,
      "--participant_id_hash",
      participantIdHash,
      "--frozen",
      String(frozen),
      "--reason_code",
      reasonCode,
      "--case_id",
      caseId,
    ],
    { sourceName: wallets.compliance.name },
    () => viewContract(
      phase0.contracts.complianceControl.contractId,
      "is_participant_frozen",
      ["--participant_id_hash", participantIdHash],
    ) === frozen,
  );
}

function setAssetPause(asset, paused, reasonCode, caseId) {
  return invokeContractWithTimeoutCheck(
    phase0.contracts.complianceControl.contractId,
    "set_asset_pause",
    [
      "--operator",
      wallets.compliance.address,
      "--asset",
      asset,
      "--paused",
      String(paused),
      "--reason_code",
      reasonCode,
      "--case_id",
      caseId,
    ],
    { sourceName: wallets.compliance.name },
    () => viewContract(
      phase0.contracts.complianceControl.contractId,
      "is_asset_paused",
      ["--asset", asset],
    ) === paused,
  );
}

function setGlobalPause(paused, reasonCode, caseId) {
  return invokeContractWithTimeoutCheck(
    phase0.contracts.complianceControl.contractId,
    "set_global_pause",
    [
      "--operator",
      wallets.compliance.address,
      "--paused",
      String(paused),
      "--reason_code",
      reasonCode,
      "--case_id",
      caseId,
    ],
    { sourceName: wallets.compliance.name },
    () => viewContract(
      phase0.contracts.complianceControl.contractId,
      "is_globally_paused",
      [],
    ) === paused,
  );
}

function setComplianceState(participantIdHash, kycStatus, sanctionsStatus, expiryLedger, reviewCaseId) {
  return invokeContractWithTimeoutCheck(
    phase0.contracts.participantRegistry.contractId,
    "set_compliance_state",
    [
      "--operator",
      wallets.compliance.address,
      "--participant_id_hash",
      participantIdHash,
      "--kyc_status",
      String(kycStatus),
      "--sanctions_status",
      String(sanctionsStatus),
      "--credential_expiry_ledger",
      String(expiryLedger),
      "--review_case_id",
      reviewCaseId,
    ],
    { sourceName: wallets.compliance.name },
    () => {
      const record = viewContract(
        phase0.contracts.participantRegistry.contractId,
        "get_participant",
        ["--participant_id_hash", participantIdHash],
      );
      return record.kyc_status === kycStatus
        && record.sanctions_status === sanctionsStatus
        && record.credential_expiry_ledger === expiryLedger;
    },
  );
}

function hasTrustline(address, assetString) {
  return findTrustline(address, assetString) !== null;
}

function isTrustlineAuthorized(address, assetString) {
  const trustline = findTrustline(address, assetString);
  return trustline?.is_authorized === true;
}

function findTrustline(address, assetString) {
  const [assetCode, assetIssuer] = assetString.split(":");
  if (!assetCode || !assetIssuer) {
    fail(`invalid asset string ${assetString}`);
  }
  const account = fetchJson(`${horizonUrl}/accounts/${address}`);
  return account.balances.find((balance) =>
    balance.asset_code === assetCode && balance.asset_issuer === assetIssuer,
  ) ?? null;
}

function hasParticipant(participantIdHash) {
  try {
    viewContract(
      phase0.contracts.participantRegistry.contractId,
      "get_participant",
      ["--participant_id_hash", participantIdHash],
    );
    return true;
  } catch {
    return false;
  }
}

async function expectFailure(name, fn) {
  try {
    await fn();
    return {
      name,
      ok: false,
      error: "call unexpectedly succeeded",
    };
  } catch (error) {
    return {
      name,
      ok: true,
      error: errorText(error),
    };
  }
}

function ensureIdentityAccount(name, createIfMissing) {
  if (!hasIdentity(name)) {
    if (!createIfMissing) {
      fail(`missing identity ${name}`);
    }
    runStellar(["keys", "generate", name]);
  }
  const address = addressOf(name);
  if (!accountExists(address)) {
    runStellar(["keys", "fund", name, "--network", network]);
  }
}

function signDigestHex(identityName, digestHex) {
  const secret = runStellar(["keys", "secret", identityName]).trim();
  const raw = decodeStrKey(secret).payload;
  const pkcs8 = Buffer.concat([
    Buffer.from("302e020100300506032b657004220420", "hex"),
    raw,
  ]);
  const key = createPrivateKey({ key: pkcs8, format: "der", type: "pkcs8" });
  return cryptoSign(null, Buffer.from(strip0x(digestHex), "hex"), key).toString("hex");
}

function invokeContract(contractId, functionName, contractArgs, { sourceName, send = true } = {}) {
  const normalizedArgs = contractArgs.map((arg) =>
    typeof arg === "string" && /^0x[0-9a-f]+$/i.test(arg) ? arg.slice(2) : arg,
  );
  const command = [
    "contract",
    "invoke",
    "--id",
    contractId,
    "--source",
    sourceName ?? wallets.deployer.name,
    "--network",
    network,
  ];
  if (!send) {
    command.push("--send", "no");
  }
  try {
    const output = runStellar([...command, "--", functionName, ...normalizedArgs]);
    return {
      contractId,
      functionName,
      txHash: parseTxHash(output),
      raw: output.trim(),
    };
  } catch (error) {
    error.message = `contract invoke failed: ${functionName} on ${contractId}\n${errorText(error)}`;
    throw error;
  }
}

function viewContract(contractId, functionName, contractArgs) {
  return parseJsonish(invokeContract(contractId, functionName, contractArgs, { send: false }).raw);
}

function fetchLatestLedger() {
  const page = fetchJson(`${horizonUrl}/ledgers?order=desc&limit=1`);
  const ledger = page._embedded.records[0];
  return {
    sequence: Number(ledger.sequence),
    closedAt: ledger.closed_at,
  };
}

function fetchJson(url) {
  return JSON.parse(execFileSync("curl", ["-sSfL", url], { encoding: "utf8" }));
}

function parseJsonish(value) {
  if (typeof value !== "string") {
    return value;
  }
  try {
    return JSON.parse(value);
  } catch {
    return value;
  }
}

function stripQuotes(value) {
  return String(value).replace(/^"+|"+$/g, "");
}

function with0x(value) {
  return value.startsWith("0x") ? value : `0x${value}`;
}

function strip0x(value) {
  return value.startsWith("0x") ? value.slice(2) : value;
}

function hashObject(value) {
  return createHash("sha256").update(JSON.stringify(sortObject(value))).digest("hex");
}

function createLiveNonce(label, scope = {}) {
  liveNonceCounter += 1;
  return hashObject({
    namespace: phase0.namespace,
    kind: "live-proof-nonce",
    label,
    scope,
    ledger: fetchLatestLedger().sequence,
    now_ms: Date.now(),
    counter: liveNonceCounter,
  });
}

function sortObject(value) {
  if (Array.isArray(value)) {
    return value.map(sortObject);
  }
  if (value && typeof value === "object") {
    return Object.fromEntries(Object.keys(value).sort().map((key) => [key, sortObject(value[key])]));
  }
  return value;
}

function waitForState(label, readFn, attempts = 8) {
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    try {
      const value = readFn();
      if (value !== undefined && value !== null) {
        return value;
      }
    } catch {
      // Keep polling until the just-written state becomes readable.
    }
    sleep(1_500 * attempt);
  }
  fail(`state visibility timeout: ${label}`);
}

function waitForLedgerAtLeast(label, targetLedger, attempts = 10) {
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    if (fetchLatestLedger().sequence >= targetLedger) {
      return;
    }
    sleep(1_500 * attempt);
  }
  fail(`ledger advancement timeout: ${label} target=${targetLedger}`);
}

function invokeWithDependencyRetry(
  contractId,
  functionName,
  contractArgs,
  options,
  { retryErrorFragment, dependencyLabel, dependencyFn, attempts = 4 },
) {
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    try {
      return invokeContract(contractId, functionName, contractArgs, options);
    } catch (error) {
      if (retryErrorFragment && errorText(error).includes(retryErrorFragment) && attempt < attempts) {
        waitForState(dependencyLabel, dependencyFn, 10);
        waitForLedgerAtLeast(`${dependencyLabel} ledger`, fetchLatestLedger().sequence + 1, 4);
        log(`retrying dependent invoke: ${functionName} (${attempt}/${attempts})`);
        continue;
      }
      throw error;
    }
  }
  fail(`dependent invoke failed after retries: ${functionName} on ${contractId}`);
}

function decodeStrKey(strkey) {
  const raw = base32Decode(strkey);
  const body = raw.subarray(0, -2);
  const checksum = raw.readUInt16LE(raw.length - 2);
  if (checksum !== crc16Xmodem(body)) {
    fail(`invalid strkey checksum for ${strkey}`);
  }
  return { version: body[0], payload: body.subarray(1) };
}

function base32Decode(value) {
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
  let bits = "";
  for (const char of value.replace(/=+$/g, "").toUpperCase()) {
    const index = alphabet.indexOf(char);
    if (index === -1) {
      fail(`invalid base32 char ${char}`);
    }
    bits += index.toString(2).padStart(5, "0");
  }
  const bytes = [];
  for (let index = 0; index + 8 <= bits.length; index += 8) {
    bytes.push(Number.parseInt(bits.slice(index, index + 8), 2));
  }
  return Buffer.from(bytes);
}

function crc16Xmodem(buffer) {
  let crc = 0x0000;
  for (const byte of buffer) {
    crc ^= byte << 8;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = (crc & 0x8000) !== 0 ? ((crc << 1) ^ 0x1021) & 0xffff : (crc << 1) & 0xffff;
    }
  }
  return crc;
}

function hasIdentity(name) {
  return runStellar(["keys", "ls"]).trim().split(/\s+/).includes(name);
}

function addressOf(name) {
  return runStellar(["keys", "address", name]).trim();
}

function accountExists(address) {
  return execFileSync(
    "curl",
    ["-s", "-o", "/dev/null", "-w", "%{http_code}", `${horizonUrl}/accounts/${address}`],
    { encoding: "utf8" },
  ).trim() === "200";
}

function runStellar(commandArgs) {
  for (let attempt = 1; attempt <= 5; attempt += 1) {
    try {
      return execFileSync("stellar", ["--no-cache", ...commandArgs], {
        cwd: process.cwd(),
        encoding: "utf8",
        stdio: ["ignore", "pipe", "pipe"],
      });
    } catch (error) {
      const message = errorText(error);
      if (
        attempt < 5
        && (
          message.includes("EOF while parsing")
          || message.includes("unexpected end of file")
          || message.includes("http request failed")
          || message.includes("Networking or low-level protocol error")
          || message.includes("connection reset")
          || message.includes("HTTP error: connection error")
          || message.includes("Request timeout")
          || message.includes("TxBadSeq")
        )
      ) {
        log(`retry ${attempt}/5: ${describeStellarCommand(commandArgs)}`);
        sleep(1_500 * attempt);
        continue;
      }
      throw error;
    }
  }
  fail(`stellar command retry budget exhausted: ${commandArgs.join(" ")}`);
}

function parseTxHash(output) {
  const explorerMatch = output.match(/\/tx\/([0-9a-f]{64})/i);
  if (explorerMatch) {
    return explorerMatch[1];
  }
  const hashes = output.match(/\b[0-9a-f]{64}\b/gi);
  return hashes?.[hashes.length - 1] ?? null;
}

function parseArgs(argv) {
  const parsed = {};
  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i];
    if (!token.startsWith("--")) {
      continue;
    }
    const next = argv[i + 1];
    if (!next || next.startsWith("--")) {
      parsed[token.slice(2)] = true;
      continue;
    }
    parsed[token.slice(2)] = next;
    i += 1;
  }
  return parsed;
}

function errorText(error) {
  return [error?.stdout, error?.stderr, error?.message].filter(Boolean).join("\n");
}

function describeStellarCommand(commandArgs) {
  if (commandArgs[0] === "contract" && commandArgs[1] === "invoke") {
    const separator = commandArgs.indexOf("--");
    const functionName = separator === -1 ? "unknown" : commandArgs[separator + 1];
    return `stellar contract invoke ${functionName}`;
  }
  return `stellar ${commandArgs[0]} ${commandArgs[1] ?? ""}`.trim();
}

function timestampTag() {
  return new Date().toISOString().replace(/[:.]/g, "-").toLowerCase();
}

function sleep(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

function log(message) {
  console.error(`[steps5-12] ${message}`);
}

function fail(message) {
  throw new Error(message);
}

const DUMMY_STATEMENT_HASH = `0x${"5a".repeat(32)}`;

main().catch((error) => {
  console.error(errorText(error) || error);
  process.exitCode = 1;
});
