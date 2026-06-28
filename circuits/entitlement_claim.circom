pragma circom 2.1.6;

include "circomlib/circuits/poseidon.circom";
include "circomlib/circuits/comparators.circom";

template MerkleRootPoseidon(depth) {
    signal input leaf;
    signal input pathElements[depth];
    signal input pathIndices[depth];
    signal output root;

    signal hashes[depth + 1];
    signal left[depth];
    signal right[depth];
    component hashers[depth];

    hashes[0] <== leaf;

    for (var i = 0; i < depth; i++) {
        pathIndices[i] * (pathIndices[i] - 1) === 0;

        left[i] <== hashes[i] + pathIndices[i] * (pathElements[i] - hashes[i]);
        right[i] <== pathElements[i] + pathIndices[i] * (hashes[i] - pathElements[i]);

        hashers[i] = Poseidon(2);
        hashers[i].inputs[0] <== left[i];
        hashers[i].inputs[1] <== right[i];
        hashes[i + 1] <== hashers[i].out;
    }

    root <== hashes[depth];
}

template EntitlementClaim(depth) {
    signal input participantIdHash;
    signal input assetIdHash;
    signal input eventIdHash;
    signal input eventRoot;
    signal input claimCommitment;
    signal input claimNullifier;
    signal input claimAmount;
    signal input statementHashHi;
    signal input statementHashLo;

    signal input entitlementQuantity;
    signal input payoutRate;
    signal input snapshotSalt;
    signal input claimSalt;
    signal input pathElements[depth];
    signal input pathIndices[depth];

    component qtyNonZero = IsZero();
    qtyNonZero.in <== entitlementQuantity;

    component rateNonZero = IsZero();
    rateNonZero.in <== payoutRate;

    component leafHasher = Poseidon(5);
    leafHasher.inputs[0] <== participantIdHash;
    leafHasher.inputs[1] <== assetIdHash;
    leafHasher.inputs[2] <== eventIdHash;
    leafHasher.inputs[3] <== entitlementQuantity;
    leafHasher.inputs[4] <== snapshotSalt;

    component merkle = MerkleRootPoseidon(depth);
    merkle.leaf <== leafHasher.out;
    for (var i = 0; i < depth; i++) {
        merkle.pathElements[i] <== pathElements[i];
        merkle.pathIndices[i] <== pathIndices[i];
    }

    signal computedClaimAmount;
    computedClaimAmount <== entitlementQuantity * payoutRate;

    component commitmentHasher = Poseidon(5);
    commitmentHasher.inputs[0] <== eventIdHash;
    commitmentHasher.inputs[1] <== participantIdHash;
    commitmentHasher.inputs[2] <== entitlementQuantity;
    commitmentHasher.inputs[3] <== computedClaimAmount;
    commitmentHasher.inputs[4] <== claimSalt;

    component nullifierHasher = Poseidon(3);
    nullifierHasher.inputs[0] <== eventIdHash;
    nullifierHasher.inputs[1] <== participantIdHash;
    nullifierHasher.inputs[2] <== claimSalt;

    eventRoot === merkle.root;
    claimAmount === computedClaimAmount;
    claimCommitment === commitmentHasher.out;
    claimNullifier === nullifierHasher.out;
    qtyNonZero.out === 0;
    rateNonZero.out === 0;
}

component main {public [
    participantIdHash,
    assetIdHash,
    eventIdHash,
    eventRoot,
    claimCommitment,
    claimNullifier,
    claimAmount,
    statementHashHi,
    statementHashLo
]} = EntitlementClaim(4);
