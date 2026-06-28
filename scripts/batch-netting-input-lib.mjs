import {
  DEFAULT_STATEMENT_HASH_HEX,
  FIELD_MODULUS,
  fromHex,
  poseidonHash,
  splitStatementHashHex,
} from "./unencumbered-input-lib.mjs";

function encodeSignedField(value) {
  const bigint = BigInt(value);
  return bigint >= 0n ? bigint : FIELD_MODULUS + bigint;
}

export async function buildBatchNettingFixture(options = {}) {
  const {
    statementHashHex,
    statementHashHi,
    statementHashLo,
  } = splitStatementHashHex(options.statementHashHex ?? DEFAULT_STATEMENT_HASH_HEX);

  const participantAIdHash = fromHex(
    options.participantAIdHashHex ??
      "0x101122223333444455556666777788889999aaaabbbbccccddddeeeeffff0001",
  );
  const participantBIdHash = fromHex(
    options.participantBIdHashHex ??
      "0x20223333444455556666777788889999aaaabbbbccccddddeeeeffff00001111",
  );
  const participantCIdHash = fromHex(
    options.participantCIdHashHex ??
      "0x3033444455556666777788889999aaaabbbbccccddddeeeeffff000011112222",
  );
  const instrumentIdHash = fromHex(
    options.instrumentIdHashHex ??
      "0x404455556666777788889999aaaabbbbccccddddeeeeffff0000111122223333",
  );
  const batchIdHash = fromHex(
    options.batchIdHex ??
      "0x50556666777788889999aaaabbbbccccddddeeeeffff00001111222233334444",
  );
  const batchSalt = fromHex(
    options.batchSaltHex ??
      "0x6066777788889999aaaabbbbccccddddeeeeffff000011112222333344445555",
  );

  const tradeA = {
    bidParticipantIdHash: participantAIdHash,
    askParticipantIdHash: participantBIdHash,
    bidLimitPrice: BigInt(options.tradeABidLimitPrice ?? 101),
    askLimitPrice: BigInt(options.tradeAAskLimitPrice ?? 99),
    clearPrice: BigInt(options.tradeAClearPrice ?? 100),
    clearQuantity: BigInt(options.tradeAClearQuantity ?? 10),
    bidOrderSalt: fromHex(
      options.tradeABidOrderSaltHex ??
        "0x707788889999aaaabbbbccccddddeeeeffff0000111122223333444455556666",
    ),
    askOrderSalt: fromHex(
      options.tradeAAskOrderSaltHex ??
        "0x80889999aaaabbbbccccddddeeeeffff00001111222233334444555566667777",
    ),
    executionSalt: fromHex(
      options.tradeAExecutionSaltHex ??
        "0x9099aaaabbbbccccddddeeeeffff000011112222333344445555666677778888",
    ),
    bidCollateralProofReceiptId: fromHex(
      options.tradeABidCollateralProofReceiptIdHex ??
        "0xa0aabbbbccccddddeeeeffff0000111122223333444455556666777788889999",
    ),
    bidEncumbranceProofReceiptId: fromHex(
      options.tradeABidEncumbranceProofReceiptIdHex ??
        "0xb0bbccccddddeeeeffff0000111122223333444455556666777788889999aaaa",
    ),
    askCollateralProofReceiptId: fromHex(
      options.tradeAAskCollateralProofReceiptIdHex ??
        "0xc0ccddddeeeeffff0000111122223333444455556666777788889999aaaabbbb",
    ),
    askEncumbranceProofReceiptId: fromHex(
      options.tradeAAskEncumbranceProofReceiptIdHex ??
        "0xd0ddeeeeffff0000111122223333444455556666777788889999aaaabbbbcccc",
    ),
  };
  const tradeB = {
    bidParticipantIdHash: participantCIdHash,
    askParticipantIdHash: participantAIdHash,
    bidLimitPrice: BigInt(options.tradeBBidLimitPrice ?? 111),
    askLimitPrice: BigInt(options.tradeBAskLimitPrice ?? 109),
    clearPrice: BigInt(options.tradeBClearPrice ?? 110),
    clearQuantity: BigInt(options.tradeBClearQuantity ?? 6),
    bidOrderSalt: fromHex(
      options.tradeBBidOrderSaltHex ??
        "0xe0eeffff0000111122223333444455556666777788889999aaaabbbbccccdddd",
    ),
    askOrderSalt: fromHex(
      options.tradeBAskOrderSaltHex ??
        "0xf0ff0000111122223333444455556666777788889999aaaabbbbccccddddeeee",
    ),
    executionSalt: fromHex(
      options.tradeBExecutionSaltHex ??
        "0x111122223333444455556666777788889999aaaabbbbccccddddeeeeffff0000",
    ),
    bidCollateralProofReceiptId: fromHex(
      options.tradeBBidCollateralProofReceiptIdHex ??
        "0x12123333444455556666777788889999aaaabbbbccccddddeeeeffff00001111",
    ),
    bidEncumbranceProofReceiptId: fromHex(
      options.tradeBBidEncumbranceProofReceiptIdHex ??
        "0x1313444455556666777788889999aaaabbbbccccddddeeeeffff000011112222",
    ),
    askCollateralProofReceiptId: fromHex(
      options.tradeBAskCollateralProofReceiptIdHex ??
        "0x141455556666777788889999aaaabbbbccccddddeeeeffff0000111122223333",
    ),
    askEncumbranceProofReceiptId: fromHex(
      options.tradeBAskEncumbranceProofReceiptIdHex ??
        "0x15156666777788889999aaaabbbbccccddddeeeeffff00001111222233334444",
    ),
  };

  tradeA.bidOrderCommitment = await poseidonHash([
    tradeA.bidParticipantIdHash,
    instrumentIdHash,
    tradeA.bidLimitPrice,
    tradeA.clearQuantity,
    tradeA.bidOrderSalt,
    tradeA.bidCollateralProofReceiptId,
    tradeA.bidEncumbranceProofReceiptId,
  ]);
  tradeA.askOrderCommitment = await poseidonHash([
    tradeA.askParticipantIdHash,
    instrumentIdHash,
    tradeA.askLimitPrice,
    tradeA.clearQuantity,
    tradeA.askOrderSalt,
    tradeA.askCollateralProofReceiptId,
    tradeA.askEncumbranceProofReceiptId,
  ]);
  tradeB.bidOrderCommitment = await poseidonHash([
    tradeB.bidParticipantIdHash,
    instrumentIdHash,
    tradeB.bidLimitPrice,
    tradeB.clearQuantity,
    tradeB.bidOrderSalt,
    tradeB.bidCollateralProofReceiptId,
    tradeB.bidEncumbranceProofReceiptId,
  ]);
  tradeB.askOrderCommitment = await poseidonHash([
    tradeB.askParticipantIdHash,
    instrumentIdHash,
    tradeB.askLimitPrice,
    tradeB.clearQuantity,
    tradeB.askOrderSalt,
    tradeB.askCollateralProofReceiptId,
    tradeB.askEncumbranceProofReceiptId,
  ]);

  const executionCommitmentA = await poseidonHash([
    tradeA.bidParticipantIdHash,
    tradeA.askParticipantIdHash,
    instrumentIdHash,
    tradeA.clearPrice,
    tradeA.clearQuantity,
    tradeA.executionSalt,
  ]);
  const executionCommitmentB = await poseidonHash([
    tradeB.bidParticipantIdHash,
    tradeB.askParticipantIdHash,
    instrumentIdHash,
    tradeB.clearPrice,
    tradeB.clearQuantity,
    tradeB.executionSalt,
  ]);

  const tradeNullifierA = await poseidonHash([executionCommitmentA, batchSalt, 1n]);
  const tradeNullifierB = await poseidonHash([executionCommitmentB, batchSalt, 2n]);

  const slotParticipantIdHash = [participantAIdHash, participantBIdHash, participantCIdHash];
  const slotNetQtySigned = [4n, -10n, 6n];
  const slotNetCashSigned = [-340n, 1000n, -660n];
  const slotNetQty = slotNetQtySigned.map(encodeSignedField);
  const slotNetCash = slotNetCashSigned.map(encodeSignedField);

  const slotCommitments = await Promise.all(
    slotParticipantIdHash.map((participantIdHash, index) =>
      poseidonHash([participantIdHash, slotNetQty[index], slotNetCash[index]]),
    ),
  );
  const netVectorHash = await poseidonHash([
    slotCommitments[0],
    slotCommitments[1],
    slotCommitments[2],
    instrumentIdHash,
    batchSalt,
  ]);
  const settlementCommitment = await poseidonHash([
    executionCommitmentA,
    executionCommitmentB,
    netVectorHash,
    tradeNullifierA,
    tradeNullifierB,
  ]);

  const input = {
    executionCommitmentA,
    executionCommitmentB,
    settlementCommitment,
    tradeNullifierA,
    tradeNullifierB,
    statementHashHi,
    statementHashLo,
    instrumentIdHash,
    tradeBidParticipantIdHash: [
      tradeA.bidParticipantIdHash,
      tradeB.bidParticipantIdHash,
    ],
    tradeAskParticipantIdHash: [
      tradeA.askParticipantIdHash,
      tradeB.askParticipantIdHash,
    ],
    tradeClearPrice: [tradeA.clearPrice, tradeB.clearPrice],
    tradeClearQuantity: [tradeA.clearQuantity, tradeB.clearQuantity],
    tradeExecutionSalt: [tradeA.executionSalt, tradeB.executionSalt],
    slotParticipantIdHash,
    slotNetQty,
    slotNetCash,
    batchSalt,
  };

  const expected = {
    publicSignals: [
      executionCommitmentA,
      executionCommitmentB,
      settlementCommitment,
      tradeNullifierA,
      tradeNullifierB,
      statementHashHi,
      statementHashLo,
    ],
    bundle: {
      batchIdHash,
      instrumentIdHash,
      netVectorHash,
      settlementCommitment,
      tradeNullifierA,
      tradeNullifierB,
      tradeA,
      tradeB,
      slotNetQtySigned,
      slotNetCashSigned,
    },
    internal: {
      statementHashHex,
    },
  };

  return { input, expected };
}
