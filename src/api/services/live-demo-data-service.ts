import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { readFileSync } from "node:fs";
import path from "node:path";

import type {
  AssetSummary,
  AuditAction,
  AuditCaseView,
  ComplianceIncident,
  ComplianceSnapshot,
  OrderFlowSummary,
  OverviewSummary,
  ParticipantSummary,
  ProofScenario,
  SettlementSummary,
} from "../types.js";

const execFileAsync = promisify(execFile);

const DEFAULT_PHASE0_PATH = path.resolve(process.cwd(), "deployments", "testnet-phase0-demo0628b.json");
const DEFAULT_PROTOCOL_PATH = path.resolve(process.cwd(), "deployments", "testnet-protocol-demo0628b.json");
const ZERO_HASH = "0".repeat(64);
const U32_MAX = 4_294_967_295;
const CACHE_TTL_MS = 60_000;

type JsonRecord = Record<string, any>;
type RpcEventRecord = {
  contractId: string;
  txHash: string;
  ledger: number;
  ledgerClosedAt: string;
  topic: string[];
  value: string;
};

type DecodedEvent = {
  contractId: string;
  txHash: string;
  ledger: number;
  ledgerClosedAt: string;
  topic: string;
  data: Record<string, any>;
};

type ParticipantMeta = {
  key: string;
  displayName: string;
  walletName?: string;
  address?: string;
  participantIdHash?: string;
};

const PARTICIPANT_DISPLAY_NAMES: Record<string, string> = {
  treasury: "Treasury",
  alpha: "Alpha",
  beta: "Beta",
  gamma: "Gamma",
  matcher: "Matcher",
  settler: "Settler",
  compliance: "Compliance",
  issuer: "Issuer",
  auditor: "Auditor",
  regulator: "Regulator",
};

const ROLE_LABELS: Record<number, string> = {
  1: "Institution Trader",
  2: "Compliance Operator",
  3: "Matcher",
  4: "Settlement Operator",
  5: "Issuer / DTC Admin",
  6: "Auditor",
  7: "Regulator",
};

const PARTICIPANT_STATUS_LABELS: Record<number, string> = {
  1: "Pending",
  2: "Active",
  3: "Suspended",
  4: "Revoked",
};

const KYC_STATUS_LABELS: Record<number, string> = {
  1: "Approved",
  2: "Pending",
  3: "Expired",
  4: "Rejected",
};

const SANCTIONS_STATUS_LABELS: Record<number, string> = {
  1: "Clear",
  2: "Review",
  3: "Blocked",
};

const ASSET_CLASS_LABELS: Record<number, string> = {
  1: "DTC Entitlement",
  2: "Settlement Cash",
  3: "Mock Regulated Asset",
  4: "SEP-57 / T-REX-like Asset",
  5: "Other",
};

const ASSET_STATUS_LABELS: Record<number, string> = {
  1: "Pending",
  2: "Active",
  3: "Suspended",
  4: "Retired",
};

const PROOF_TYPE_LABELS: Record<number, string> = {
  1: "Eligibility",
  2: "Collateral Sufficiency",
  3: "Unencumbered Lot",
  4: "Private Match",
  5: "Batch Netting",
  6: "Entitlement Claim",
};

const PROOF_TYPE_KEYS: Record<number, string> = {
  1: "eligibility",
  2: "collateral",
  3: "unencumbered",
  4: "private-match",
  5: "batch-netting",
  6: "entitlement-claim",
};

export class LiveDemoDataService {
  private readonly phase0: JsonRecord;
  private readonly protocol: JsonRecord;
  private readonly cache = new Map<string, { expiresAt: number; value: Promise<any> }>();

  constructor(
    phase0Path: string = DEFAULT_PHASE0_PATH,
    protocolPath: string = DEFAULT_PROTOCOL_PATH,
  ) {
    this.phase0 = this.readJson(phase0Path);
    this.protocol = this.readJson(protocolPath);
  }

  async getOverview(): Promise<OverviewSummary> {
    const [
      participants,
      assets,
      proofs,
      settlements,
      compliance,
      disclosureAction,
      latestLedgerSequence,
    ] = await Promise.all([
      this.getParticipants(),
      this.getAssets(),
      this.getProofScenarios(),
      this.getSettlementSummaries(),
      this.getComplianceSnapshot(),
      this.getLatestDisclosureAction(),
      this.getLatestLedgerSequence(),
    ]);

    return {
      protocolName: "zk-DTCC Demo API",
      environment: this.phase0.namespace ?? "demo",
      network: this.network,
      protocolLive: true,
      globalPause: compliance.globalPause,
      latestLedgerSequence,
      participantCount: participants.length,
      supportedAssetCount: assets.length,
      recentProofCount: proofs.length,
      recentSettlementCount: settlements.length,
      recentComplianceActionCount: compliance.recentIncident ? 1 : 0,
      institutions: participants
        .filter((participant) => participant.role === "Institution Trader" || participant.key === "treasury")
        .map((participant) => participant.displayName),
      supportedAssets: assets.map((asset) => asset.displayName),
      latestSettlement: settlements[0] ?? null,
      latestComplianceIncident: compliance.recentIncident,
      latestDisclosureAction: disclosureAction,
    };
  }

  async getParticipants(): Promise<ParticipantSummary[]> {
    return this.cached("participants", async () => {
      const metaById = new Map<string, ParticipantMeta>();
      const metaByAddress = new Map<string, ParticipantMeta>();
      const knownParticipantIds = new Set<string>();

      for (const participant of this.phase0Participants) {
        const key = String(participant.key);
        const address = String(participant.wallet?.address ?? "");
        const meta: ParticipantMeta = {
          key,
          displayName: PARTICIPANT_DISPLAY_NAMES[key] ?? this.humanizeKey(key),
          walletName: String(participant.wallet?.name ?? ""),
          address,
          participantIdHash: String(participant.participantIdHash),
        };
        metaById.set(meta.participantIdHash!, meta);
        knownParticipantIds.add(meta.participantIdHash!);
        if (address) {
          metaByAddress.set(address, meta);
        }
      }

      const localIdentityMeta = await this.getLocalIdentityMeta();
      for (const meta of localIdentityMeta) {
        if (meta.address) {
          metaByAddress.set(meta.address, metaByAddress.get(meta.address) ?? meta);
        }
      }

      const candidateAddresses = new Set<string>();
      for (const wallet of Object.values(this.phase0.wallets ?? {}) as Array<{ address?: string }>) {
        if (wallet && typeof wallet === "object" && wallet.address) {
          candidateAddresses.add(String(wallet.address));
        }
      }
      for (const meta of localIdentityMeta) {
        if (meta.address) {
          candidateAddresses.add(meta.address);
        }
      }

      const discoveredIds = await Promise.all(
        [...candidateAddresses].map(async (address) => {
          try {
            return await this.viewContract(
              this.phase0.contracts.participantRegistry.contractId,
              "wallet_owner",
              ["--wallet", address],
            ) as string;
          } catch {
            return null;
          }
        }),
      );

      for (const participantIdHash of discoveredIds) {
        if (participantIdHash) {
          knownParticipantIds.add(participantIdHash);
        }
      }

      const participantRecords = await Promise.all(
        [...knownParticipantIds].map(async (participantIdHash) => {
          const record = await this.viewContract(
            this.phase0.contracts.participantRegistry.contractId,
            "get_participant",
            ["--participant_id_hash", participantIdHash],
          );
          return {
            participantIdHash,
            record,
          };
        }),
      );

      const frozenFlags = await Promise.all(
        participantRecords.map(({ participantIdHash }) =>
          this.viewContract(
            this.phase0.contracts.complianceControl.contractId,
            "is_participant_frozen",
            ["--participant_id_hash", participantIdHash],
          ) as Promise<boolean>,
        ),
      );

      return participantRecords
        .map(({ participantIdHash, record }, index) => {
          const primaryWallet = String(record.primary_wallet ?? "");
          const meta = metaById.get(participantIdHash) ?? metaByAddress.get(primaryWallet) ?? {
            key: this.inferParticipantKey(primaryWallet, participantIdHash),
            displayName: this.humanizeKey(this.inferParticipantKey(primaryWallet, participantIdHash)),
          };

          return {
            key: meta.key,
            displayName: meta.displayName,
            role: ROLE_LABELS[Number(record.role)] ?? `Role ${record.role}`,
            participantIdHash,
            primaryWallet,
            walletCount: Number(record.wallet_count ?? 0),
            legalEntityHash: String(record.legal_entity_hash ?? ZERO_HASH),
            jurisdictionHash: String(record.jurisdiction_hash ?? ZERO_HASH),
            credentialRoot: String(record.credential_root ?? ZERO_HASH),
            participantStatus: PARTICIPANT_STATUS_LABELS[Number(record.status)] ?? String(record.status),
            kycStatus: KYC_STATUS_LABELS[Number(record.kyc_status)] ?? String(record.kyc_status),
            sanctionsStatus: SANCTIONS_STATUS_LABELS[Number(record.sanctions_status)] ?? String(record.sanctions_status),
            credentialExpiryLedger: this.normalizeLedger(record.credential_expiry_ledger),
            reviewCaseId: this.normalizeHash(record.review_case_id),
            createdLedger: this.normalizeLedger(record.created_ledger),
            updatedLedger: this.normalizeLedger(record.updated_ledger),
            currentFrozen: Boolean(frozenFlags[index]),
          } satisfies ParticipantSummary;
        })
        .sort((left, right) => {
          const roleOrder = Number(this.phase0ParticipantsByKey[left.key]?.role ?? 99) - Number(this.phase0ParticipantsByKey[right.key]?.role ?? 99);
          if (roleOrder !== 0) {
            return roleOrder;
          }
          return left.displayName.localeCompare(right.displayName);
        });
    });
  }

  async getParticipant(participantKey: string): Promise<ParticipantSummary | null> {
    const participants = await this.getParticipants();
    return participants.find((participant) => participant.key === participantKey) ?? null;
  }

  async getAssets(): Promise<AssetSummary[]> {
    return this.cached("assets", async () => {
      const catalogs = [
        ...(Array.isArray(this.phase0.assets?.demo) ? this.phase0.assets.demo : []),
        this.phase0.assets?.usdc,
      ].filter(Boolean);

      const results = await Promise.all(
        catalogs.map(async (catalogAsset: JsonRecord) => {
          const sacContractId = String(catalogAsset.sacContractId);
          const liveRecord = await this.viewContract(
            this.phase0.contracts.assetRegistry.contractId,
            "get_asset",
            ["--asset", sacContractId],
          );
          const paused = await this.viewContract(
            this.phase0.contracts.complianceControl.contractId,
            "is_asset_paused",
            ["--asset", sacContractId],
          ) as boolean;

          return {
            key: String(catalogAsset.key ?? catalogAsset.displaySymbol?.toLowerCase() ?? sacContractId),
            displayName: String(catalogAsset.displaySymbol ?? catalogAsset.displayName ?? sacContractId),
            symbol: String(catalogAsset.displaySymbol ?? catalogAsset.onChainCode ?? sacContractId),
            assetString: String(catalogAsset.assetString ?? ""),
            sacContractId,
            issuer: String(catalogAsset.issuer ?? liveRecord.issuer ?? this.extractIssuerFromAssetString(catalogAsset.assetString)),
            assetClass: ASSET_CLASS_LABELS[Number(liveRecord.asset_class)] ?? String(liveRecord.asset_class),
            status: paused
              ? "Paused"
              : ASSET_STATUS_LABELS[Number(liveRecord.status)] ?? String(liveRecord.status),
            settlementEnabled: Boolean(liveRecord.settlement_enabled),
            corporateActionsEnabled: Boolean(liveRecord.corporate_actions_enabled),
            requiresRegisteredWallets: Boolean(liveRecord.requires_registered_wallets),
            requiresIssuerAuth: Boolean(liveRecord.requires_issuer_auth),
            clawbackEnabled: Boolean(liveRecord.clawback_enabled),
            issuerPolicyHash: this.normalizeHash(liveRecord.issuer_policy_hash),
            transferClassHash: this.normalizeHash(liveRecord.transfer_class_hash),
            jurisdictionPolicyHash: this.normalizeHash(liveRecord.jurisdiction_policy_hash),
          } satisfies AssetSummary;
        }),
      );

      return results.sort((left, right) => left.displayName.localeCompare(right.displayName));
    });
  }

  async getProofScenarios(): Promise<ProofScenario[]> {
    return this.cached("proofs", async () => {
      const participantDirectory = await this.getParticipantDirectory();
      const events = await this.fetchContractEvents(
        [this.protocol.contracts.proofGateway.contractId],
        Number(this.protocol.config?.proofGateway?.validFromLedger ?? 0),
        200,
      );

      const revokedByReceipt = new Map<string, DecodedEvent>();
      const latestRecordedByScenario = new Map<string, DecodedEvent>();
      for (const event of events) {
        if (event.topic === "receipt_revoked") {
          revokedByReceipt.set(String(event.data.receipt_id), event);
          continue;
        }
        if (event.topic !== "proof_recorded") {
          continue;
        }
        const scenarioKey = `${event.data.participant_id_hash}:${event.data.proof_type}`;
        latestRecordedByScenario.set(scenarioKey, event);
      }

      const verifierById = this.getVerifierMetadataById();

      const scenarios = await Promise.all(
        [...latestRecordedByScenario.values()].map(async (event) => {
          const receiptId = String(event.data.receipt_id);
          const proofType = Number(event.data.proof_type);
          const participant = participantDirectory.byId.get(String(event.data.participant_id_hash));
          const verifier = verifierById.get(String(event.data.verifier_id));
          const revokedEvent = revokedByReceipt.get(receiptId);

          const status: ProofScenario["status"] = revokedEvent
            ? "revoked"
            : proofType === 2 || proofType === 3
              ? "usable"
              : "verified";

          const baseKey = participant?.key ?? this.humanizeKey(String(event.data.participant_id_hash).slice(0, 8)).toLowerCase();
          return {
            key: `${baseKey}-${PROOF_TYPE_KEYS[proofType] ?? `proof-${proofType}`}`,
            proofType: PROOF_TYPE_LABELS[proofType] ?? `Proof ${proofType}`,
            participantKey: participant?.key ?? baseKey,
            participantName: participant?.displayName ?? this.humanizeKey(baseKey),
            verifierContractId: verifier?.contractId ?? null,
            verifierId: String(event.data.verifier_id ?? ""),
            receiptId,
            status,
            source: "live-chain",
            notes: revokedEvent
              ? `Receipt was revoked on ledger ${revokedEvent.ledger}.`
              : `Receipt was recorded on ledger ${event.ledger}.`,
          } satisfies ProofScenario;
        }),
      );

      return scenarios.sort((left, right) => left.participantName.localeCompare(right.participantName) || left.proofType.localeCompare(right.proofType));
    });
  }

  async getSettlementSummaries(): Promise<SettlementSummary[]> {
    return this.cached("settlements", async () => {
      const participantDirectory = await this.getParticipantDirectory();
      const assetBySacId = this.getAssetCatalogBySacId();
      const events = await this.fetchContractEvents(
        [this.protocol.contracts.settlementNettingEngine.contractId],
        this.protocolStartLedger,
        100,
      );

      const batchTransfersBySettlementId = new Map<string, DecodedEvent>();
      const settlements: SettlementSummary[] = [];

      for (const event of events) {
        if (event.topic === "batch_transfers_applied") {
          batchTransfersBySettlementId.set(String(event.data.settlement_id), event);
        }
      }

      for (const event of events) {
        if (event.topic === "execution_settled_dvp") {
          const record = await this.viewContract(
            this.protocol.contracts.settlementNettingEngine.contractId,
            "get_execution_settlement",
            ["--execution_id", String(event.data.execution_id)],
          );
          settlements.push({
            kind: "direct",
            settlementId: String(record.settlement_id),
            batchId: null,
            settlementTxHash: event.txHash,
            transferTxHash: event.txHash,
            executionIds: [String(record.execution_id)],
            participants: [
              participantDirectory.byWallet.get(String(record.buyer))?.displayName ?? this.truncateAddress(String(record.buyer)),
              participantDirectory.byWallet.get(String(record.seller))?.displayName ?? this.truncateAddress(String(record.seller)),
            ],
            assetSymbols: [
              assetBySacId.get(String(record.instrument_asset))?.symbol ?? this.truncateAddress(String(record.instrument_asset)),
              assetBySacId.get(String(record.cash_asset))?.symbol ?? this.truncateAddress(String(record.cash_asset)),
            ],
            completedAt: event.ledgerClosedAt,
          });
        }

        if (event.topic === "batch_settled") {
          const settlementId = String(event.data.settlement_id);
          const batch = await this.viewContract(
            this.protocol.contracts.settlementNettingEngine.contractId,
            "get_batch",
            ["--settlement_id", settlementId],
          );
          const transferEvent = batchTransfersBySettlementId.get(settlementId);
          settlements.push({
            kind: "batch",
            settlementId,
            batchId: String(batch.batch_id),
            settlementTxHash: event.txHash,
            transferTxHash: transferEvent?.txHash ?? null,
            executionIds: [String(batch.execution_a_id), String(batch.execution_b_id)].filter(Boolean),
            participants: await this.resolveBatchParticipants(settlementId, participantDirectory.byWallet),
            assetSymbols: [
              assetBySacId.get(String(this.phase0.assets.demo?.[0]?.sacContractId ?? ""))?.symbol ?? "Instrument",
              assetBySacId.get(String(this.phase0.assets.usdc?.sacContractId ?? ""))?.symbol ?? "Cash",
            ],
            completedAt: transferEvent?.ledgerClosedAt ?? event.ledgerClosedAt,
          });
        }
      }

      return settlements.sort((left, right) => Date.parse(right.completedAt ?? "") - Date.parse(left.completedAt ?? ""));
    });
  }

  async getOrderFlowSummary(): Promise<OrderFlowSummary | null> {
    return this.cached("orders", async () => {
      const participants = await this.getParticipants();
      const participantById = new Map(participants.map((participant) => [participant.participantIdHash, participant]));
      const events = await this.fetchContractEvents(
        [this.protocol.contracts.orderCommitPool.contractId],
        this.protocolStartLedger,
        100,
      );

      const latestCancel = [...events].reverse().find((event) => event.topic === "order_cancelled");
      const latestMatch = [...events].reverse().find((event) => event.topic === "private_match_recorded");
      if (!latestMatch) {
        return null;
      }

      const execution = await this.viewContract(
        this.protocol.contracts.orderCommitPool.contractId,
        "get_execution",
        ["--execution_id", String(latestMatch.data.execution_id)],
      );
      const bidOrder = await this.viewContract(
        this.protocol.contracts.orderCommitPool.contractId,
        "get_order",
        ["--order_id", String(execution.bid_order_id)],
      );
      const askOrder = await this.viewContract(
        this.protocol.contracts.orderCommitPool.contractId,
        "get_order",
        ["--order_id", String(execution.ask_order_id)],
      );

      const commitByOrderId = new Map<string, DecodedEvent>();
      for (const event of events) {
        if (event.topic === "order_committed") {
          commitByOrderId.set(String(event.data.order_id), event);
        }
      }

      return {
        cancelledOrderId: latestCancel ? String(latestCancel.data.order_id) : null,
        executionId: String(execution.execution_id),
        batchId: String(execution.batch_id),
        bidParticipant: participantById.get(String(bidOrder.participant_id_hash))?.displayName ?? "Unknown",
        askParticipant: participantById.get(String(askOrder.participant_id_hash))?.displayName ?? "Unknown",
        commitCancelledOrderTxHash: latestCancel ? commitByOrderId.get(String(latestCancel.data.order_id))?.txHash ?? null : null,
        cancelOrderTxHash: latestCancel?.txHash ?? null,
        commitBidTxHash: commitByOrderId.get(String(execution.bid_order_id))?.txHash ?? null,
        commitAskTxHash: commitByOrderId.get(String(execution.ask_order_id))?.txHash ?? null,
        matchOrdersTxHash: latestMatch.txHash,
        expectedFailure: null,
        completedAt: latestMatch.ledgerClosedAt,
      };
    });
  }

  async getComplianceSnapshot(): Promise<ComplianceSnapshot> {
    return this.cached("compliance", async () => {
      const [participants, assets, events, globalPause, operatorModel] = await Promise.all([
        this.getParticipants(),
        this.getAssets(),
        this.fetchContractEvents([this.phase0.contracts.complianceControl.contractId], this.protocolStartLedger, 100),
        this.viewContract(this.phase0.contracts.complianceControl.contractId, "is_globally_paused", []) as Promise<boolean>,
        this.getOperatorModel(),
      ]);

      const latestIncident = await this.buildLatestComplianceIncident(events, participants, assets);

      return {
        protocolLive: true,
        globalPause: Boolean(globalPause),
        participants: participants.map((participant) => ({
          key: participant.key,
          displayName: participant.displayName,
          frozen: participant.currentFrozen,
          participantStatus: participant.participantStatus,
          kycStatus: participant.kycStatus,
          sanctionsStatus: participant.sanctionsStatus,
        })),
        assets: await Promise.all(assets.map(async (asset) => ({
          key: asset.key,
          displayName: asset.displayName,
          paused: Boolean(await this.viewContract(
            this.phase0.contracts.complianceControl.contractId,
            "is_asset_paused",
            ["--asset", asset.sacContractId],
          )),
          settlementEnabled: asset.settlementEnabled,
          corporateActionsEnabled: asset.corporateActionsEnabled,
        }))),
        recentIncident: latestIncident,
        operatorModel,
      };
    });
  }

  async getAuditCase(caseId: string): Promise<AuditCaseView | null> {
    const normalizedCaseId = this.normalizeHash(caseId);
    const participantDirectory = await this.getParticipantDirectory();
    const auditEvents = await this.fetchContractEvents(
      [this.protocol.contracts.auditDisclosureRegistry.contractId],
      this.protocolStartLedger,
      100,
    );
    const complianceEvents = await this.fetchContractEvents(
      [this.phase0.contracts.complianceControl.contractId],
      this.protocolStartLedger,
      100,
    );

    const grantEvents = auditEvents.filter((event) => event.topic === "grant_set");
    const accessEvents = auditEvents.filter((event) => event.topic === "access_recorded");
    const blobEvents = auditEvents.filter((event) => event.topic === "blob_registered");

    const grants = await Promise.all(grantEvents.map(async (event) => ({
      event,
      record: await this.viewContract(
        this.protocol.contracts.auditDisclosureRegistry.contractId,
        "get_grant",
        ["--grant_id", String(event.data.grant_id)],
      ),
    })));

    const accesses = await Promise.all(accessEvents.map(async (event) => ({
      event,
      record: await this.viewContract(
        this.protocol.contracts.auditDisclosureRegistry.contractId,
        "get_access_receipt",
        ["--receipt_id", String(event.data.receipt_id)],
      ),
    })));

    const selectedAccess = normalizedCaseId
      ? accesses.find(({ record }) => this.normalizeHash(record.case_id) === normalizedCaseId)
      : accesses.at(-1);
    const selectedGrant = normalizedCaseId
      ? grants.find(({ record }) => this.normalizeHash(record.case_id) === normalizedCaseId)
      : grants.at(-1);

    const effectiveCaseId = normalizedCaseId
      ?? this.normalizeHash(selectedAccess?.record.case_id)
      ?? this.normalizeHash(selectedGrant?.record.case_id);

    if (!effectiveCaseId) {
      return null;
    }

    const matchingAccess = selectedAccess
      ?? accesses.find(({ record }) => this.normalizeHash(record.case_id) === effectiveCaseId)
      ?? null;
    const matchingGrant = selectedGrant
      ?? grants.find(({ record }) => this.normalizeHash(record.case_id) === effectiveCaseId)
      ?? null;
    const matchingFreeze = [...complianceEvents].reverse().find((event) =>
      event.topic === "participant_freeze_set" && this.normalizeHash(event.data.case_id) === effectiveCaseId,
    ) ?? null;
    const matchingParticipant = matchingFreeze
      ? participantDirectory.byId.get(String(matchingFreeze.data.participant_id_hash)) ?? null
      : null;
    const matchingBlobHash = this.normalizeHash(matchingAccess?.record.blob_hash)
      ?? this.normalizeHash(blobEvents.at(-1)?.data.blob_hash);

    return {
      caseId: effectiveCaseId,
      summary: matchingFreeze
        ? "Compliance-triggered freeze with scoped disclosure access."
        : "Scoped disclosure access recorded on-chain.",
      participantKey: matchingParticipant?.key ?? null,
      participantName: matchingParticipant?.displayName ?? null,
      linkedSettlementId: null,
      linkedTxHashes: [
        matchingGrant?.event.txHash,
        matchingAccess?.event.txHash,
        matchingFreeze?.txHash,
      ].filter((value): value is string => Boolean(value)),
      disclosureGrantTxHash: matchingGrant?.event.txHash ?? null,
      disclosureAccessTxHash: matchingAccess?.event.txHash ?? null,
      blobHash: matchingBlobHash,
      notes: [
        matchingGrant ? `Grant active through ledger ${matchingGrant.record.expiry_ledger}.` : "No live disclosure grant found for this case.",
        matchingAccess ? `Access was recorded by ${this.truncateAddress(String(matchingAccess.record.accessor))}.` : "No live disclosure access receipt found for this case.",
        matchingFreeze ? "This case is also linked to a participant freeze on-chain." : "This case is disclosure-only in the current on-chain record.",
      ],
    };
  }

  async getLatestDisclosureAction(): Promise<AuditAction | null> {
    const auditEvents = await this.fetchContractEvents(
      [this.protocol.contracts.auditDisclosureRegistry.contractId],
      this.protocolStartLedger,
      100,
    );
    const latestAccess = [...auditEvents].reverse().find((event) => event.topic === "access_recorded");
    if (!latestAccess) {
      return null;
    }

    const receipt = await this.viewContract(
      this.protocol.contracts.auditDisclosureRegistry.contractId,
      "get_access_receipt",
      ["--receipt_id", String(latestAccess.data.receipt_id)],
    );

    return {
      title: "Scoped audit disclosure recorded",
      caseId: this.normalizeHash(receipt.case_id),
      blobHash: this.normalizeHash(receipt.blob_hash),
      txHash: latestAccess.txHash,
      happenedAt: latestAccess.ledgerClosedAt,
    };
  }

  private async buildLatestComplianceIncident(
    events: DecodedEvent[],
    participants: ParticipantSummary[],
    assets: AssetSummary[],
  ): Promise<ComplianceIncident | null> {
    const participantById = new Map(participants.map((participant) => [participant.participantIdHash, participant]));
    const assetById = new Map(assets.map((asset) => [asset.sacContractId, asset]));
    const latest = [...events].reverse().find((event) =>
      ["participant_freeze_set", "asset_pause_set", "global_pause_set"].includes(event.topic),
    );
    if (!latest) {
      return null;
    }

    if (latest.topic === "participant_freeze_set") {
      const participant = participantById.get(String(latest.data.participant_id_hash));
      const frozen = Boolean(latest.data.frozen);
      return {
        title: frozen ? "Participant frozen" : "Participant unfrozen",
        caseId: this.normalizeHash(latest.data.case_id),
        participantKey: participant?.key ?? null,
        participantName: participant?.displayName ?? null,
        outcome: frozen ? "Trading and downstream settlement are blocked." : "Trading eligibility can resume once other checks pass.",
        reportedAt: latest.ledgerClosedAt,
      };
    }

    if (latest.topic === "asset_pause_set") {
      const asset = assetById.get(String(latest.data.asset));
      return {
        title: Boolean(latest.data.paused) ? "Asset paused" : "Asset unpaused",
        caseId: this.normalizeHash(latest.data.case_id),
        participantKey: null,
        participantName: asset?.displayName ?? null,
        outcome: "Asset-level transfer controls changed on-chain.",
        reportedAt: latest.ledgerClosedAt,
      };
    }

    return {
      title: Boolean(latest.data.paused) ? "Global pause enabled" : "Global pause disabled",
      caseId: this.normalizeHash(latest.data.case_id),
      participantKey: null,
      participantName: null,
      outcome: "Protocol-wide trading gates were updated on-chain.",
      reportedAt: latest.ledgerClosedAt,
    };
  }

  private async resolveBatchParticipants(
    settlementId: string,
    participantByWallet: Map<string, ParticipantMeta>,
  ): Promise<string[]> {
    try {
      const transfer = await this.viewContract(
        this.protocol.contracts.settlementNettingEngine.contractId,
        "get_batch_transfer",
        ["--settlement_id", settlementId],
      );
      const wallets = [
        String(transfer.execution_a_buyer),
        String(transfer.execution_a_seller),
        String(transfer.execution_b_buyer),
        String(transfer.execution_b_seller),
      ];
      return [...new Set(wallets.map((wallet) => participantByWallet.get(wallet)?.displayName ?? this.truncateAddress(wallet)))];
    } catch {
      return [];
    }
  }

  private async getParticipantDirectory(): Promise<{
    byId: Map<string, ParticipantMeta>;
    byWallet: Map<string, ParticipantMeta>;
  }> {
    return this.cached("participant-directory", async () => {
      const byId = new Map<string, ParticipantMeta>();
      const byWallet = new Map<string, ParticipantMeta>();

      for (const participant of this.phase0Participants) {
        const key = String(participant.key);
        const address = String(participant.wallet?.address ?? "");
        const meta: ParticipantMeta = {
          key,
          displayName: PARTICIPANT_DISPLAY_NAMES[key] ?? this.humanizeKey(key),
          walletName: String(participant.wallet?.name ?? ""),
          address,
          participantIdHash: String(participant.participantIdHash),
        };
        byId.set(String(participant.participantIdHash), meta);
        if (address) {
          byWallet.set(address, meta);
        }
      }

      const localIdentityMeta = await this.getLocalIdentityMeta();
      for (const meta of localIdentityMeta) {
        if (meta.address) {
          byWallet.set(meta.address, byWallet.get(meta.address) ?? meta);
        }
      }

      const missingIdentityMeta = localIdentityMeta.filter((meta) => meta.address && !meta.participantIdHash);
      const ownerPairs = await Promise.all(missingIdentityMeta.map(async (meta) => {
        try {
          const participantIdHash = await this.viewContract(
            this.phase0.contracts.participantRegistry.contractId,
            "wallet_owner",
            ["--wallet", String(meta.address)],
          ) as string;
          return [participantIdHash, meta] as const;
        } catch {
          return null;
        }
      }));

      for (const entry of ownerPairs) {
        if (!entry) {
          continue;
        }
        const [participantIdHash, meta] = entry;
        if (!byId.has(participantIdHash)) {
          byId.set(participantIdHash, {
            ...meta,
            participantIdHash,
          });
        }
      }

      return { byId, byWallet };
    });
  }

  private getAssetCatalogBySacId(): Map<string, { key: string; symbol: string; displayName: string }> {
    const catalogs = [
      ...(Array.isArray(this.phase0.assets?.demo) ? this.phase0.assets.demo : []),
      this.phase0.assets?.usdc,
    ].filter(Boolean);

    return new Map(
      catalogs.map((asset: any) => [
        String(asset.sacContractId),
        {
          key: String(asset.key ?? asset.displaySymbol?.toLowerCase() ?? asset.sacContractId),
          symbol: String(asset.displaySymbol ?? asset.onChainCode ?? asset.sacContractId),
          displayName: String(asset.displayName ?? asset.displaySymbol ?? asset.sacContractId),
        },
      ]),
    );
  }

  private async getOperatorModel(): Promise<ComplianceSnapshot["operatorModel"]> {
    const complianceAccount = await this.fetchHorizonAccount(String(this.phase0.wallets?.compliance?.address ?? ""));
    const signerCount = Array.isArray(complianceAccount.signers) ? complianceAccount.signers.length : 0;
    const highThreshold = Number(complianceAccount.thresholds?.high_threshold ?? 0);

    if (signerCount > 1 || highThreshold > 1) {
      return {
        type: "multisig",
        details: `Compliance operator currently exposes ${signerCount} signers with high threshold ${highThreshold}.`,
      };
    }

    if (signerCount === 1) {
      return {
        type: "single-signer",
        details: "Current deployed demo operator account is a single-signer Stellar account.",
      };
    }

    return {
      type: "unknown",
      details: "Unable to infer operator signer policy from Horizon.",
    };
  }

  private async getLatestLedgerSequence(): Promise<number> {
    const response = await this.fetchRpc("getLatestLedger", {});
    return Number(response.sequence);
  }

  private async fetchContractEvents(
    contractIds: string[],
    startLedger: number,
    limit: number,
  ): Promise<DecodedEvent[]> {
    const cacheKey = `events:${contractIds.join(",")}:${startLedger}:${limit}`;
    return this.cached(cacheKey, async () => {
      const response = await this.fetchRpc("getEvents", {
        startLedger,
        filters: [{ type: "contract", contractIds }],
        pagination: { limit },
      });

      const events: RpcEventRecord[] = Array.isArray(response.events) ? response.events : [];
      return Promise.all(events.map(async (event) => ({
        contractId: event.contractId,
        txHash: event.txHash,
        ledger: Number(event.ledger),
        ledgerClosedAt: String(event.ledgerClosedAt),
        topic: await this.decodeTopic(String(event.topic?.[0] ?? "")),
        data: await this.decodeEventValue(String(event.value)),
      })));
    });
  }

  private async decodeTopic(base64Value: string): Promise<string> {
    if (!base64Value) {
      return "";
    }
    const decoded = await this.decodeScVal(base64Value);
    return typeof decoded.symbol === "string" ? decoded.symbol : JSON.stringify(decoded);
  }

  private async decodeEventValue(base64Value: string): Promise<Record<string, any>> {
    const decoded = await this.decodeScVal(base64Value);
    return this.toNativeScVal(decoded) as Record<string, any>;
  }

  private async decodeScVal(base64Value: string): Promise<any> {
    return this.cached(`xdr:${base64Value}`, async () => {
      const { stdout } = await execFileAsync(
        "stellar",
        [
          "xdr",
          "decode",
          "--type",
          "ScVal",
          "--input",
          "single-base64",
          "--output",
          "json",
          base64Value,
        ],
        { encoding: "utf8" },
      );
      return JSON.parse(stdout);
    }, 60_000);
  }

  private toNativeScVal(decoded: any): any {
    if (decoded === null || decoded === undefined) {
      return decoded;
    }
    if (typeof decoded !== "object") {
      return decoded;
    }
    if ("map" in decoded && Array.isArray(decoded.map)) {
      return Object.fromEntries(
        decoded.map.map((entry: any) => [this.toNativeScVal(entry.key), this.toNativeScVal(entry.val)]),
      );
    }
    if ("vec" in decoded && Array.isArray(decoded.vec)) {
      return decoded.vec.map((value: any) => this.toNativeScVal(value));
    }
    if ("symbol" in decoded) return decoded.symbol;
    if ("string" in decoded) return decoded.string;
    if ("bool" in decoded) return decoded.bool;
    if ("u32" in decoded) return decoded.u32;
    if ("u64" in decoded) return decoded.u64;
    if ("i128" in decoded) return decoded.i128;
    if ("bytes" in decoded) return decoded.bytes;
    if ("address" in decoded) return decoded.address;
    return decoded;
  }

  private async viewContract(contractId: string, functionName: string, contractArgs: string[]): Promise<any> {
    return this.cached(`view:${contractId}:${functionName}:${contractArgs.join("|")}`, async () => {
      const { stdout } = await execFileAsync(
        "stellar",
        [
          "--no-cache",
          "contract",
          "invoke",
          "--id",
          contractId,
          "--source",
          await this.getReadSourceName(),
          "--network",
          this.network,
          "--send",
          "no",
          "--",
          functionName,
          ...contractArgs,
        ],
        { encoding: "utf8" },
      );
      return this.parseJsonish(this.extractContractOutput(stdout));
    });
  }

  private extractContractOutput(stdout: string): string {
    const lines = stdout
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean)
      .filter((line) => !line.startsWith("ℹ"));
    return lines.at(-1) ?? "";
  }

  private async fetchRpc(method: string, params: Record<string, any>): Promise<any> {
    const response = await fetch(this.rpcUrl, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: 1,
        method,
        params,
      }),
    });
    if (!response.ok) {
      throw new Error(`RPC request failed: ${method} ${response.status}`);
    }
    const payload = await response.json();
    if (payload.error) {
      throw new Error(`RPC error for ${method}: ${JSON.stringify(payload.error)}`);
    }
    return payload.result;
  }

  private async fetchHorizonAccount(address: string): Promise<any> {
    const response = await fetch(`${this.horizonUrl}/accounts/${address}`);
    if (!response.ok) {
      throw new Error(`Horizon request failed for account ${address}: ${response.status}`);
    }
    return response.json();
  }

  private async getLocalIdentityMeta(): Promise<ParticipantMeta[]> {
    return this.cached("local-identity-meta", async () => {
      const { stdout } = await execFileAsync("stellar", ["keys", "ls"], { encoding: "utf8" });
      const names = stdout
        .split("\n")
        .map((line) => line.trim())
        .filter(Boolean)
        .filter((name) => name.startsWith(`zkdtcc-${this.namespace}-`));

      const identities = await Promise.all(names.map(async (name) => {
        const { stdout: addressStdout } = await execFileAsync("stellar", ["keys", "address", name], { encoding: "utf8" });
        const key = name.replace(`zkdtcc-${this.namespace}-`, "");
        return {
          key,
          displayName: PARTICIPANT_DISPLAY_NAMES[key] ?? this.humanizeKey(key),
          walletName: name,
          address: addressStdout.trim(),
        } satisfies ParticipantMeta;
      }));

      return identities;
    }, 60_000);
  }

  private async getReadSourceName(): Promise<string> {
    return this.cached("read-source", async () => {
      const requested = process.env.STELLAR_VIEW_SOURCE?.trim();
      if (requested) {
        return requested;
      }

      const { stdout } = await execFileAsync("stellar", ["keys", "ls"], { encoding: "utf8" });
      const keys = stdout.split("\n").map((line) => line.trim()).filter(Boolean);
      const preferred = `zkdtcc-${this.namespace}-probe`;
      if (keys.includes(preferred)) {
        return preferred;
      }
      const deployer = String(this.phase0.wallets?.deployer?.name ?? this.phase0.admin?.name ?? "rosca-admin");
      if (keys.includes(deployer)) {
        return deployer;
      }
      throw new Error("No local Stellar source account available for read-only contract simulation.");
    }, 60_000);
  }

  private getVerifierMetadataById(): Map<string, { contractId: string; key: string }> {
    const map = new Map<string, { contractId: string; key: string }>();
    const configVerifiers = this.protocol.config?.proofGateway?.verifiers ?? {};
    const deployedVerifiers = this.protocol.verifiers ?? {};
    for (const [key, config] of Object.entries(configVerifiers as Record<string, { verifierId?: string }>)) {
      const contractId = deployedVerifiers[key]?.contractId;
      if (config?.verifierId && contractId) {
        map.set(String(config.verifierId), {
          key,
          contractId: String(contractId),
        });
      }
    }
    return map;
  }

  private normalizeHash(value: unknown): string | null {
    if (typeof value !== "string") {
      return null;
    }
    return value === ZERO_HASH ? null : value;
  }

  private normalizeLedger(value: unknown): number | null {
    const numeric = Number(value ?? 0);
    if (!Number.isFinite(numeric) || numeric <= 0 || numeric === U32_MAX) {
      return null;
    }
    return numeric;
  }

  private extractIssuerFromAssetString(assetString: string): string {
    const [, issuer = ""] = String(assetString).split(":");
    return issuer;
  }

  private parseJsonish(value: string): any {
    try {
      return JSON.parse(value);
    } catch {
      return value;
    }
  }

  private readJson(filePath: string): JsonRecord {
    return JSON.parse(readFileSync(filePath, "utf8")) as JsonRecord;
  }

  private humanizeKey(value: string): string {
    return value
      .replace(/[-_]+/g, " ")
      .replace(/\b\w/g, (char) => char.toUpperCase());
  }

  private inferParticipantKey(primaryWallet: string, participantIdHash: string): string {
    const walletMeta = (Object.values(this.phase0.wallets ?? {}) as Array<{ address?: string; name?: string }>)
      .find((wallet) => wallet?.address === primaryWallet);
    if (walletMeta?.name) {
      return String(walletMeta.name).replace(`zkdtcc-${this.namespace}-`, "");
    }
    return `participant-${participantIdHash.slice(0, 8)}`;
  }

  private truncateAddress(value: string): string {
    return value.length > 12 ? `${value.slice(0, 6)}...${value.slice(-4)}` : value;
  }

  private cached<T>(key: string, loader: () => Promise<T>, ttlMs: number = CACHE_TTL_MS): Promise<T> {
    const now = Date.now();
    const existing = this.cache.get(key);
    if (existing && existing.expiresAt > now) {
      return existing.value as Promise<T>;
    }

    const value = loader().catch((error) => {
      this.cache.delete(key);
      throw error;
    });
    this.cache.set(key, {
      expiresAt: now + ttlMs,
      value,
    });
    return value;
  }

  private get namespace(): string {
    return String(this.phase0.namespace ?? "demo");
  }

  private get network(): string {
    return String(this.protocol.network ?? this.phase0.network ?? "testnet");
  }

  private get rpcUrl(): string {
    if (this.network === "testnet") {
      return "https://soroban-testnet.stellar.org";
    }
    throw new Error(`Unsupported network for RPC: ${this.network}`);
  }

  private get horizonUrl(): string {
    if (this.network === "testnet") {
      return "https://horizon-testnet.stellar.org";
    }
    throw new Error(`Unsupported network for Horizon: ${this.network}`);
  }

  private get protocolStartLedger(): number {
    return Number(this.protocol.config?.proofGateway?.validFromLedger ?? 0);
  }

  private get phase0Participants(): JsonRecord[] {
    return Array.isArray(this.phase0.participants) ? this.phase0.participants : [];
  }

  private get phase0ParticipantsByKey(): Record<string, JsonRecord> {
    return Object.fromEntries(this.phase0Participants.map((participant) => [String(participant.key), participant]));
  }
}

export async function prewarmLiveDemoData(service: LiveDemoDataService): Promise<void> {
  await Promise.allSettled([
    service.getParticipants(),
    service.getAssets(),
    service.getProofScenarios(),
    service.getSettlementSummaries(),
    service.getComplianceSnapshot(),
    service.getOverview(),
  ]);
}
