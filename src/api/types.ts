export interface ApiResponse<T> {
  data: T;
  meta: {
    generatedAt: string;
    source: string;
  };
}

export interface OverviewSummary {
  protocolName: string;
  environment: string;
  network: string;
  protocolLive: boolean;
  globalPause: boolean;
  latestLedgerSequence: number | null;
  participantCount: number;
  supportedAssetCount: number;
  recentProofCount: number;
  recentSettlementCount: number;
  recentComplianceActionCount: number;
  institutions: string[];
  supportedAssets: string[];
  latestSettlement: SettlementSummary | null;
  latestComplianceIncident: ComplianceIncident | null;
  latestDisclosureAction: AuditAction | null;
}

export interface ParticipantSummary {
  key: string;
  displayName: string;
  role: string;
  participantIdHash: string;
  primaryWallet: string;
  walletCount: number;
  legalEntityHash: string;
  jurisdictionHash: string;
  credentialRoot: string;
  participantStatus: string;
  kycStatus: string;
  sanctionsStatus: string;
  credentialExpiryLedger: number | null;
  reviewCaseId: string | null;
  createdLedger: number | null;
  updatedLedger: number | null;
  currentFrozen: boolean;
}

export interface AssetSummary {
  key: string;
  displayName: string;
  symbol: string;
  assetString: string;
  sacContractId: string;
  issuer: string;
  assetClass: string;
  status: string;
  settlementEnabled: boolean;
  corporateActionsEnabled: boolean;
  requiresRegisteredWallets: boolean;
  requiresIssuerAuth: boolean;
  clawbackEnabled: boolean;
  issuerPolicyHash: string | null;
  transferClassHash: string | null;
  jurisdictionPolicyHash: string | null;
}

export interface ProofScenario {
  key: string;
  proofType: string;
  participantKey: string;
  participantName: string;
  verifierContractId: string | null;
  verifierId: string | null;
  receiptId: string | null;
  status: "prepared" | "submitted" | "verified" | "usable" | "revoked" | "expired";
  source: "precomputed" | "onchain-report" | "inferred" | "live-chain";
  notes: string;
}

export interface SettlementSummary {
  kind: "direct" | "batch";
  settlementId: string | null;
  batchId: string | null;
  settlementTxHash: string | null;
  transferTxHash: string | null;
  executionIds: string[];
  participants: string[];
  assetSymbols: string[];
  completedAt: string | null;
}

export interface OrderFlowSummary {
  cancelledOrderId: string | null;
  executionId: string | null;
  batchId: string | null;
  bidParticipant: string;
  askParticipant: string;
  commitCancelledOrderTxHash: string | null;
  cancelOrderTxHash: string | null;
  commitBidTxHash: string | null;
  commitAskTxHash: string | null;
  matchOrdersTxHash: string | null;
  expectedFailure: string | null;
  completedAt: string | null;
}

export interface ComplianceIncident {
  title: string;
  caseId: string | null;
  participantKey: string | null;
  participantName: string | null;
  outcome: string;
  reportedAt: string | null;
}

export interface ComplianceSnapshot {
  protocolLive: boolean;
  globalPause: boolean;
  participants: Array<{
    key: string;
    displayName: string;
    frozen: boolean;
    participantStatus: string;
    kycStatus: string;
    sanctionsStatus: string;
  }>;
  assets: Array<{
    key: string;
    displayName: string;
    paused: boolean;
    settlementEnabled: boolean;
    corporateActionsEnabled: boolean;
  }>;
  recentIncident: ComplianceIncident | null;
  operatorModel: {
    type: "single-signer" | "multisig" | "unknown";
    details: string;
  };
}

export interface AuditAction {
  title: string;
  caseId: string | null;
  blobHash: string | null;
  txHash: string | null;
  happenedAt: string | null;
}

export interface AuditCaseView {
  caseId: string;
  summary: string;
  participantKey: string | null;
  participantName: string | null;
  linkedSettlementId: string | null;
  linkedTxHashes: string[];
  disclosureGrantTxHash: string | null;
  disclosureAccessTxHash: string | null;
  blobHash: string | null;
  notes: string[];
}
