pragma circom 2.1.6;

include "circomlib/circuits/poseidon.circom";

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

template UnencumberedLot(depth) {
    signal input participantIdHash;
    signal input assetIdHash;
    signal input availabilityRoot;
    signal input scopeHash;
    signal input reasonHash;
    signal input proofNonce;
    signal input statementHashHi;
    signal input statementHashLo;
    signal input lotNullifier;

    signal input lotIdHash;
    signal input quantity;
    signal input lotSalt;
    signal input pathElements[depth];
    signal input pathIndices[depth];

    component leafHasher = Poseidon(5);
    leafHasher.inputs[0] <== participantIdHash;
    leafHasher.inputs[1] <== assetIdHash;
    leafHasher.inputs[2] <== lotIdHash;
    leafHasher.inputs[3] <== quantity;
    leafHasher.inputs[4] <== lotSalt;

    component merkle = MerkleRootPoseidon(depth);
    merkle.leaf <== leafHasher.out;
    for (var i = 0; i < depth; i++) {
        merkle.pathElements[i] <== pathElements[i];
        merkle.pathIndices[i] <== pathIndices[i];
    }

    component nullifierHasher = Poseidon(5);
    nullifierHasher.inputs[0] <== scopeHash;
    nullifierHasher.inputs[1] <== reasonHash;
    nullifierHasher.inputs[2] <== proofNonce;
    nullifierHasher.inputs[3] <== lotIdHash;
    nullifierHasher.inputs[4] <== lotSalt;

    availabilityRoot === merkle.root;
    lotNullifier === nullifierHasher.out;
}

component main {public [
    participantIdHash,
    assetIdHash,
    availabilityRoot,
    scopeHash,
    reasonHash,
    proofNonce,
    statementHashHi,
    statementHashLo,
    lotNullifier
]} = UnencumberedLot(4);
