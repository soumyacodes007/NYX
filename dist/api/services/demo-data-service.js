import { existsSync, readdirSync, readFileSync } from "node:fs";
import path from "node:path";
const DEFAULT_PHASE0_PATH = path.resolve(process.cwd(), "deployments", "testnet-phase0-demo0628b.json");
const DEFAULT_PROTOCOL_PATH = path.resolve(process.cwd(), "deployments", "testnet-protocol-demo0628b.json");
const REPORTS_DIR = path.resolve(process.cwd(), "deployments", "reports");
const PARTICIPANT_DISPLAY_NAMES = {
    treasury: "Treasury",
    alpha: "Alpha",
    beta: "Beta",
    gamma: "Gamma",
    matcher: "Matcher",
    settler: "Settler",
    compliance: "Compliance",
    issuer: "Issuer",
};
const PARTICIPANT_ROLE_LABELS = {
    1: "Institution Trader",
    2: "Compliance Operator",
    3: "Matcher",
    4: "Settlement Operator",
    5: "Issuer / Treasury Admin",
    6: "Auditor / Regulator",
};
const PROOF_TYPE_ORDER = [
    "collateralSufficiency",
    "unencumberedLot",
    "privateMatch",
    "batchNetting",
    "entitlementClaim",
];
export class DemoDataService {
    phase0;
    protocol;
    constructor(phase0Path = DEFAULT_PHASE0_PATH, protocolPath = DEFAULT_PROTOCOL_PATH) {
        this.phase0 = this.readJson(phase0Path);
        this.protocol = this.readJson(protocolPath);
    }
    getOverview() {
        const participants = this.getParticipants();
        const assets = this.getAssets();
        const step9 = this.getLatestReport("step9-onchain-");
        const step10 = this.getLatestReport("step10-onchain-");
        const step12 = this.getLatestReport("step12-onchain-");
        const latestSettlement = this.getSettlementSummaries()[0] ?? null;
        const latestIncident = this.buildLatestComplianceIncident(step12);
        const latestDisclosureAction = this.buildLatestDisclosureAction(step12);
        const phase0Checks = this.getLatestReport("phase0-onchain-checks-");
        const latestLedgerSequence = phase0Checks?.step1?.latestLedger?.sequence ?? null;
        return {
            protocolName: "zk-DTCC Demo API",
            environment: this.phase0.namespace ?? "demo",
            network: this.protocol.network ?? this.phase0.network ?? "testnet",
            protocolLive: true,
            globalPause: false,
            latestLedgerSequence,
            participantCount: participants.length,
            supportedAssetCount: assets.length,
            recentProofCount: this.getProofScenarios().length,
            recentSettlementCount: [step9, step10].filter(Boolean).length,
            recentComplianceActionCount: step12 ? 1 : 0,
            institutions: participants
                .filter((participant) => ["alpha", "beta", "gamma", "treasury"].includes(participant.key))
                .map((participant) => participant.displayName),
            supportedAssets: assets.map((asset) => asset.displayName),
            latestSettlement,
            latestComplianceIncident: latestIncident,
            latestDisclosureAction,
        };
    }
    getParticipants() {
        const phase0Participants = Array.isArray(this.phase0.participants) ? this.phase0.participants : [];
        return phase0Participants.map((participant) => ({
            key: participant.key,
            displayName: PARTICIPANT_DISPLAY_NAMES[participant.key] ?? participant.key,
            role: PARTICIPANT_ROLE_LABELS[participant.role] ?? `Role ${participant.role}`,
            participantIdHash: participant.participantIdHash,
            primaryWallet: participant.wallet?.address ?? "",
            walletCount: 1,
            legalEntityHash: participant.legalEntityHash,
            jurisdictionHash: participant.jurisdictionHash,
            credentialRoot: participant.credentialRoot,
            participantStatus: "Active",
            kycStatus: "Approved",
            sanctionsStatus: "Clear",
            credentialExpiryLedger: null,
            reviewCaseId: null,
            createdLedger: null,
            updatedLedger: null,
            currentFrozen: false,
        }));
    }
    getParticipant(participantKey) {
        return this.getParticipants().find((participant) => participant.key === participantKey) ?? null;
    }
    getAssets() {
        const demoAssets = Array.isArray(this.phase0.assets?.demo) ? this.phase0.assets.demo : [];
        const usdcAsset = this.phase0.assets?.usdc;
        const mappedDemoAssets = demoAssets.map((asset) => ({
            key: asset.key,
            displayName: asset.displaySymbol,
            symbol: asset.displaySymbol,
            assetString: asset.assetString,
            sacContractId: asset.sacContractId,
            issuer: asset.issuer ?? this.extractIssuerFromAssetString(asset.assetString),
            assetClass: "DTC Entitlement",
            status: "Active",
            settlementEnabled: true,
            corporateActionsEnabled: true,
            requiresRegisteredWallets: true,
            requiresIssuerAuth: true,
            clawbackEnabled: false,
            issuerPolicyHash: null,
            transferClassHash: null,
            jurisdictionPolicyHash: null,
        }));
        const mappedUsdc = usdcAsset
            ? [{
                    key: usdcAsset.key ?? "usdc",
                    displayName: "USDC",
                    symbol: "USDC",
                    assetString: usdcAsset.assetString,
                    sacContractId: usdcAsset.sacContractId,
                    issuer: this.extractIssuerFromAssetString(usdcAsset.assetString),
                    assetClass: "Settlement Cash",
                    status: "Active",
                    settlementEnabled: true,
                    corporateActionsEnabled: false,
                    requiresRegisteredWallets: true,
                    requiresIssuerAuth: true,
                    clawbackEnabled: false,
                    issuerPolicyHash: null,
                    transferClassHash: null,
                    jurisdictionPolicyHash: null,
                }]
            : [];
        return [...mappedDemoAssets, ...mappedUsdc];
    }
    getProofScenarios() {
        const verifiers = this.protocol.verifiers ?? {};
        const verifierLookup = {
            collateralSufficiency: verifiers.collateralSufficiency ?? null,
            unencumberedLot: verifiers.unencumberedLot ?? null,
            privateMatch: verifiers.privateMatch ?? null,
            batchNetting: verifiers.batchNetting ?? null,
            entitlementClaim: verifiers.entitlementClaim ?? null,
        };
        const scenarios = [
            this.buildProofScenario("alpha-collateral", "Collateral Sufficiency", "alpha", "collateralSufficiency", "usable"),
            this.buildProofScenario("alpha-unencumbered", "Unencumbered Lot", "alpha", "unencumberedLot", "usable"),
            this.buildProofScenario("beta-collateral", "Collateral Sufficiency", "beta", "collateralSufficiency", "usable"),
            this.buildProofScenario("beta-unencumbered", "Unencumbered Lot", "beta", "unencumberedLot", "usable"),
            this.buildProofScenario("gamma-collateral", "Collateral Sufficiency", "gamma", "collateralSufficiency", "prepared"),
            this.buildProofScenario("gamma-unencumbered", "Unencumbered Lot", "gamma", "unencumberedLot", "prepared"),
            this.buildProofScenario("alpha-beta-private-match", "Private Match", "matcher", "privateMatch", "verified"),
            this.buildProofScenario("batch-netting-demo", "Batch Netting", "settler", "batchNetting", "verified"),
            this.buildProofScenario("alpha-entitlement-claim", "Entitlement Claim", "alpha", "entitlementClaim", "verified"),
        ];
        return scenarios.map((scenario) => ({
            ...scenario,
            verifierContractId: verifierLookup[this.normalizeProofKey(scenario.key)]?.contractId ?? null,
            verifierId: this.protocol.config?.proofGateway?.verifiers?.[this.normalizeProofKey(scenario.key)]?.verifierId ?? null,
        }));
    }
    getSettlementSummaries() {
        const step10 = this.getLatestReport("step10-onchain-");
        const step9 = this.getLatestReport("step9-onchain-");
        const settlements = [];
        if (step10?.steps?.step10) {
            settlements.push({
                kind: "batch",
                settlementId: step10.steps.step10.settlementId ?? null,
                batchId: step10.steps.step10.batchId ?? null,
                settlementTxHash: step10.steps.step10.settlementTxHash ?? null,
                transferTxHash: step10.steps.step10.transferTxHash ?? null,
                executionIds: [
                    step10.steps.step10.executionAId,
                    step10.steps.step10.executionBId,
                ].filter(Boolean),
                participants: ["Alpha", "Beta", "Gamma"],
                assetSymbols: ["DTCUST10Y-ENT", "USDC"],
                completedAt: step10.completedAt ?? null,
            });
        }
        if (step9?.steps?.step9) {
            settlements.push({
                kind: "direct",
                settlementId: step9.steps.step9.settlementId ?? null,
                batchId: null,
                settlementTxHash: step9.steps.step9.txHash ?? null,
                transferTxHash: step9.steps.step9.txHash ?? null,
                executionIds: [step9.steps.step8?.executionId].filter(Boolean),
                participants: ["Alpha", "Beta"],
                assetSymbols: ["DTCUST10Y-ENT", "USDC"],
                completedAt: step9.completedAt ?? null,
            });
        }
        return settlements.sort((left, right) => {
            const leftTime = left.completedAt ? Date.parse(left.completedAt) : 0;
            const rightTime = right.completedAt ? Date.parse(right.completedAt) : 0;
            return rightTime - leftTime;
        });
    }
    getOrderFlowSummary() {
        const step8 = this.getLatestReport("step8-onchain-");
        if (!step8?.steps?.step8) {
            return null;
        }
        return {
            cancelledOrderId: step8.steps.step8.cancelledOrderId ?? null,
            executionId: step8.steps.step8.executionId ?? null,
            batchId: null,
            bidParticipant: "Alpha",
            askParticipant: "Beta",
            commitCancelledOrderTxHash: step8.steps.step8.txHashes?.commitCancelledOrder ?? null,
            cancelOrderTxHash: step8.steps.step8.txHashes?.cancelOrder ?? null,
            commitBidTxHash: step8.steps.step8.txHashes?.commitBid ?? null,
            commitAskTxHash: step8.steps.step8.txHashes?.commitAsk ?? null,
            matchOrdersTxHash: step8.steps.step8.txHashes?.matchOrders ?? null,
            expectedFailure: step8.steps.step8.expectedFailures?.[0]?.name ?? null,
            completedAt: step8.completedAt ?? null,
        };
    }
    getComplianceSnapshot() {
        const participants = this.getParticipants();
        const assets = this.getAssets();
        const incident = this.buildLatestComplianceIncident(this.getLatestReport("step12-onchain-"));
        return {
            protocolLive: true,
            globalPause: false,
            participants: participants.map((participant) => ({
                key: participant.key,
                displayName: participant.displayName,
                frozen: participant.currentFrozen,
                participantStatus: participant.participantStatus,
                kycStatus: participant.kycStatus,
                sanctionsStatus: participant.sanctionsStatus,
            })),
            assets: assets.map((asset) => ({
                key: asset.key,
                displayName: asset.displayName,
                paused: false,
                settlementEnabled: asset.settlementEnabled,
                corporateActionsEnabled: asset.corporateActionsEnabled,
            })),
            recentIncident: incident,
            operatorModel: {
                type: "single-signer",
                details: "Current deployed demo operator accounts are single-signer Stellar accounts.",
            },
        };
    }
    getAuditCase(caseId) {
        const step12 = this.getLatestReport("step12-onchain-");
        const step10 = this.getLatestReport("step10-onchain-");
        const step11 = this.getLatestReport("step11-onchain-");
        const normalizedCaseId = caseId || "CASE-DEMO-ALPHA-FREEZE";
        return {
            caseId: normalizedCaseId,
            summary: "Post-trade compliance freeze and scoped disclosure demo case.",
            participantKey: "alpha",
            participantName: "Alpha",
            linkedSettlementId: step10?.steps?.step10?.settlementId ?? step11?.steps?.step11?.claimId ?? null,
            linkedTxHashes: [
                step10?.steps?.step10?.settlementTxHash,
                step10?.steps?.step10?.transferTxHash,
                step11?.steps?.step11?.claim,
                step12?.steps?.step12?.txHashes?.grant,
                step12?.steps?.step12?.txHashes?.access,
            ].filter(Boolean),
            disclosureGrantTxHash: step12?.steps?.step12?.txHashes?.grant ?? null,
            disclosureAccessTxHash: step12?.steps?.step12?.txHashes?.access ?? null,
            blobHash: null,
            notes: [
                "Alpha was used as the primary regulated participant in the demo scenario.",
                "Compliance freeze is demonstrated as a downstream block, not just a status badge.",
                "Disclosure access is scoped and separately recorded.",
            ],
        };
    }
    buildProofScenario(key, proofType, participantKey, verifierKey, status) {
        return {
            key,
            proofType,
            participantKey,
            participantName: PARTICIPANT_DISPLAY_NAMES[participantKey] ?? participantKey,
            verifierContractId: null,
            verifierId: null,
            receiptId: null,
            status,
            source: "inferred",
            notes: `Demo proof scenario for ${proofType} using the ${verifierKey} verifier path.`,
        };
    }
    normalizeProofKey(scenarioKey) {
        if (scenarioKey.includes("collateral"))
            return "collateralSufficiency";
        if (scenarioKey.includes("unencumbered"))
            return "unencumberedLot";
        if (scenarioKey.includes("private-match"))
            return "privateMatch";
        if (scenarioKey.includes("batch"))
            return "batchNetting";
        if (scenarioKey.includes("claim"))
            return "entitlementClaim";
        return PROOF_TYPE_ORDER[0];
    }
    buildLatestComplianceIncident(report) {
        if (!report?.steps?.step12) {
            return null;
        }
        return {
            title: "Frozen trader blocked downstream",
            caseId: "CASE-DEMO-ALPHA-FREEZE",
            participantKey: "alpha",
            participantName: "Alpha",
            outcome: "Order commit blocked after compliance freeze.",
            reportedAt: report.completedAt ?? null,
        };
    }
    buildLatestDisclosureAction(report) {
        if (!report?.steps?.step12?.txHashes) {
            return null;
        }
        return {
            title: "Scoped audit disclosure recorded",
            caseId: "CASE-DEMO-ALPHA-FREEZE",
            blobHash: null,
            txHash: report.steps.step12.txHashes.access ?? report.steps.step12.txHashes.grant ?? null,
            happenedAt: report.completedAt ?? null,
        };
    }
    getLatestReport(prefix) {
        if (!existsSync(REPORTS_DIR)) {
            return null;
        }
        const fileName = readdirSync(REPORTS_DIR)
            .filter((entry) => entry.startsWith(prefix) && entry.endsWith(".json"))
            .sort()
            .at(-1);
        if (!fileName) {
            return null;
        }
        return this.readJson(path.join(REPORTS_DIR, fileName));
    }
    readJson(filePath) {
        return JSON.parse(readFileSync(filePath, "utf8"));
    }
    extractIssuerFromAssetString(assetString) {
        const [, issuer = ""] = String(assetString).split(":");
        return issuer;
    }
}
