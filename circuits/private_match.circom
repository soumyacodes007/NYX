pragma circom 2.1.6;

include "circomlib/circuits/poseidon.circom";
include "circomlib/circuits/comparators.circom";

template PrivateMatch() {
    signal input bidOrderCommitment;
    signal input askOrderCommitment;
    signal input instrumentIdHash;
    signal input executionCommitment;
    signal input statementHashHi;
    signal input statementHashLo;

    signal input bidParticipantIdHash;
    signal input bidLimitPrice;
    signal input bidQuantity;
    signal input bidOrderSalt;
    signal input bidCollateralProofReceiptId;
    signal input bidEncumbranceProofReceiptId;

    signal input askParticipantIdHash;
    signal input askLimitPrice;
    signal input askQuantity;
    signal input askOrderSalt;
    signal input askCollateralProofReceiptId;
    signal input askEncumbranceProofReceiptId;

    signal input clearPrice;
    signal input clearQuantity;
    signal input executionSalt;

    component bidHasher = Poseidon(7);
    bidHasher.inputs[0] <== bidParticipantIdHash;
    bidHasher.inputs[1] <== instrumentIdHash;
    bidHasher.inputs[2] <== bidLimitPrice;
    bidHasher.inputs[3] <== bidQuantity;
    bidHasher.inputs[4] <== bidOrderSalt;
    bidHasher.inputs[5] <== bidCollateralProofReceiptId;
    bidHasher.inputs[6] <== bidEncumbranceProofReceiptId;

    component askHasher = Poseidon(7);
    askHasher.inputs[0] <== askParticipantIdHash;
    askHasher.inputs[1] <== instrumentIdHash;
    askHasher.inputs[2] <== askLimitPrice;
    askHasher.inputs[3] <== askQuantity;
    askHasher.inputs[4] <== askOrderSalt;
    askHasher.inputs[5] <== askCollateralProofReceiptId;
    askHasher.inputs[6] <== askEncumbranceProofReceiptId;

    component executionHasher = Poseidon(6);
    executionHasher.inputs[0] <== bidParticipantIdHash;
    executionHasher.inputs[1] <== askParticipantIdHash;
    executionHasher.inputs[2] <== instrumentIdHash;
    executionHasher.inputs[3] <== clearPrice;
    executionHasher.inputs[4] <== clearQuantity;
    executionHasher.inputs[5] <== executionSalt;

    component bidCrossesAsk = LessEqThan(64);
    bidCrossesAsk.in[0] <== askLimitPrice;
    bidCrossesAsk.in[1] <== bidLimitPrice;

    component priceAboveAsk = LessEqThan(64);
    priceAboveAsk.in[0] <== askLimitPrice;
    priceAboveAsk.in[1] <== clearPrice;

    component priceBelowBid = LessEqThan(64);
    priceBelowBid.in[0] <== clearPrice;
    priceBelowBid.in[1] <== bidLimitPrice;

    component qtyWithinBid = LessEqThan(64);
    qtyWithinBid.in[0] <== clearQuantity;
    qtyWithinBid.in[1] <== bidQuantity;

    component qtyWithinAsk = LessEqThan(64);
    qtyWithinAsk.in[0] <== clearQuantity;
    qtyWithinAsk.in[1] <== askQuantity;

    component qtyNonZero = IsZero();
    qtyNonZero.in <== clearQuantity;

    bidOrderCommitment === bidHasher.out;
    askOrderCommitment === askHasher.out;
    executionCommitment === executionHasher.out;
    bidCrossesAsk.out === 1;
    priceAboveAsk.out === 1;
    priceBelowBid.out === 1;
    qtyWithinBid.out === 1;
    qtyWithinAsk.out === 1;
    qtyNonZero.out === 0;
}

component main {public [
    bidOrderCommitment,
    askOrderCommitment,
    instrumentIdHash,
    executionCommitment,
    statementHashHi,
    statementHashLo
]} = PrivateMatch();
