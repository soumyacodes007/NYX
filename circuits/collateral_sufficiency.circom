pragma circom 2.1.6;

include "circomlib/circuits/poseidon.circom";
include "circomlib/circuits/comparators.circom";

template CollateralSufficiency() {
    signal input participantIdHash;
    signal input portfolioCommitment;
    signal input proofNonce;
    signal input statementHashHi;
    signal input statementHashLo;

    signal input assetIdHash[3];
    signal input balance[3];
    signal input price[3];
    signal input haircutBps[3];
    signal input assetSalt[3];
    signal input requiredMargin;

    signal adjustedValue[3];
    signal valueTimesPrice[3];
    component lineHashers[3];

    for (var i = 0; i < 3; i++) {
        lineHashers[i] = Poseidon(5);
        lineHashers[i].inputs[0] <== assetIdHash[i];
        lineHashers[i].inputs[1] <== balance[i];
        lineHashers[i].inputs[2] <== price[i];
        lineHashers[i].inputs[3] <== haircutBps[i];
        lineHashers[i].inputs[4] <== assetSalt[i];

        valueTimesPrice[i] <== balance[i] * price[i];
        adjustedValue[i] <== valueTimesPrice[i] * haircutBps[i];
    }

    component portfolioHasher = Poseidon(5);
    portfolioHasher.inputs[0] <== lineHashers[0].out;
    portfolioHasher.inputs[1] <== lineHashers[1].out;
    portfolioHasher.inputs[2] <== lineHashers[2].out;
    portfolioHasher.inputs[3] <== participantIdHash;
    portfolioHasher.inputs[4] <== proofNonce;

    signal totalAdjustedValue;
    totalAdjustedValue <== adjustedValue[0] + adjustedValue[1] + adjustedValue[2];

    component marginCheck = LessEqThan(128);
    marginCheck.in[0] <== requiredMargin;
    marginCheck.in[1] <== totalAdjustedValue;

    portfolioCommitment === portfolioHasher.out;
    marginCheck.out === 1;
}

component main {public [
    participantIdHash,
    portfolioCommitment,
    proofNonce,
    statementHashHi,
    statementHashLo
]} = CollateralSufficiency();
