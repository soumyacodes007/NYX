#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

const args = parseArgs(process.argv.slice(2));
const network = args.network ?? "testnet";
const deployer = args.deployer ?? "rosca-admin";
const namespace = sanitizeNamespace(
  args.namespace ?? new Date().toISOString().replace(/[:.]/g, "-").toLowerCase(),
);
const outDir = path.resolve(args["out-dir"] ?? "deployments");
const horizonUrl = network === "testnet"
  ? "https://horizon-testnet.stellar.org"
  : fail(`unsupported network "${network}"; this bootstrap only targets Stellar testnet`);

const stroopsPerUnit = 10_000_000n;
const zeroHash = "0".repeat(64);
const assetClass = {
  dtcEntitlement: 1,
  usdcSac: 2,
};
const participantRole = {
  institutionTrader: 1,
  complianceOperator: 2,
  matcher: 3,
  settlementOperator: 4,
  issuerOrDtcAdmin: 5,
};
const officialTestnetUsdcIssuer = "GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5";

const identities = {
  issuer: `zkdtcc-${namespace}-issuer`,
  treasury: `zkdtcc-${namespace}-treasury`,
  alpha: `zkdtcc-${namespace}-alpha`,
  beta: `zkdtcc-${namespace}-beta`,
  matcher: `zkdtcc-${namespace}-matcher`,
  settler: `zkdtcc-${namespace}-settler`,
  compliance: `zkdtcc-${namespace}-compliance`,
};

const demoAssets = [
  {
    key: "dtcust10y_ent",
    displaySymbol: "DTCUST10Y-ENT",
    onChainCode: "DTCUST10YENT",
    displayName: "DTC Custodied 10Y Treasury Entitlement",
    description: "Mock DTC-tokenized 10-year U.S. Treasury note entitlement for the institutional demo path.",
    underlying: "10-Year U.S. Treasury Note",
    actionType: "coupon",
    totalSupplyUnits: "1000000.0000000",
    alphaUnits: "100000.0000000",
    betaUnits: "50000.0000000",
  },
  {
    key: "dtcspy_ent",
    displaySymbol: "DTCSPY-ENT",
    onChainCode: "DTCSPYENT",
    displayName: "DTC Custodied S&P 500 ETF Entitlement",
    description: "Mock DTC-tokenized broad-market ETF entitlement for the simplified demo path.",
    underlying: "S&P 500 ETF entitlement",
    actionType: "dividend",
    totalSupplyUnits: "250000.0000000",
    alphaUnits: "35000.0000000",
    betaUnits: "15000.0000000",
  },
];

async function main() {
  log(`bootstrapping phase 0 namespace "${namespace}" on ${network}`);
  mkdirSync(outDir, { recursive: true });

  const deployerAddress = await ensureIdentityAccount(deployer, { createIfMissing: false });
  const walletMatrix = {
    deployer: { name: deployer, address: deployerAddress },
    issuer: await ensureGeneratedAccount(identities.issuer),
    treasury: await ensureGeneratedAccount(identities.treasury),
    alpha: await ensureGeneratedAccount(identities.alpha),
    beta: await ensureGeneratedAccount(identities.beta),
    matcher: await ensureGeneratedAccount(identities.matcher),
    settler: await ensureGeneratedAccount(identities.settler),
    compliance: await ensureGeneratedAccount(identities.compliance),
  };

  const buildArtifacts = buildRegistryArtifacts();
  const contracts = await deployPhaseZeroContracts(buildArtifacts);
  await configureRegistryOperators(contracts, walletMatrix.compliance.address);

  const usdc = await resolveOfficialUsdc();
  await bootstrapUsdcTrustlines(walletMatrix, usdc);

  const issuedAssets = [];
  for (const asset of demoAssets) {
    const issued = await issueDtcAsset(asset, walletMatrix);
    issuedAssets.push(issued);
  }

  const participants = registerParticipants(contracts, walletMatrix, issuedAssets, usdc);
  const legalStates = registerLegalStates(contracts, walletMatrix, issuedAssets, participants);
  const verification = await verifyDeployment(
    contracts,
    walletMatrix,
    issuedAssets,
    usdc,
    participants,
    legalStates,
  );

  const manifest = {
    namespace,
    network,
    deployedAt: new Date().toISOString(),
    admin: walletMatrix.deployer,
    wallets: walletMatrix,
    contracts,
    assets: {
      usdc,
      demo: issuedAssets,
    },
    participants,
    legalStates,
    verification,
  };

  const manifestPath = path.join(outDir, `testnet-phase0-${namespace}.json`);
  writeFileSync(manifestPath, JSON.stringify(manifest, null, 2));

  log(`manifest written to ${manifestPath}`);
  log(`primary institutional asset: ${issuedAssets[0].displaySymbol} (${issuedAssets[0].sacContractId})`);
  log(`secondary explainer asset: ${issuedAssets[1].displaySymbol} (${issuedAssets[1].sacContractId})`);
}

function buildRegistryArtifacts() {
  const packages = [
    "asset-registry",
    "participant-registry",
    "compliance-control",
    "legal-state-registry",
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
  return {
    assetRegistry: path.resolve(".stellar-artifacts/asset_registry.wasm"),
    participantRegistry: path.resolve(".stellar-artifacts/participant_registry.wasm"),
    complianceControl: path.resolve(".stellar-artifacts/compliance_control.wasm"),
    legalStateRegistry: path.resolve(".stellar-artifacts/legal_state_registry.wasm"),
  };
}

async function deployPhaseZeroContracts(buildArtifacts) {
  const complianceControl = await deployWasmContract({
    wasmPath: buildArtifacts.complianceControl,
    alias: `zkdtcc-${namespace}-compliance-control`,
  });
  const assetRegistry = await deployWasmContract({
    wasmPath: buildArtifacts.assetRegistry,
    alias: `zkdtcc-${namespace}-asset-registry`,
  });
  const participantRegistry = await deployWasmContract({
    wasmPath: buildArtifacts.participantRegistry,
    alias: `zkdtcc-${namespace}-participant-registry`,
  });
  const legalStateRegistry = await deployWasmContract({
    wasmPath: buildArtifacts.legalStateRegistry,
    alias: `zkdtcc-${namespace}-legal-state-registry`,
  });

  return {
    complianceControl,
    assetRegistry,
    participantRegistry,
    legalStateRegistry,
  };
}

async function configureRegistryOperators(contracts, complianceWallet) {
  const operatorArgs = [
    "--admin",
    deployer,
    "--operator",
    complianceWallet,
    "--enabled",
    "true",
  ];

  log("setting compliance wallet as operator on phase 0 contracts");
  invokeContract(contracts.complianceControl.contractId, "set_operator", operatorArgs);
  invokeContract(contracts.assetRegistry.contractId, "set_operator", operatorArgs);
  invokeContract(contracts.participantRegistry.contractId, "set_operator", operatorArgs);
  invokeContract(contracts.legalStateRegistry.contractId, "set_operator", operatorArgs);
}

async function resolveOfficialUsdc() {
  log("resolving official Stellar testnet USDC");
  const assetResponse = await fetchJson(
    `${horizonUrl}/assets?asset_code=USDC&asset_issuer=${officialTestnetUsdcIssuer}`,
  );
  const record = assetResponse?._embedded?.records?.[0];
  if (!record?.contract_id) {
    fail("could not resolve official testnet USDC contract id from Horizon");
  }

  return {
    displaySymbol: "USDC",
    onChainCode: "USDC",
    displayName: "Circle Testnet USDC",
    assetString: `USDC:${officialTestnetUsdcIssuer}`,
    issuer: officialTestnetUsdcIssuer,
    sacContractId: record.contract_id,
    flags: record.flags,
    metadataHash: hashObject({
      symbol: "USDC",
      issuer: officialTestnetUsdcIssuer,
      contractId: record.contract_id,
      flags: record.flags,
      source: "official-circle-testnet",
      network,
    }),
    issuerPolicyHash: hashObject({
      transfer: "open-trustline",
      authRequired: Boolean(record.flags.auth_required),
      authRevocable: Boolean(record.flags.auth_revocable),
      clawbackEnabled: Boolean(record.flags.auth_clawback_enabled),
      settlementAsset: true,
      network,
    }),
    jurisdictionPolicyHash: hashObject({
      jurisdiction: "US",
      settlementAsset: "Circle testnet USDC",
      network,
    }),
    transferClassHash: hashObject({
      transferClass: "stablecoin-settlement",
      authModel: "issuer-open",
      network,
    }),
    assetPermissionsHash: hashObject({
      allowedRoles: [
        "InstitutionTrader",
        "SettlementOperator",
        "IssuerOrDtcAdmin",
        "ComplianceOperator",
      ],
    }),
  };
}

async function bootstrapUsdcTrustlines(walletMatrix, usdc) {
  log("creating USDC settlement trustlines for treasury and institutions");
  for (const wallet of [walletMatrix.treasury, walletMatrix.alpha, walletMatrix.beta]) {
    runStellar([
      "tx",
      "new",
      "change-trust",
      "--source",
      wallet.name,
      "--network",
      network,
      "--line",
      usdc.assetString,
    ]);
  }
}

async function issueDtcAsset(asset, walletMatrix) {
  const assetString = `${asset.onChainCode}:${walletMatrix.issuer.address}`;
  log(`configuring issuer flags for ${asset.displaySymbol} as ${asset.onChainCode}`);
  runStellar([
    "tx",
    "new",
    "set-options",
    "--source",
    walletMatrix.issuer.name,
    "--network",
    network,
    "--set-required",
    "--set-revocable",
    "--set-clawback-enabled",
  ]);

  log(`creating trustlines for ${asset.displaySymbol}`);
  for (const wallet of [walletMatrix.treasury, walletMatrix.alpha, walletMatrix.beta]) {
    runStellar([
      "tx",
      "new",
      "change-trust",
      "--source",
      wallet.name,
      "--network",
      network,
      "--line",
      assetString,
    ]);
    runStellar([
      "tx",
      "new",
      "set-trustline-flags",
      "--source",
      walletMatrix.issuer.name,
      "--network",
      network,
      "--trustor",
      wallet.address,
      "--asset",
      assetString,
      "--set-authorize",
    ]);
  }

  const totalSupply = unitsToStroops(asset.totalSupplyUnits);
  const alphaBalance = unitsToStroops(asset.alphaUnits);
  const betaBalance = unitsToStroops(asset.betaUnits);

  log(`issuing ${asset.displaySymbol} to treasury and demo institutions`);
  runStellar([
    "tx",
    "new",
    "payment",
    "--source",
    walletMatrix.issuer.name,
    "--network",
    network,
    "--destination",
    walletMatrix.treasury.address,
    "--asset",
    assetString,
    "--amount",
    totalSupply.toString(),
  ]);
  runStellar([
    "tx",
    "new",
    "payment",
    "--source",
    walletMatrix.treasury.name,
    "--network",
    network,
    "--destination",
    walletMatrix.alpha.address,
    "--asset",
    assetString,
    "--amount",
    alphaBalance.toString(),
  ]);
  runStellar([
    "tx",
    "new",
    "payment",
    "--source",
    walletMatrix.treasury.name,
    "--network",
    network,
    "--destination",
    walletMatrix.beta.address,
    "--asset",
    assetString,
    "--amount",
    betaBalance.toString(),
  ]);

  const sacContractId = await deployAssetContract(
    assetString,
    `zkdtcc-${namespace}-${asset.key}-sac`,
  );

  const metadata = {
    displaySymbol: asset.displaySymbol,
    onChainCode: asset.onChainCode,
    displayName: asset.displayName,
    description: asset.description,
    underlying: asset.underlying,
    sourceOfTruth: "mock DTC entitlement for Stellar testnet demo",
    issuerAccount: walletMatrix.issuer.address,
    distributorAccount: walletMatrix.treasury.address,
    network,
    decimals: 7,
  };
  const issuerPolicy = {
    registeredWalletsOnly: true,
    issuerAuthorized: true,
    revocable: true,
    clawbackEnabled: true,
    reversibleTransferAuthority: "issuer / DTC admin",
    lifecycle: ["settlement", "corporate_actions"],
    network,
  };
  const transferPolicy = {
    transferClass: "registered_wallets_only",
    displaySymbol: asset.displaySymbol,
    useCase: "institutional entitlement settlement",
    network,
  };
  const jurisdictionPolicy = {
    jurisdiction: "US",
    audience: "qualified demo participants",
    walletRestriction: "registered-wallet-only",
    network,
  };
  const permissions = {
    allowedRoles: [
      "InstitutionTrader",
      "SettlementOperator",
      "IssuerOrDtcAdmin",
      "ComplianceOperator",
    ],
    assetDisplaySymbol: asset.displaySymbol,
  };

  return {
    ...asset,
    assetString,
    issuer: walletMatrix.issuer.address,
    distributor: walletMatrix.treasury.address,
    sacContractId,
    metadataHash: hashObject(metadata),
    issuerPolicyHash: hashObject(issuerPolicy),
    transferClassHash: hashObject(transferPolicy),
    jurisdictionPolicyHash: hashObject(jurisdictionPolicy),
    assetPermissionsHash: hashObject(permissions),
    totalSupplyUnits: asset.totalSupplyUnits,
    balances: {
      treasuryUnits: stroopsToUnits(totalSupply - alphaBalance - betaBalance),
      alphaUnits: asset.alphaUnits,
      betaUnits: asset.betaUnits,
    },
    metadata,
    issuerPolicy,
  };
}

function registerParticipants(contracts, walletMatrix, issuedAssets, usdc) {
  log("registering participant matrix");
  const participantPermissions = hashObject({
    assets: [
      ...issuedAssets.map((asset) => asset.displaySymbol),
      usdc.displaySymbol,
    ],
    environment: network,
  });

  const records = [
    participantRecord("treasury", walletMatrix.treasury, participantRole.issuerOrDtcAdmin),
    participantRecord("alpha", walletMatrix.alpha, participantRole.institutionTrader),
    participantRecord("beta", walletMatrix.beta, participantRole.institutionTrader),
    participantRecord("matcher", walletMatrix.matcher, participantRole.matcher),
    participantRecord("settler", walletMatrix.settler, participantRole.settlementOperator),
    participantRecord("compliance", walletMatrix.compliance, participantRole.complianceOperator),
  ];

  for (const record of records) {
    invokeContract(contracts.participantRegistry.contractId, "register_participant", [
      "--operator",
      deployer,
      "--participant_id_hash",
      record.participantIdHash,
      "--primary_wallet",
      record.wallet.address,
      "--role",
      String(record.role),
      "--credential_root",
      record.credentialRoot,
      "--legal_entity_hash",
      record.legalEntityHash,
      "--jurisdiction_hash",
      record.jurisdictionHash,
    ]);
    invokeContract(contracts.participantRegistry.contractId, "set_permissions_hash", [
      "--operator",
      deployer,
      "--participant_id_hash",
      record.participantIdHash,
      "--permissions_hash",
      participantPermissions,
    ]);
    record.permissionsHash = participantPermissions;
  }

  for (const asset of issuedAssets) {
    registerAssetInRegistry(contracts.assetRegistry.contractId, asset, assetClass.dtcEntitlement);
  }
  registerAssetInRegistry(contracts.assetRegistry.contractId, usdc, assetClass.usdcSac);

  return records;
}

function registerLegalStates(contracts, walletMatrix, issuedAssets, participants) {
  log("recording mock legal-state mappings for live entitlement balances");
  const eventDate = Math.floor(Date.now() / 1000);
  const participantByKey = new Map(participants.map((record) => [record.key, record]));
  const entries = [];

  for (const asset of issuedAssets) {
    const holdings = [
      {
        holderKey: "treasury",
        wallet: walletMatrix.treasury,
        quantityUnits: asset.balances.treasuryUnits,
      },
      {
        holderKey: "alpha",
        wallet: walletMatrix.alpha,
        quantityUnits: asset.balances.alphaUnits,
      },
      {
        holderKey: "beta",
        wallet: walletMatrix.beta,
        quantityUnits: asset.balances.betaUnits,
      },
    ];

    for (const holding of holdings) {
      const participant = participantByKey.get(holding.holderKey);
      const entitlementIdHash = hashObject({
        namespace,
        participant: holding.holderKey,
        displaySymbol: asset.displaySymbol,
        wallet: holding.wallet.address,
      });
      const stateCommitment = hashObject({
        namespace,
        entitlementIdHash,
        quantityUnits: holding.quantityUnits,
        sacContractId: asset.sacContractId,
        eventDate,
        issuerPolicyHash: asset.issuerPolicyHash,
      });
      const stateIdHash = hashObject({
        namespace,
        entitlementIdHash,
        stateCommitment,
        eventDate,
      });

      invokeContract(contracts.legalStateRegistry.contractId, "record_state", [
        "--operator",
        deployer,
        "--state_id_hash",
        stateIdHash,
        "--participant_id_hash",
        participant.participantIdHash,
        "--wallet",
        holding.wallet.address,
        "--entitlement_id_hash",
        entitlementIdHash,
        "--asset",
        asset.sacContractId,
        "--event_date",
        String(eventDate),
        "--issuer_policy_hash",
        asset.issuerPolicyHash,
        "--state_commitment",
        stateCommitment,
      ]);

      entries.push({
        participantKey: holding.holderKey,
        participantIdHash: participant.participantIdHash,
        wallet: holding.wallet.address,
        displaySymbol: asset.displaySymbol,
        asset: asset.sacContractId,
        entitlementIdHash,
        stateIdHash,
        stateCommitment,
        quantityUnits: holding.quantityUnits,
        eventDate,
      });
    }
  }

  return entries;
}

async function verifyDeployment(
  contracts,
  walletMatrix,
  issuedAssets,
  usdc,
  participants,
  legalStates,
) {
  log("verifying registry state and live Horizon balances");
  const assetRegistryChecks = [];
  for (const asset of [...issuedAssets, usdc]) {
    const supported = invokeContract(contracts.assetRegistry.contractId, "is_supported_asset", [
      "--asset",
      asset.sacContractId,
    ]).trim();
    assetRegistryChecks.push({
      displaySymbol: asset.displaySymbol,
      sacContractId: asset.sacContractId,
      isSupportedAsset: supported,
    });
  }

  const walletOwnerChecks = [];
  for (const participant of participants) {
    const owner = invokeContract(contracts.participantRegistry.contractId, "wallet_owner", [
      "--wallet",
      participant.wallet.address,
    ]).trim();
    walletOwnerChecks.push({
      participantKey: participant.key,
      wallet: participant.wallet.address,
      owner,
    });
  }

  const legalStateChecks = [];
  for (const state of legalStates.slice(0, 3)) {
    const current = invokeContract(
      contracts.legalStateRegistry.contractId,
      "current_state_for_entitlement",
      ["--entitlement_id_hash", state.entitlementIdHash],
    ).trim();
    legalStateChecks.push({
      entitlementIdHash: state.entitlementIdHash,
      currentStateIdHash: current,
    });
  }

  const horizonBalances = {};
  for (const wallet of [walletMatrix.treasury, walletMatrix.alpha, walletMatrix.beta]) {
    const account = await fetchJson(`${horizonUrl}/accounts/${wallet.address}`);
    horizonBalances[wallet.name] = account.balances.filter(
      (balance) => balance.asset_type !== "native",
    );
  }

  return {
    assetRegistryChecks,
    walletOwnerChecks,
    legalStateChecks,
    horizonBalances,
  };
}

function participantRecord(key, wallet, role) {
  return {
    key,
    role,
    wallet,
    participantIdHash: hashObject({ namespace, kind: "participant", key }),
    credentialRoot: hashObject({
      namespace,
      kind: "credential",
      key,
      wallet: wallet.address,
      kyc: "approved",
      sanctions: "clear",
    }),
    legalEntityHash: hashObject({
      namespace,
      kind: "legal-entity",
      key,
      lei: `MOCK-${namespace.toUpperCase()}-${key.toUpperCase()}`,
    }),
    jurisdictionHash: hashObject({
      namespace,
      kind: "jurisdiction",
      key,
      country: "US",
      venue: "Stellar testnet",
    }),
  };
}

function registerAssetInRegistry(contractId, asset, assetClassValue) {
  invokeContract(contractId, "register_asset", [
    "--operator",
    deployer,
    "--asset",
    asset.sacContractId,
    "--asset_id_hash",
    hashObject({
      namespace,
      displaySymbol: asset.displaySymbol,
      assetString: asset.assetString,
      network,
    }),
    "--issuer",
    asset.issuer,
    "--asset_class",
    String(assetClassValue),
    "--uses_sac",
    "true",
    "--requires_registered_wallets",
    assetClassValue === assetClass.dtcEntitlement ? "true" : "false",
    "--requires_issuer_auth",
    assetClassValue === assetClass.dtcEntitlement ? "true" : "false",
    "--clawback_enabled",
    assetClassValue === assetClass.dtcEntitlement ? "true" : "false",
    "--metadata_hash",
    asset.metadataHash,
    "--issuer_policy_hash",
    asset.issuerPolicyHash,
  ]);
  invokeContract(contractId, "set_transfer_policy", [
    "--operator",
    deployer,
    "--asset",
    asset.sacContractId,
    "--settlement_enabled",
    "true",
    "--corporate_actions_enabled",
    assetClassValue === assetClass.usdcSac ? "false" : "true",
    "--jurisdiction_policy_hash",
    asset.jurisdictionPolicyHash,
    "--transfer_class_hash",
    asset.transferClassHash,
  ]);
  invokeContract(contractId, "set_asset_permissions_hash", [
    "--operator",
    deployer,
    "--asset",
    asset.sacContractId,
    "--asset_permissions_hash",
    asset.assetPermissionsHash,
  ]);
}

async function deployWasmContract({ wasmPath, alias }) {
  const wasmHash = hashFileHex(wasmPath);
  log(`uploading ${path.basename(wasmPath)} as ${alias}`);
  runStellar([
    "contract",
    "upload",
    "--wasm",
    wasmPath,
    "--source",
    deployer,
    "--network",
    network,
  ]);

  let contractId = "";
  for (let attempt = 1; attempt <= 12; attempt += 1) {
    try {
      const output = runStellar([
        "contract",
        "deploy",
        "--wasm-hash",
        wasmHash,
        "--source",
        deployer,
        "--network",
        network,
        "--alias",
        alias,
        "--",
        "--admin",
        deployer,
      ]);
      contractId = parseContractId(output);
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
    fail(`failed to deploy ${alias} after waiting for wasm propagation`);
  }

  await waitForContract(contractId);
  return {
    alias,
    wasmPath,
    wasmHash,
    contractId,
  };
}

async function deployAssetContract(assetString, alias) {
  log(`deploying SAC wrapper for ${assetString}`);
  let contractId = "";
  try {
    const output = runStellar([
      "contract",
      "asset",
      "deploy",
      "--asset",
      assetString,
      "--source",
      deployer,
      "--network",
      network,
      "--alias",
      alias,
    ]);
    contractId = parseContractId(output);
  } catch (error) {
    const message = errorText(error);
    if (!message.includes("contract already exists")) {
      throw error;
    }
    contractId = runStellar([
      "contract",
      "id",
      "asset",
      "--asset",
      assetString,
      "--network",
      network,
    ]).trim();
  }
  await waitForContract(contractId);
  return contractId;
}

function invokeContract(contractId, functionName, contractArgs, { settleMs = 1_500 } = {}) {
  for (let attempt = 1; attempt <= 5; attempt += 1) {
    try {
      const output = runStellar([
        "contract",
        "invoke",
        "--id",
        contractId,
        "--source",
        deployer,
        "--network",
        network,
        "--",
        functionName,
        ...contractArgs,
      ]);
      if (settleMs > 0) {
        sleep(settleMs);
      }
      return output;
    } catch (error) {
      const message = errorText(error);
      if (message.includes("transaction simulation failed") && attempt < 5) {
        sleep(2_000 * attempt);
        continue;
      }
      throw error;
    }
  }
  fail(`unreachable contract invoke retry state for ${functionName}`);
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

async function ensureGeneratedAccount(name) {
  if (!hasIdentity(name)) {
    log(`generating key ${name}`);
    runStellar(["keys", "generate", name]);
  }
  const address = addressOf(name);
  if (!(await accountExists(address))) {
    log(`funding ${name} from ${deployer}`);
    runStellar([
      "tx",
      "new",
      "create-account",
      "--source",
      deployer,
      "--network",
      network,
      "--destination",
      address,
      "--starting-balance",
      "500000000",
    ]);
  }
  return { name, address };
}

async function ensureIdentityAccount(name, { createIfMissing }) {
  if (!hasIdentity(name)) {
    if (!createIfMissing) {
      fail(`identity "${name}" is not available in stellar-cli`);
    }
    runStellar(["keys", "generate", name]);
  }
  const address = addressOf(name);
  if (!(await accountExists(address))) {
    if (!createIfMissing) {
      fail(`identity "${name}" exists locally but has no on-chain account on ${network}`);
    }
    runStellar([
      "tx",
      "new",
      "create-account",
      "--source",
      deployer,
      "--network",
      network,
      "--destination",
      address,
      "--starting-balance",
      "500000000",
    ]);
  }
  return address;
}

function hasIdentity(name) {
  const identitiesList = runStellar(["keys", "ls"]).trim().split(/\s+/).filter(Boolean);
  return identitiesList.includes(name);
}

function addressOf(name) {
  return runStellar(["keys", "address", name]).trim();
}

async function accountExists(address) {
  const statusCode = execFileSync(
    "curl",
    ["-s", "-o", "/dev/null", "-w", "%{http_code}", `${horizonUrl}/accounts/${address}`],
    { cwd: process.cwd(), encoding: "utf8" },
  ).trim();
  if (statusCode === "404") {
    return false;
  }
  if (statusCode !== "200") {
    fail(`failed to query Horizon account ${address}: HTTP ${statusCode}`);
  }
  return true;
}

async function fetchJson(url) {
  const response = execFileSync("curl", ["-sSfL", url], {
    cwd: process.cwd(),
    encoding: "utf8",
  });
  return JSON.parse(response);
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
        (message.includes("TxBadSeq") || message.includes("transaction submission timeout"))
        && attempt < 5
      ) {
        sleep(1_500 * attempt);
        continue;
      }
      throw error;
    }
  }
  fail(`unreachable stellar command retry state for: ${commandArgs.join(" ")}`);
}

function parseContractId(output) {
  const match = output.match(/\bC[A-Z2-7]{55}\b/g);
  if (!match?.length) {
    fail(`unable to parse contract id from output:\n${output}`);
  }
  return match[match.length - 1];
}

function unitsToStroops(units) {
  const [wholePart, fractionalPart = ""] = String(units).split(".");
  const normalizedFractional = `${fractionalPart}0000000`.slice(0, 7);
  return BigInt(wholePart) * stroopsPerUnit + BigInt(normalizedFractional);
}

function stroopsToUnits(stroops) {
  const whole = stroops / stroopsPerUnit;
  const fractional = stroops % stroopsPerUnit;
  return `${whole.toString()}.${fractional.toString().padStart(7, "0")}`;
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

function sanitizeNamespace(value) {
  return String(value)
    .toLowerCase()
    .replace(/[^a-z0-9-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .replace(/-+/g, "-")
    .slice(0, 32);
}

function errorText(error) {
  return [error?.stdout, error?.stderr, error?.message].filter(Boolean).join("\n");
}

function sleep(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

function log(message) {
  console.error(`[phase0-testnet] ${message}`);
}

function fail(message) {
  throw new Error(message);
}

main().catch((error) => {
  console.error(errorText(error) || error);
  process.exitCode = 1;
});
