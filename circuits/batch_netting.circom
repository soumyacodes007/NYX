pragma circom 2.1.6;

include "circomlib/circuits/poseidon.circom";
include "circomlib/circuits/comparators.circom";

template BatchNetting() {
    signal input executionCommitmentA;
    signal input executionCommitmentB;
    signal input settlementCommitment;
    signal input tradeNullifierA;
    signal input tradeNullifierB;
    signal input statementHashHi;
    signal input statementHashLo;

    signal input instrumentIdHash;
    signal input tradeBidParticipantIdHash[2];
    signal input tradeAskParticipantIdHash[2];
    signal input tradeClearPrice[2];
    signal input tradeClearQuantity[2];
    signal input tradeExecutionSalt[2];

    signal input slotParticipantIdHash[3];
    signal input slotNetQty[3];
    signal input slotNetCash[3];
    signal input batchSalt;

    component executionHasherA = Poseidon(6);
    executionHasherA.inputs[0] <== tradeBidParticipantIdHash[0];
    executionHasherA.inputs[1] <== tradeAskParticipantIdHash[0];
    executionHasherA.inputs[2] <== instrumentIdHash;
    executionHasherA.inputs[3] <== tradeClearPrice[0];
    executionHasherA.inputs[4] <== tradeClearQuantity[0];
    executionHasherA.inputs[5] <== tradeExecutionSalt[0];

    component executionHasherB = Poseidon(6);
    executionHasherB.inputs[0] <== tradeBidParticipantIdHash[1];
    executionHasherB.inputs[1] <== tradeAskParticipantIdHash[1];
    executionHasherB.inputs[2] <== instrumentIdHash;
    executionHasherB.inputs[3] <== tradeClearPrice[1];
    executionHasherB.inputs[4] <== tradeClearQuantity[1];
    executionHasherB.inputs[5] <== tradeExecutionSalt[1];

    component tradeNullifierHasherA = Poseidon(3);
    tradeNullifierHasherA.inputs[0] <== executionCommitmentA;
    tradeNullifierHasherA.inputs[1] <== batchSalt;
    tradeNullifierHasherA.inputs[2] <== 1;

    component tradeNullifierHasherB = Poseidon(3);
    tradeNullifierHasherB.inputs[0] <== executionCommitmentB;
    tradeNullifierHasherB.inputs[1] <== batchSalt;
    tradeNullifierHasherB.inputs[2] <== 2;

    component slotDistinct01 = IsEqual();
    slotDistinct01.in[0] <== slotParticipantIdHash[0];
    slotDistinct01.in[1] <== slotParticipantIdHash[1];

    component slotDistinct02 = IsEqual();
    slotDistinct02.in[0] <== slotParticipantIdHash[0];
    slotDistinct02.in[1] <== slotParticipantIdHash[2];

    component slotDistinct12 = IsEqual();
    slotDistinct12.in[0] <== slotParticipantIdHash[1];
    slotDistinct12.in[1] <== slotParticipantIdHash[2];

    component tradeASelfCheck = IsEqual();
    tradeASelfCheck.in[0] <== tradeBidParticipantIdHash[0];
    tradeASelfCheck.in[1] <== tradeAskParticipantIdHash[0];

    component tradeBSelfCheck = IsEqual();
    tradeBSelfCheck.in[0] <== tradeBidParticipantIdHash[1];
    tradeBSelfCheck.in[1] <== tradeAskParticipantIdHash[1];

    component qtyNonZeroA = IsZero();
    qtyNonZeroA.in <== tradeClearQuantity[0];

    component qtyNonZeroB = IsZero();
    qtyNonZeroB.in <== tradeClearQuantity[1];

    component priceNonZeroA = IsZero();
    priceNonZeroA.in <== tradeClearPrice[0];

    component priceNonZeroB = IsZero();
    priceNonZeroB.in <== tradeClearPrice[1];

    signal cashValue[2];
    cashValue[0] <== tradeClearPrice[0] * tradeClearQuantity[0];
    cashValue[1] <== tradeClearPrice[1] * tradeClearQuantity[1];

    component bidSlotEq[2][3];
    component askSlotEq[2][3];
    signal bidQtyContribution[2][3];
    signal askQtyContribution[2][3];
    signal bidCashContribution[2][3];
    signal askCashContribution[2][3];
    signal slotUsage[3];
    signal slotComputedQty[3];
    signal slotComputedCash[3];
    component slotUsageNonZero[3];
    component slotCommitmentHasher[3];

    signal tradeABidPresence;
    signal tradeAAskPresence;
    signal tradeBBidPresence;
    signal tradeBAskPresence;

    for (var tradeIndex = 0; tradeIndex < 2; tradeIndex++) {
        for (var slotIndex = 0; slotIndex < 3; slotIndex++) {
            bidSlotEq[tradeIndex][slotIndex] = IsEqual();
            bidSlotEq[tradeIndex][slotIndex].in[0] <== slotParticipantIdHash[slotIndex];
            bidSlotEq[tradeIndex][slotIndex].in[1] <== tradeBidParticipantIdHash[tradeIndex];

            askSlotEq[tradeIndex][slotIndex] = IsEqual();
            askSlotEq[tradeIndex][slotIndex].in[0] <== slotParticipantIdHash[slotIndex];
            askSlotEq[tradeIndex][slotIndex].in[1] <== tradeAskParticipantIdHash[tradeIndex];
        }
    }

    tradeABidPresence <== bidSlotEq[0][0].out + bidSlotEq[0][1].out + bidSlotEq[0][2].out;
    tradeAAskPresence <== askSlotEq[0][0].out + askSlotEq[0][1].out + askSlotEq[0][2].out;
    tradeBBidPresence <== bidSlotEq[1][0].out + bidSlotEq[1][1].out + bidSlotEq[1][2].out;
    tradeBAskPresence <== askSlotEq[1][0].out + askSlotEq[1][1].out + askSlotEq[1][2].out;

    for (var slotIndex = 0; slotIndex < 3; slotIndex++) {
        slotUsage[slotIndex] <==
            bidSlotEq[0][slotIndex].out +
            askSlotEq[0][slotIndex].out +
            bidSlotEq[1][slotIndex].out +
            askSlotEq[1][slotIndex].out;
        bidQtyContribution[0][slotIndex] <== bidSlotEq[0][slotIndex].out * tradeClearQuantity[0];
        askQtyContribution[0][slotIndex] <== askSlotEq[0][slotIndex].out * tradeClearQuantity[0];
        bidQtyContribution[1][slotIndex] <== bidSlotEq[1][slotIndex].out * tradeClearQuantity[1];
        askQtyContribution[1][slotIndex] <== askSlotEq[1][slotIndex].out * tradeClearQuantity[1];
        askCashContribution[0][slotIndex] <== askSlotEq[0][slotIndex].out * cashValue[0];
        bidCashContribution[0][slotIndex] <== bidSlotEq[0][slotIndex].out * cashValue[0];
        askCashContribution[1][slotIndex] <== askSlotEq[1][slotIndex].out * cashValue[1];
        bidCashContribution[1][slotIndex] <== bidSlotEq[1][slotIndex].out * cashValue[1];

        slotComputedQty[slotIndex] <==
            bidQtyContribution[0][slotIndex] -
            askQtyContribution[0][slotIndex] +
            bidQtyContribution[1][slotIndex] -
            askQtyContribution[1][slotIndex];
        slotComputedCash[slotIndex] <==
            askCashContribution[0][slotIndex] -
            bidCashContribution[0][slotIndex] +
            askCashContribution[1][slotIndex] -
            bidCashContribution[1][slotIndex];

        slotUsageNonZero[slotIndex] = IsZero();
        slotUsageNonZero[slotIndex].in <== slotUsage[slotIndex];

        slotCommitmentHasher[slotIndex] = Poseidon(3);
        slotCommitmentHasher[slotIndex].inputs[0] <== slotParticipantIdHash[slotIndex];
        slotCommitmentHasher[slotIndex].inputs[1] <== slotNetQty[slotIndex];
        slotCommitmentHasher[slotIndex].inputs[2] <== slotNetCash[slotIndex];

        slotNetQty[slotIndex] === slotComputedQty[slotIndex];
        slotNetCash[slotIndex] === slotComputedCash[slotIndex];
        slotUsageNonZero[slotIndex].out === 0;
    }

    component netVectorHasher = Poseidon(5);
    netVectorHasher.inputs[0] <== slotCommitmentHasher[0].out;
    netVectorHasher.inputs[1] <== slotCommitmentHasher[1].out;
    netVectorHasher.inputs[2] <== slotCommitmentHasher[2].out;
    netVectorHasher.inputs[3] <== instrumentIdHash;
    netVectorHasher.inputs[4] <== batchSalt;

    component settlementHasher = Poseidon(5);
    settlementHasher.inputs[0] <== executionCommitmentA;
    settlementHasher.inputs[1] <== executionCommitmentB;
    settlementHasher.inputs[2] <== netVectorHasher.out;
    settlementHasher.inputs[3] <== tradeNullifierA;
    settlementHasher.inputs[4] <== tradeNullifierB;

    executionCommitmentA === executionHasherA.out;
    executionCommitmentB === executionHasherB.out;
    tradeNullifierA === tradeNullifierHasherA.out;
    tradeNullifierB === tradeNullifierHasherB.out;
    settlementCommitment === settlementHasher.out;

    slotDistinct01.out === 0;
    slotDistinct02.out === 0;
    slotDistinct12.out === 0;
    tradeASelfCheck.out === 0;
    tradeBSelfCheck.out === 0;
    qtyNonZeroA.out === 0;
    qtyNonZeroB.out === 0;
    priceNonZeroA.out === 0;
    priceNonZeroB.out === 0;
    tradeABidPresence === 1;
    tradeAAskPresence === 1;
    tradeBBidPresence === 1;
    tradeBAskPresence === 1;
}

component main {public [
    executionCommitmentA,
    executionCommitmentB,
    settlementCommitment,
    tradeNullifierA,
    tradeNullifierB,
    statementHashHi,
    statementHashLo
]} = BatchNetting();
