#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import {
  artifactDir as unencumberedArtifactDir,
  ensureGroth16Setup as ensureUnencumberedSetup,
} from "./unencumbered-phase3-lib.mjs";
import {
  artifactDir as privateMatchArtifactDir,
  ensureGroth16Setup as ensurePrivateMatchSetup,
} from "./private-match-phase4-lib.mjs";
import {
  artifactDir as batchNettingArtifactDir,
  ensureGroth16Setup as ensureBatchNettingSetup,
} from "./batch-netting-phase5-lib.mjs";
import {
  artifactDir as entitlementClaimArtifactDir,
  ensureGroth16Setup as ensureEntitlementClaimSetup,
} from "./entitlement-claim-phase6-lib.mjs";

const args = parseArgs(process.argv.slice(2));
const phase0ManifestPath = path.resolve(
  args.manifest ?? "deployments/testnet-phase0-demo0628b.json",
);
const phase0 = JSON.parse(readFileSync(phase0ManifestPath, "utf8"));
const network = phase0.network ?? "testnet";
const namespace = phase0.namespace;
const outDir = path.resolve(args["out-dir"] ?? "deployments");
const reportDir = path.resolve(args["report-dir"] ?? "deployments/reports");
const horizonUrl = network === "testnet"
  ? "https://horizon-testnet.stellar.org"
  : fail(`unsupported network "${network}" for protocol deployment`);
const deployerName = phase0.admin?.name ?? phase0.wallets?.deployer?.name ?? "rosca-admin";
const deployerAddress = phase0.admin?.address ?? addressOf(deployerName);
const complianceWallet = phase0.wallets?.compliance ?? fail("phase-0 manifest missing compliance wallet");
const matcherWallet = phase0.wallets?.matcher ?? fail("phase-0 manifest missing matcher wallet");
const settlerWallet = phase0.wallets?.settler ?? fail("phase-0 manifest missing settler wallet");

const proofType = {
  collateralSufficiency: 2,
  unencumberedLot: 3,
  privateMatch: 4,
  batchNetting: 5,
  entitlementClaim: 6,
};

const cctp = {
  stellarDomain: 27,
  tokenMessengerMinter: "CDNG7HXAPBWICI2E3AUBP3YZWZELJLYSB6F5CC7WLDTLTHVM74SLRTHP",
  messageTransmitter: "CBJ6MTCKKZG73PMDZCJMSFRD7DQEMI4FKDH7CGDSV4W6FHCRBCQAVVJY",
  forwarderContract: "CA66Q2WFBND6V4UEB7RD4SAXSVIWMD6RA4X3U32ELVFGXV5PJK4T4VSZ",
};

async function main() {
  mkdirSync(outDir, { recursive: true });
  mkdirSync(reportDir, { recursive: true });

  log(`deploying protocol stack for phase-0 namespace "${namespace}" on ${network}`);
  const latestLedger = await fetchLatestLedger();
  const verifierArtifacts = await ensureVerifierArtifacts();
  const buildArtifacts = buildProtocolArtifacts();
  const attestor = ensureAttestorIdentity();

  const config = buildProtocolConfig(latestLedger.sequence, attestor);
  const contracts = await deployProtocolContracts(buildArtifacts, verifierArtifacts, config);
  const txLog = configureProtocolContracts(contracts, config);
  const verification = verifyDeployment(contracts, config, latestLedger.sequence);

  const manifest = {
    namespace,
    network,
    deployedAt: new Date().toISOString(),
    basedOnPhase0Manifest: phase0ManifestPath,
    admin: {
      name: deployerName,
      address: deployerAddress,
    },
    phase0: {
      contracts: phase0.contracts,
      assets: phase0.assets,
      wallets: phase0.wallets,
    },
    cctp: {
      ...cctp,
      forwarderPayloadHex: config.cctpForwarderPayloadHex,
    },
    attestor,
    config,
    verifiers: contracts.verifiers,
    contracts: contracts.protocol,
    txLog,
    verification,
  };

  const manifestPath = path.join(outDir, `testnet-protocol-${namespace}.json`);
  writeFileSync(manifestPath, JSON.stringify(manifest, null, 2));

  const report = {
    phase0ManifestPath,
    manifestPath,
    namespace,
    network,
    startedAt: latestLedger.closedAt,
    completedAt: new Date().toISOString(),
    latestLedger,
    contracts: Object.fromEntries(
      Object.entries(contracts.protocol).map(([key, value]) => [key, value.contractId]),
    ),
    verifierContracts: Object.fromEntries(
      Object.entries(contracts.verifiers).map(([key, value]) => [key, value.contractId]),
    ),
    verification,
    txLog,
  };
  const reportPath = path.join(
    reportDir,
    `phase4-protocol-deploy-${namespace}-${timestampTag()}.json`,
  );
  writeFileSync(reportPath, JSON.stringify(report, null, 2));

  log(`protocol manifest written to ${manifestPath}`);
  log(`protocol deployment report written to ${reportPath}`);
}

async function ensureVerifierArtifacts() {
  if (!existsSync(path.join(unencumberedArtifactDir, "verification_key.json"))) {
    await ensureUnencumberedSetup();
  }
  if (!existsSync(path.join(privateMatchArtifactDir, "verification_key.json"))) {
    await ensurePrivateMatchSetup();
  }
  if (!existsSync(path.join(batchNettingArtifactDir, "verification_key.json"))) {
    await ensureBatchNettingSetup();
  }
  if (!existsSync(path.join(entitlementClaimArtifactDir, "verification_key.json"))) {
    await ensureEntitlementClaimSetup();
  }

  return {
    unencumberedLot: {
      verificationKeyHex: encodeVerificationKeyJson(path.join(unencumberedArtifactDir, "verification_key.json")),
      source: path.join(unencumberedArtifactDir, "verification_key.json"),
    },
    privateMatch: {
      verificationKeyHex: encodeVerificationKeyJson(path.join(privateMatchArtifactDir, "verification_key.json")),
      source: path.join(privateMatchArtifactDir, "verification_key.json"),
    },
    batchNetting: {
      verificationKeyHex: encodeVerificationKeyJson(path.join(batchNettingArtifactDir, "verification_key.json")),
      source: path.join(batchNettingArtifactDir, "verification_key.json"),
    },
    entitlementClaim: {
      verificationKeyHex: encodeVerificationKeyJson(path.join(entitlementClaimArtifactDir, "verification_key.json")),
      source: path.join(entitlementClaimArtifactDir, "verification_key.json"),
    },
  };
}

function buildProtocolArtifacts() {
  const packages = [
    "audit-disclosure-registry",
    "collateral-policy",
    "cctp-ingress-adapter",
    "encumbrance-registry",
    "proof-gateway",
    "order-commit-pool",
    "settlement-netting-engine",
    "corporate-actions-engine",
    "mock-proof-verifier",
    "unencumbered-lot-verifier",
    "private-match-verifier",
    "batch-netting-verifier",
    "entitlement-claim-verifier",
  ];

  for (const pkg of packages) {
    log(`building ${pkg}`);
    runStellar([
      "contract",
      "build",
      "--package",
      pkg,
      "--out-dir",
      ".stellar-artifacts",
    ]);
  }

  return Object.fromEntries(
    packages.map((pkg) => [pkg, path.resolve(".stellar-artifacts", `${pkg.replaceAll("-", "_")}.wasm`)]),
  );
}

function ensureAttestorIdentity() {
  const name = `zkdtcc-${namespace}-attestor`;
  if (!hasIdentity(name)) {
    log(`generating local attestor identity ${name}`);
    runStellar(["keys", "generate", name]);
  }
  const address = addressOf(name);
  return {
    name,
    address,
    attestorId: hashObject({
      namespace,
      kind: "encumbrance-attestor",
      address,
    }),
    publicKeyHex: strkeyPayloadHex(address),
  };
}

function buildProtocolConfig(currentLedger, attestor) {
  const validFromLedger = currentLedger;
  const validUntilLedger = Math.min(0xffff_fffe, currentLedger + 1_500_000);
  const currentEpoch = Number(currentLedger);
  const requiredMargin = "1000000";

  const verifierRegistry = {
    collateralSufficiency: {
      verifierId: hashObject({ namespace, kind: "verifier-id", proofType: "collateral-sufficiency" }),
      proofType: proofType.collateralSufficiency,
      mode: "statement-hash-echo",
      policyCutoffHash: hashObject({ namespace, kind: "policy-cutoff", proofType: "collateral-sufficiency" }),
    },
    unencumberedLot: {
      verifierId: hashObject({ namespace, kind: "verifier-id", proofType: "unencumbered-lot" }),
      proofType: proofType.unencumberedLot,
      mode: "circom-groth16",
      policyCutoffHash: hashObject({ namespace, kind: "policy-cutoff", proofType: "unencumbered-lot" }),
    },
    privateMatch: {
      verifierId: hashObject({ namespace, kind: "verifier-id", proofType: "private-match" }),
      proofType: proofType.privateMatch,
      mode: "circom-groth16",
      policyCutoffHash: hashObject({ namespace, kind: "policy-cutoff", proofType: "private-match" }),
    },
    batchNetting: {
      verifierId: hashObject({ namespace, kind: "verifier-id", proofType: "batch-netting" }),
      proofType: proofType.batchNetting,
      mode: "circom-groth16",
      policyCutoffHash: hashObject({ namespace, kind: "policy-cutoff", proofType: "batch-netting" }),
    },
    entitlementClaim: {
      verifierId: hashObject({ namespace, kind: "verifier-id", proofType: "entitlement-claim" }),
      proofType: proofType.entitlementClaim,
      mode: "circom-groth16",
      policyCutoffHash: hashObject({ namespace, kind: "policy-cutoff", proofType: "entitlement-claim" }),
    },
  };

  const assetPolicies = [
    {
      key: "usdc",
      asset: phase0.assets.usdc.sacContractId,
      decimals: 7,
      haircutBps: 0,
      price: "1000000",
      priceEpoch: currentEpoch,
      enabled: true,
    },
    {
      key: phase0.assets.demo[0].key,
      asset: phase0.assets.demo[0].sacContractId,
      decimals: 7,
      haircutBps: 8000,
      price: "125000",
      priceEpoch: currentEpoch,
      enabled: true,
    },
    {
      key: phase0.assets.demo[1].key,
      asset: phase0.assets.demo[1].sacContractId,
      decimals: 7,
      haircutBps: 7500,
      price: "525000",
      priceEpoch: currentEpoch,
      enabled: true,
    },
  ];

  return {
    collateralPolicy: {
      requiredMargin,
      currentEpoch,
      assetPolicies,
    },
    proofGateway: {
      validFromLedger,
      validUntilLedger,
      verifiers: verifierRegistry,
    },
    cctpForwarderPayloadHex: strkeyPayloadHex(cctp.forwarderContract),
    matcher: matcherWallet.address,
    settler: settlerWallet.address,
    operator: complianceWallet.address,
    attestor,
  };
}

async function deployProtocolContracts(buildArtifacts, verifierArtifacts, config) {
  const protocol = {};
  const verifiers = {};

  verifiers.collateralSufficiency = await deployWasmContract({
    label: "mock-proof-verifier",
    wasmPath: buildArtifacts["mock-proof-verifier"],
  });
  verifiers.unencumberedLot = await deployWasmContract({
    label: "unencumbered-lot-verifier",
    wasmPath: buildArtifacts["unencumbered-lot-verifier"],
    constructorArgs: [
      "--verification_key",
      verifierArtifacts.unencumberedLot.verificationKeyHex,
    ],
  });
  verifiers.privateMatch = await deployWasmContract({
    label: "private-match-verifier",
    wasmPath: buildArtifacts["private-match-verifier"],
    constructorArgs: [
      "--verification_key",
      verifierArtifacts.privateMatch.verificationKeyHex,
    ],
  });
  verifiers.batchNetting = await deployWasmContract({
    label: "batch-netting-verifier",
    wasmPath: buildArtifacts["batch-netting-verifier"],
    constructorArgs: [
      "--verification_key",
      verifierArtifacts.batchNetting.verificationKeyHex,
    ],
  });
  verifiers.entitlementClaim = await deployWasmContract({
    label: "entitlement-claim-verifier",
    wasmPath: buildArtifacts["entitlement-claim-verifier"],
    constructorArgs: [
      "--verification_key",
      verifierArtifacts.entitlementClaim.verificationKeyHex,
    ],
  });

  protocol.collateralPolicy = await deployWasmContract({
    label: "collateral-policy",
    wasmPath: buildArtifacts["collateral-policy"],
    constructorArgs: [
      "--admin",
      deployerAddress,
      "--asset_registry",
      phase0.contracts.assetRegistry.contractId,
      "--required_margin",
      config.collateralPolicy.requiredMargin,
      "--current_epoch",
      String(config.collateralPolicy.currentEpoch),
    ],
  });

  protocol.proofGateway = await deployWasmContract({
    label: "proof-gateway",
    wasmPath: buildArtifacts["proof-gateway"],
    constructorArgs: [
      "--admin",
      deployerAddress,
      "--participant_registry",
      phase0.contracts.participantRegistry.contractId,
      "--collateral_policy",
      protocol.collateralPolicy.contractId,
    ],
  });

  protocol.cctpIngressAdapter = await deployWasmContract({
    label: "cctp-ingress-adapter",
    wasmPath: buildArtifacts["cctp-ingress-adapter"],
    constructorArgs: [
      "--admin",
      deployerAddress,
      "--participant_registry",
      phase0.contracts.participantRegistry.contractId,
      "--asset_registry",
      phase0.contracts.assetRegistry.contractId,
      "--compliance_control",
      phase0.contracts.complianceControl.contractId,
      "--usdc_asset",
      phase0.assets.usdc.sacContractId,
      "--forwarder_payload",
      config.cctpForwarderPayloadHex,
      "--expected_destination_domain",
      String(cctp.stellarDomain),
    ],
  });

  protocol.encumbranceRegistry = await deployWasmContract({
    label: "encumbrance-registry",
    wasmPath: buildArtifacts["encumbrance-registry"],
    constructorArgs: [
      "--admin",
      deployerAddress,
      "--participant_registry",
      phase0.contracts.participantRegistry.contractId,
      "--asset_registry",
      phase0.contracts.assetRegistry.contractId,
      "--proof_gateway",
      protocol.proofGateway.contractId,
    ],
  });

  protocol.orderCommitPool = await deployWasmContract({
    label: "order-commit-pool",
    wasmPath: buildArtifacts["order-commit-pool"],
    constructorArgs: [
      "--admin",
      deployerAddress,
      "--participant_registry",
      phase0.contracts.participantRegistry.contractId,
      "--proof_gateway",
      protocol.proofGateway.contractId,
      "--compliance_control",
      phase0.contracts.complianceControl.contractId,
    ],
  });

  protocol.settlementNettingEngine = await deployWasmContract({
    label: "settlement-netting-engine",
    wasmPath: buildArtifacts["settlement-netting-engine"],
    constructorArgs: [
      "--admin",
      deployerAddress,
      "--participant_registry",
      phase0.contracts.participantRegistry.contractId,
      "--proof_gateway",
      protocol.proofGateway.contractId,
      "--compliance_control",
      phase0.contracts.complianceControl.contractId,
      "--order_commit_pool",
      protocol.orderCommitPool.contractId,
    ],
  });

  protocol.corporateActionsEngine = await deployWasmContract({
    label: "corporate-actions-engine",
    wasmPath: buildArtifacts["corporate-actions-engine"],
    constructorArgs: [
      "--admin",
      deployerAddress,
      "--participant_registry",
      phase0.contracts.participantRegistry.contractId,
      "--proof_gateway",
      protocol.proofGateway.contractId,
      "--asset_registry",
      phase0.contracts.assetRegistry.contractId,
      "--compliance_control",
      phase0.contracts.complianceControl.contractId,
    ],
  });

  protocol.auditDisclosureRegistry = await deployWasmContract({
    label: "audit-disclosure-registry",
    wasmPath: buildArtifacts["audit-disclosure-registry"],
    constructorArgs: [
      "--admin",
      deployerAddress,
    ],
  });

  return { protocol, verifiers };
}

function configureProtocolContracts(contracts, config) {
  const txLog = [];
  const operatorSourceName = complianceWallet.name;

  for (const contract of [
    contracts.protocol.collateralPolicy,
    contracts.protocol.proofGateway,
    contracts.protocol.cctpIngressAdapter,
    contracts.protocol.encumbranceRegistry,
    contracts.protocol.orderCommitPool,
    contracts.protocol.settlementNettingEngine,
    contracts.protocol.corporateActionsEngine,
    contracts.protocol.auditDisclosureRegistry,
  ]) {
    txLog.push(invokeContract(
      contract.contractId,
      "set_operator",
      [
        "--admin",
        deployerAddress,
        "--operator",
        config.operator,
        "--enabled",
        "true",
      ],
    ));
  }

  for (const assetPolicy of config.collateralPolicy.assetPolicies) {
    txLog.push(invokeContract(
      contracts.protocol.collateralPolicy.contractId,
      "upsert_asset_policy",
      [
        "--operator",
        config.operator,
        "--asset",
        assetPolicy.asset,
        "--decimals",
        String(assetPolicy.decimals),
        "--haircut_bps",
        String(assetPolicy.haircutBps),
        "--price",
        assetPolicy.price,
        "--price_epoch",
        String(assetPolicy.priceEpoch),
        "--enabled",
        String(assetPolicy.enabled),
      ],
      { sourceName: operatorSourceName },
    ));
  }

  const verifierRoutes = [
    ["collateralSufficiency", contracts.verifiers.collateralSufficiency.contractId],
    ["unencumberedLot", contracts.verifiers.unencumberedLot.contractId],
    ["privateMatch", contracts.verifiers.privateMatch.contractId],
    ["batchNetting", contracts.verifiers.batchNetting.contractId],
    ["entitlementClaim", contracts.verifiers.entitlementClaim.contractId],
  ];

  for (const [key, verifierContractId] of verifierRoutes) {
    const verifierConfig = config.proofGateway.verifiers[key];
    txLog.push(invokeContract(
      contracts.protocol.collateralPolicy.contractId,
      "set_accepted_verifier",
      [
        "--operator",
        config.operator,
        "--proof_type",
        String(verifierConfig.proofType),
        "--verifier_id",
        verifierConfig.verifierId,
        "--enabled",
        "true",
      ],
      { sourceName: operatorSourceName },
    ));
    txLog.push(invokeContract(
      contracts.protocol.proofGateway.contractId,
      "set_verifier_route",
      [
        "--operator",
        config.operator,
        "--verifier_id",
        verifierConfig.verifierId,
        "--verifier",
        verifierContractId,
        "--enabled",
        "true",
      ],
      { sourceName: operatorSourceName },
    ));
    txLog.push(invokeContract(
      contracts.protocol.proofGateway.contractId,
      "set_verifier_policy",
      [
        "--operator",
        config.operator,
        "--verifier_id",
        verifierConfig.verifierId,
        "--enabled",
        "true",
        "--valid_from_ledger",
        String(config.proofGateway.validFromLedger),
        "--valid_until_ledger",
        String(config.proofGateway.validUntilLedger),
        "--policy_cutoff_hash",
        verifierConfig.policyCutoffHash,
      ],
      { sourceName: operatorSourceName },
    ));
  }

  txLog.push(invokeContract(
    contracts.protocol.encumbranceRegistry.contractId,
    "set_attestor",
    [
      "--operator",
      config.operator,
      "--attestor_id",
      config.attestor.attestorId,
      "--public_key",
      config.attestor.publicKeyHex,
      "--enabled",
      "true",
    ],
    { sourceName: operatorSourceName },
  ));

  txLog.push(invokeContract(
    contracts.protocol.orderCommitPool.contractId,
    "set_matcher",
    [
      "--operator",
      config.operator,
      "--matcher",
      config.matcher,
      "--enabled",
      "true",
    ],
    { sourceName: operatorSourceName },
  ));

  txLog.push(invokeContract(
    contracts.protocol.settlementNettingEngine.contractId,
    "set_settler",
    [
      "--operator",
      config.operator,
      "--settler",
      config.settler,
      "--enabled",
      "true",
    ],
    { sourceName: operatorSourceName },
  ));

  return txLog;
}

function verifyDeployment(contracts, config, latestLedger) {
  const policySummary = parseJsonish(invokeView(
    contracts.protocol.collateralPolicy.contractId,
    "get_policy_summary",
    [],
  ));

  const assetPolicies = config.collateralPolicy.assetPolicies.map((assetPolicy) => ({
    key: assetPolicy.key,
    asset: assetPolicy.asset,
    policy: parseJsonish(invokeView(
      contracts.protocol.collateralPolicy.contractId,
      "get_asset_policy",
      ["--asset", assetPolicy.asset],
    )),
  }));

  const acceptedVerifiers = Object.fromEntries(
    Object.entries(config.proofGateway.verifiers).map(([key, verifier]) => [
      key,
      invokeView(
        contracts.protocol.collateralPolicy.contractId,
        "is_verifier_accepted",
        [
          "--proof_type",
          String(verifier.proofType),
          "--verifier_id",
          verifier.verifierId,
        ],
      ),
    ]),
  );

  const attestorRecord = parseJsonish(invokeView(
    contracts.protocol.encumbranceRegistry.contractId,
    "get_attestor",
    ["--attestor_id", config.attestor.attestorId],
  ));

  return {
    verifiedAt: new Date().toISOString(),
    latestLedger,
    policySummary,
    assetPolicies,
    acceptedVerifiers,
    attestorRecord,
    callSmoke: {
      statementHashPreview: stripQuotes(invokeView(
        contracts.protocol.proofGateway.contractId,
        "build_statement_hash",
        [
          "--proof_type",
          String(proofType.privateMatch),
          "--participant_id_hash",
          phase0.participants[1].participantIdHash,
          "--submitter",
          phase0.wallets.matcher.address,
          "--nonce",
          hashObject({ namespace, kind: "smoke-nonce" }),
          "--expiry_ledger",
          String(config.proofGateway.validUntilLedger),
          "--policy_version",
          String(policySummary.policy_version ?? policySummary.policyVersion),
          "--epoch_id",
          String(config.collateralPolicy.currentEpoch),
          "--portfolio_commitment",
          hashObject({ namespace, kind: "smoke-portfolio" }),
          "--required_margin",
          config.collateralPolicy.requiredMargin,
        ],
      )),
    },
  };
}

async function deployWasmContract({ label, wasmPath, constructorArgs = [] }) {
  const wasmHash = hashFileHex(wasmPath);
  log(`uploading ${label}`);
  runStellar([
    "contract",
    "upload",
    "--wasm",
    wasmPath,
    "--source",
    deployerName,
    "--network",
    network,
  ]);

  let contractId = "";
  for (let attempt = 1; attempt <= 12; attempt += 1) {
    try {
      const command = [
        "contract",
        "deploy",
        "--wasm-hash",
        wasmHash,
        "--source",
        deployerName,
        "--network",
        network,
      ];
      if (constructorArgs.length > 0) {
        command.push("--", ...constructorArgs);
      }
      contractId = parseContractId(runStellar(command));
      break;
    } catch (error) {
      const message = errorText(error);
      if (message.includes("Contract Code not found") || message.includes("Wasm does not exist")) {
        sleep(4_000 * attempt);
        continue;
      }
      throw error;
    }
  }

  if (!contractId) {
    fail(`failed to deploy ${label} after waiting for wasm propagation`);
  }

  await waitForContract(contractId);
  return {
    label,
    wasmPath,
    wasmHash,
    contractId,
  };
}

function invokeContract(
  contractId,
  functionName,
  contractArgs,
  { settleMs = 1_500, sourceName = deployerName } = {},
) {
  log(`invoke ${functionName} on ${contractId}`);
  const output = runStellar([
    "contract",
    "invoke",
    "--id",
    contractId,
    "--source",
    sourceName,
    "--network",
    network,
    "--",
    functionName,
    ...contractArgs,
  ]);
  if (settleMs > 0) {
    sleep(settleMs);
  }
  return {
    contractId,
    functionName,
    args: contractArgs,
    txHash: parseTxHash(output),
    raw: output.trim(),
  };
}

function invokeView(contractId, functionName, contractArgs) {
  log(`view ${functionName} on ${contractId}`);
  return runStellar([
    "contract",
    "invoke",
    "--id",
    contractId,
    "--source",
    deployerName,
    "--network",
    network,
    "--send",
    "no",
    "--",
    functionName,
    ...contractArgs,
  ]).trim();
}

async function waitForContract(contractId) {
  for (let attempt = 1; attempt <= 15; attempt += 1) {
    try {
      runStellar([
        "contract",
        "info",
        "interface",
        "--contract-id",
        contractId,
        "--network",
        network,
        "--output",
        "rust",
      ]);
      return;
    } catch (error) {
      const message = errorText(error);
      if (message.includes("Contract not found")) {
        sleep(3_000 * attempt);
        continue;
      }
      throw error;
    }
  }
  fail(`contract ${contractId} did not become queryable in time`);
}

async function fetchLatestLedger() {
  const latestLedgerPage = await fetchJson(`${horizonUrl}/ledgers?order=desc&limit=1`);
  const latestLedger = latestLedgerPage._embedded.records[0];
  return {
    sequence: Number(latestLedger.sequence),
    closedAt: latestLedger.closed_at,
    hash: latestLedger.hash,
  };
}

function encodeVerificationKeyJson(filePath) {
  const vk = JSON.parse(readFileSync(filePath, "utf8"));
  return [
    encodeG1(vk.vk_alpha_1),
    encodeG2(vk.vk_beta_2),
    encodeG2(vk.vk_gamma_2),
    encodeG2(vk.vk_delta_2),
    toHex4(vk.IC.length),
    ...vk.IC.map(encodeG1),
  ].join("");
}

function encodeG1(point) {
  return `${toHex32(point[0])}${toHex32(point[1])}`;
}

function encodeG2(point) {
  return `${encodeFp2(point[0])}${encodeFp2(point[1])}`;
}

function encodeFp2(coords) {
  return `${toHex32(coords[1])}${toHex32(coords[0])}`;
}

function toHex4(value) {
  return Number(value).toString(16).padStart(8, "0");
}

function toHex32(value) {
  return BigInt(value).toString(16).padStart(64, "0");
}

function strkeyPayloadHex(strkey) {
  const decoded = decodeStrKey(strkey);
  if (decoded.payload.length !== 32) {
    fail(`unexpected payload length for strkey ${strkey}`);
  }
  return decoded.payload.toString("hex");
}

function decodeStrKey(strkey) {
  const raw = base32Decode(strkey);
  if (raw.length < 35) {
    fail(`invalid strkey length for ${strkey}`);
  }
  const body = raw.subarray(0, -2);
  const checksum = raw.readUInt16LE(raw.length - 2);
  const expected = crc16Xmodem(body);
  if (checksum !== expected) {
    fail(`invalid strkey checksum for ${strkey}`);
  }
  return {
    version: body[0],
    payload: body.subarray(1),
  };
}

function base32Decode(value) {
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
  let bits = "";
  for (const char of value.replace(/=+$/g, "").toUpperCase()) {
    const index = alphabet.indexOf(char);
    if (index === -1) {
      fail(`invalid base32 character "${char}"`);
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
      if ((crc & 0x8000) !== 0) {
        crc = ((crc << 1) ^ 0x1021) & 0xffff;
      } else {
        crc = (crc << 1) & 0xffff;
      }
    }
  }
  return crc;
}

function hasIdentity(name) {
  return runStellar(["keys", "ls"]).trim().split(/\s+/).filter(Boolean).includes(name);
}

function addressOf(name) {
  return runStellar(["keys", "address", name]).trim();
}

async function fetchJson(url) {
  return JSON.parse(execFileSync("curl", ["-sSfL", url], { encoding: "utf8" }));
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
        (message.includes("TxBadSeq")
          || message.includes("transaction submission timeout")
          || message.includes("transaction simulation failed")
          || message.includes("Networking or low-level protocol error")
          || message.includes("HTTP error: connection error")
          || message.includes("unexpected end of file"))
        && attempt < 5
      ) {
        log(`retrying stellar command after transient failure (${attempt}/5): ${commandArgs[0]} ${commandArgs[1]}`);
        sleep(1_500 * attempt);
        continue;
      }
      throw error;
    }
  }
  fail(`stellar command retry budget exhausted: ${commandArgs.join(" ")}`);
}

function parseJsonish(value) {
  try {
    return JSON.parse(value);
  } catch {
    return value;
  }
}

function parseContractId(output) {
  const match = output.match(/\bC[A-Z2-7]{55}\b/g);
  if (!match?.length) {
    fail(`unable to parse contract id from output:\n${output}`);
  }
  return match[match.length - 1];
}

function parseTxHash(output) {
  const explorerMatch = output.match(/\/tx\/([0-9a-f]{64})/i);
  if (explorerMatch) {
    return explorerMatch[1];
  }
  const hashes = output.match(/\b[0-9a-f]{64}\b/gi);
  return hashes?.[hashes.length - 1] ?? null;
}

function stripQuotes(value) {
  return value.replace(/^"+|"+$/g, "");
}

function hashFileHex(filePath) {
  return createHash("sha256").update(readFileSync(filePath)).digest("hex");
}

function hashObject(value) {
  return createHash("sha256")
    .update(JSON.stringify(sortObject(value)))
    .digest("hex");
}

function sortObject(value) {
  if (Array.isArray(value)) {
    return value.map(sortObject);
  }
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map((key) => [key, sortObject(value[key])]),
    );
  }
  return value;
}

function parseArgs(argv) {
  const parsed = {};
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    if (!token.startsWith("--")) {
      continue;
    }
    const next = argv[index + 1];
    if (!next || next.startsWith("--")) {
      parsed[token.slice(2)] = true;
      continue;
    }
    parsed[token.slice(2)] = next;
    index += 1;
  }
  return parsed;
}

function errorText(error) {
  return [error?.stdout, error?.stderr, error?.message].filter(Boolean).join("\n");
}

function timestampTag() {
  return new Date().toISOString().replace(/[:.]/g, "-").toLowerCase();
}

function sleep(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

function log(message) {
  console.error(`[protocol-testnet] ${message}`);
}

function fail(message) {
  throw new Error(message);
}

main().catch((error) => {
  console.error(errorText(error) || error);
  process.exitCode = 1;
});
