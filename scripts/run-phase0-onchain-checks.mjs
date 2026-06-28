#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

const args = parseArgs(process.argv.slice(2));
const manifestPath = path.resolve(
  args.manifest ?? "deployments/testnet-phase0-demo0628b.json",
);
const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
const network = manifest.network ?? "testnet";
const namespace = manifest.namespace;
const horizonUrl = network === "testnet"
  ? "https://horizon-testnet.stellar.org"
  : fail(`unsupported network "${network}" for phase-0 checks`);
const reportDir = path.resolve(args["out-dir"] ?? "deployments/reports");
const deployerName = manifest.admin?.name ?? manifest.wallets?.deployer?.name ?? "rosca-admin";

async function main() {
  mkdirSync(reportDir, { recursive: true });

  const report = {
    manifestPath,
    namespace,
    network,
    startedAt: new Date().toISOString(),
    step1: await freezeEnvironment(),
    step2: await validatePhaseZeroState(),
    step3: await testRawAssetRail(),
  };
  report.completedAt = new Date().toISOString();

  const reportPath = path.join(
    reportDir,
    `phase0-onchain-checks-${namespace}-${timestampTag()}.json`,
  );
  writeFileSync(reportPath, JSON.stringify(report, null, 2));

  console.error(`[phase0-checks] report written to ${reportPath}`);
  console.error(
    `[phase0-checks] raw asset rail probe wallet: ${report.step3.probeWallet.address}`,
  );
}

async function freezeEnvironment() {
  console.error("[phase0-checks] step 1: freezing live environment state");
  const latestLedgerPage = await fetchJson(
    `${horizonUrl}/ledgers?order=desc&limit=1`,
  );
  const latestLedger = latestLedgerPage._embedded.records[0];

  const walletSnapshots = {};
  for (const [key, wallet] of Object.entries(manifest.wallets)) {
    walletSnapshots[key] = await accountSnapshot(wallet.address);
  }

  return {
    latestLedger: {
      sequence: Number(latestLedger.sequence),
      closedAt: latestLedger.closed_at,
      hash: latestLedger.hash,
    },
    contracts: Object.fromEntries(
      Object.entries(manifest.contracts).map(([key, value]) => [key, value.contractId]),
    ),
    assetContracts: {
      usdc: manifest.assets.usdc.sacContractId,
      demo: manifest.assets.demo.map((asset) => ({
        displaySymbol: asset.displaySymbol,
        assetString: asset.assetString,
        sacContractId: asset.sacContractId,
      })),
    },
    walletSnapshots,
  };
}

async function validatePhaseZeroState() {
  console.error("[phase0-checks] step 2: validating phase-0 contract state");
  const assertions = [];

  const globalPause = invokeView(
    manifest.contracts.complianceControl.contractId,
    "is_globally_paused",
    [],
  );
  assert(globalPause === "false", "global pause should be false", assertions, { globalPause });

  const participantChecks = [];
  for (const participant of manifest.participants) {
    const walletRegistered = invokeView(
      manifest.contracts.participantRegistry.contractId,
      "is_wallet_registered",
      ["--wallet", participant.wallet.address],
    );
    const walletOwner = stripQuotes(invokeView(
      manifest.contracts.participantRegistry.contractId,
      "wallet_owner",
      ["--wallet", participant.wallet.address],
    ));
    const tradeEligible = invokeView(
      manifest.contracts.participantRegistry.contractId,
      "is_participant_trade_eligible",
      [
        "--participant_id_hash",
        participant.participantIdHash,
        "--asset",
        manifest.assets.demo[0].sacContractId,
      ],
    );
    const frozen = invokeView(
      manifest.contracts.complianceControl.contractId,
      "is_participant_frozen",
      ["--participant_id_hash", participant.participantIdHash],
    );

    assert(
      walletRegistered === "true",
      `wallet must be registered for ${participant.key}`,
      assertions,
      { participant: participant.key, walletRegistered },
    );
    assert(
      walletOwner === participant.participantIdHash,
      `wallet owner hash must match participant for ${participant.key}`,
      assertions,
      { participant: participant.key, walletOwner, expected: participant.participantIdHash },
    );
    assert(
      tradeEligible === "true",
      `participant must be trade-eligible for ${participant.key}`,
      assertions,
      { participant: participant.key, tradeEligible },
    );
    assert(
      frozen === "false",
      `participant must not be frozen for ${participant.key}`,
      assertions,
      { participant: participant.key, frozen },
    );

    participantChecks.push({
      key: participant.key,
      walletRegistered,
      walletOwner,
      tradeEligible,
      frozen,
      rawParticipantRecord: invokeView(
        manifest.contracts.participantRegistry.contractId,
        "get_participant",
        ["--participant_id_hash", participant.participantIdHash],
      ),
    });
  }

  const assetChecks = [];
  for (const asset of [...manifest.assets.demo, manifest.assets.usdc]) {
    const supported = invokeView(
      manifest.contracts.assetRegistry.contractId,
      "is_supported_asset",
      ["--asset", asset.sacContractId],
    );
    const settlementEnabled = invokeView(
      manifest.contracts.assetRegistry.contractId,
      "is_asset_settlement_enabled",
      ["--asset", asset.sacContractId],
    );
    const corpActionsEnabled = invokeView(
      manifest.contracts.assetRegistry.contractId,
      "is_asset_corp_actions_enabled",
      ["--asset", asset.sacContractId],
    );
    const paused = invokeView(
      manifest.contracts.complianceControl.contractId,
      "is_asset_paused",
      ["--asset", asset.sacContractId],
    );

    assert(
      supported === "true",
      `${asset.displaySymbol} must be supported`,
      assertions,
      { asset: asset.displaySymbol, supported },
    );
    assert(
      settlementEnabled === "true",
      `${asset.displaySymbol} must have settlement enabled`,
      assertions,
      { asset: asset.displaySymbol, settlementEnabled },
    );
    assert(
      paused === "false",
      `${asset.displaySymbol} must not be paused`,
      assertions,
      { asset: asset.displaySymbol, paused },
    );
    if (asset.displaySymbol === "USDC") {
      assert(
        corpActionsEnabled === "false",
        "USDC should not have corporate actions enabled",
        assertions,
        { asset: asset.displaySymbol, corpActionsEnabled },
      );
    } else {
      assert(
        corpActionsEnabled === "true",
        `${asset.displaySymbol} should have corporate actions enabled`,
        assertions,
        { asset: asset.displaySymbol, corpActionsEnabled },
      );
    }

    assetChecks.push({
      displaySymbol: asset.displaySymbol,
      supported,
      settlementEnabled,
      corpActionsEnabled,
      paused,
      rawAssetRecord: invokeView(
        manifest.contracts.assetRegistry.contractId,
        "get_asset",
        ["--asset", asset.sacContractId],
      ),
    });
  }

  const legalStateChecks = [];
  for (const legalState of manifest.legalStates) {
    const currentState = stripQuotes(invokeView(
      manifest.contracts.legalStateRegistry.contractId,
      "current_state_for_entitlement",
      ["--entitlement_id_hash", legalState.entitlementIdHash],
    ));
    assert(
      currentState === legalState.stateIdHash,
      `current legal state must match manifest for ${legalState.displaySymbol}/${legalState.participantKey}`,
      assertions,
      {
        participant: legalState.participantKey,
        displaySymbol: legalState.displaySymbol,
        currentState,
        expected: legalState.stateIdHash,
      },
    );

    legalStateChecks.push({
      participantKey: legalState.participantKey,
      displaySymbol: legalState.displaySymbol,
      entitlementIdHash: legalState.entitlementIdHash,
      currentState,
      rawStateRecord: invokeView(
        manifest.contracts.legalStateRegistry.contractId,
        "get_state",
        ["--state_id_hash", legalState.stateIdHash],
      ),
    });
  }

  return {
    assertions,
    participantChecks,
    assetChecks,
    legalStateChecks,
  };
}

async function testRawAssetRail() {
  console.error("[phase0-checks] step 3: testing raw asset behavior outside protocol");
  const probeWallet = await ensureProbeWallet();
  const treasury = manifest.wallets.treasury;
  const issuer = manifest.wallets.issuer;

  await ensureTrustline(probeWallet.name, manifest.assets.usdc.assetString);
  for (const asset of manifest.assets.demo) {
    await ensureTrustline(probeWallet.name, asset.assetString);
    await ensureAuthorizedTrustline(issuer.name, probeWallet.address, asset.assetString);
  }

  const probeBefore = await accountSnapshot(probeWallet.address);
  const treasuryBefore = await accountSnapshot(treasury.address);

  const transferTests = [];
  transferTests.push(await roundTripTransfer({
    label: "USDC",
    assetString: manifest.assets.usdc.assetString,
    fromIdentity: treasury.name,
    fromAddress: treasury.address,
    toIdentity: probeWallet.name,
    toAddress: probeWallet.address,
    amount: "0.5000000",
  }));
  for (const asset of manifest.assets.demo) {
    transferTests.push(await roundTripTransfer({
      label: asset.displaySymbol,
      assetString: asset.assetString,
      fromIdentity: treasury.name,
      fromAddress: treasury.address,
      toIdentity: probeWallet.name,
      toAddress: probeWallet.address,
      amount: "10.0000000",
    }));
  }

  const probeAfter = await accountSnapshot(probeWallet.address);
  const treasuryAfter = await accountSnapshot(treasury.address);

  const assertions = [];
  for (const transfer of transferTests) {
    const probeBeforeBalance = assetBalanceOf(probeBefore.balances, transfer.assetString);
    const probeAfterBalance = assetBalanceOf(probeAfter.balances, transfer.assetString);
    assert(
      probeBeforeBalance === probeAfterBalance,
      `${transfer.label} probe balance should round-trip back to its starting value`,
      assertions,
      { label: transfer.label, probeBeforeBalance, probeAfterBalance },
    );
  }

  return {
    probeWallet,
    probeBefore,
    probeAfter,
    treasuryBefore,
    treasuryAfter,
    assertions,
    transferTests,
    note:
      "The unregistered probe wallet was able to receive raw classic-asset transfers once trustlines existed and the issuer authorized them. Registered-wallet enforcement therefore lives at the protocol contract layer, not in the classic asset rail alone.",
  };
}

async function ensureProbeWallet() {
  const probeName = `zkdtcc-${namespace}-probe`;
  if (!hasIdentity(probeName)) {
    runStellar(["keys", "generate", probeName]);
  }
  const address = runStellar(["keys", "address", probeName]).trim();
  if (!(await accountExists(address))) {
    runStellar([
      "tx",
      "new",
      "create-account",
      "--source",
      deployerName,
      "--network",
      network,
      "--destination",
      address,
      "--starting-balance",
      "100000000",
    ]);
    await waitForAccount(address);
  }
  return { name: probeName, address };
}

async function ensureTrustline(identityName, assetString) {
  const address = runStellar(["keys", "address", identityName]).trim();
  const snapshot = await accountSnapshot(address);
  if (snapshot.balances.some((balance) => toAssetString(balance) === assetString)) {
    return;
  }
  runStellar([
    "tx",
    "new",
    "change-trust",
    "--source",
    identityName,
    "--network",
    network,
    "--line",
    assetString,
  ]);
}

async function ensureAuthorizedTrustline(issuerName, trustorAddress, assetString) {
  const snapshot = await accountSnapshot(trustorAddress);
  const trustline = snapshot.balances.find((balance) => toAssetString(balance) === assetString);
  if (trustline?.is_authorized) {
    return;
  }
  runStellar([
    "tx",
    "new",
    "set-trustline-flags",
    "--source",
    issuerName,
    "--network",
    network,
    "--trustor",
    trustorAddress,
    "--asset",
    assetString,
    "--set-authorize",
  ]);
}

async function roundTripTransfer({
  label,
  assetString,
  fromIdentity,
  fromAddress,
  toIdentity,
  toAddress,
  amount,
}) {
  const outbound = runStellar([
    "tx",
    "new",
    "payment",
    "--source",
    fromIdentity,
    "--network",
    network,
    "--destination",
    toAddress,
    "--asset",
    assetString,
    "--amount",
    unitsToStroops(amount),
  ]);
  const inbound = runStellar([
    "tx",
    "new",
    "payment",
    "--source",
    toIdentity,
    "--network",
    network,
    "--destination",
    fromAddress,
    "--asset",
    assetString,
    "--amount",
    unitsToStroops(amount),
  ]);

  return {
    label,
    assetString,
    amount,
    outboundTxHash: parseTxHash(outbound),
    inboundTxHash: parseTxHash(inbound),
  };
}

function invokeView(contractId, functionName, slopArgs) {
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
    ...slopArgs,
  ]).trim();
}

async function accountSnapshot(address) {
  const account = await fetchJson(`${horizonUrl}/accounts/${address}`);
  return {
    address,
    sequence: account.sequence,
    lastModifiedLedger: account.last_modified_ledger,
    balances: account.balances.map((balance) => ({
      assetString: toAssetString(balance),
      balance: balance.balance,
      limit: balance.limit ?? null,
      authorized: balance.is_authorized ?? null,
      assetType: balance.asset_type,
    })),
  };
}

async function accountExists(address) {
  const statusCode = execFileSync(
    "curl",
    ["-s", "-o", "/dev/null", "-w", "%{http_code}", `${horizonUrl}/accounts/${address}`],
    { encoding: "utf8" },
  ).trim();
  if (statusCode === "404") {
    return false;
  }
  if (statusCode !== "200") {
    fail(`failed to query account ${address}: HTTP ${statusCode}`);
  }
  return true;
}

async function waitForAccount(address) {
  for (let attempt = 1; attempt <= 10; attempt += 1) {
    if (await accountExists(address)) {
      return;
    }
    sleep(2_000 * attempt);
  }
  fail(`account ${address} did not appear on Horizon in time`);
}

async function fetchJson(url) {
  return JSON.parse(
    execFileSync("curl", ["-sSfL", url], {
      encoding: "utf8",
    }),
  );
}

function hasIdentity(name) {
  return runStellar(["keys", "ls"])
    .trim()
    .split(/\s+/)
    .filter(Boolean)
    .includes(name);
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
          || message.includes("transaction simulation failed"))
        && attempt < 5
      ) {
        sleep(1_500 * attempt);
        continue;
      }
      throw error;
    }
  }
  fail(`stellar command retry budget exhausted: ${commandArgs.join(" ")}`);
}

function unitsToStroops(units) {
  const [whole, fractional = ""] = String(units).split(".");
  const normalized = `${fractional}0000000`.slice(0, 7);
  return `${whole}${normalized}`;
}

function toAssetString(balance) {
  if (balance.asset_type === "native") {
    return "native";
  }
  return `${balance.asset_code}:${balance.asset_issuer}`;
}

function assetBalanceOf(balances, assetString) {
  const balance = balances.find((entry) => entry.assetString === assetString);
  return balance?.balance ?? "0.0000000";
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

function assert(condition, message, assertions, detail) {
  assertions.push({
    ok: Boolean(condition),
    message,
    detail,
  });
  if (!condition) {
    fail(`${message}: ${JSON.stringify(detail)}`);
  }
}

function timestampTag() {
  return new Date().toISOString().replace(/[:.]/g, "-").toLowerCase();
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

function sleep(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

function fail(message) {
  throw new Error(message);
}

main().catch((error) => {
  console.error(errorText(error) || error);
  process.exitCode = 1;
});
