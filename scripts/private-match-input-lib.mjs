import {
  DEFAULT_STATEMENT_HASH_HEX,
  fromHex,
  poseidonHash,
  splitStatementHashHex,
} from "./unencumbered-input-lib.mjs";

export async function buildPrivateMatchFixture(options = {}) {
  const {
    statementHashHex,
    statementHashHi,
    statementHashLo,
  } = splitStatementHashHex(options.statementHashHex ?? DEFAULT_STATEMENT_HASH_HEX);

  const bidParticipantIdHash = fromHex(
    options.bidParticipantIdHashHex ??
      "0x101122223333444455556666777788889999aaaabbbbccccddddeeeeffff0001",
  );
  const askParticipantIdHash = fromHex(
    options.askParticipantIdHashHex ??
      "0x20223333444455556666777788889999aaaabbbbccccddddeeeeffff00001111",
  );
  const instrumentIdHash = fromHex(
    options.instrumentIdHashHex ??
      "0x3033444455556666777788889999aaaabbbbccccddddeeeeffff000011112222",
  );

  const bidLimitPrice = BigInt(options.bidLimitPrice ?? 10_500);
  const askLimitPrice = BigInt(options.askLimitPrice ?? 9_900);
  const bidQuantity = BigInt(options.bidQuantity ?? 75);
  const askQuantity = BigInt(options.askQuantity ?? 60);
  const clearPrice = BigInt(options.clearPrice ?? 10_000);
  const clearQuantity = BigInt(options.clearQuantity ?? 60);

  const bidOrderSalt = fromHex(
    options.bidOrderSaltHex ??
      "0x404455556666777788889999aaaabbbbccccddddeeeeffff0000111122223333",
  );
  const askOrderSalt = fromHex(
    options.askOrderSaltHex ??
      "0x50556666777788889999aaaabbbbccccddddeeeeffff00001111222233334444",
  );
  const executionSalt = fromHex(
    options.executionSaltHex ??
      "0x6066777788889999aaaabbbbccccddddeeeeffff000011112222333344445555",
  );

  const bidCollateralProofReceiptId = fromHex(
    options.bidCollateralProofReceiptIdHex ??
      "0x707788889999aaaabbbbccccddddeeeeffff0000111122223333444455556666",
  );
  const bidEncumbranceProofReceiptId = fromHex(
    options.bidEncumbranceProofReceiptIdHex ??
      "0x80889999aaaabbbbccccddddeeeeffff00001111222233334444555566667777",
  );
  const askCollateralProofReceiptId = fromHex(
    options.askCollateralProofReceiptIdHex ??
      "0x9099aaaabbbbccccddddeeeeffff000011112222333344445555666677778888",
  );
  const askEncumbranceProofReceiptId = fromHex(
    options.askEncumbranceProofReceiptIdHex ??
      "0xa0aabbbbccccddddeeeeffff0000111122223333444455556666777788889999",
  );

  const bidOrderCommitment = await poseidonHash([
    bidParticipantIdHash,
    instrumentIdHash,
    bidLimitPrice,
    bidQuantity,
    bidOrderSalt,
    bidCollateralProofReceiptId,
    bidEncumbranceProofReceiptId,
  ]);
  const askOrderCommitment = await poseidonHash([
    askParticipantIdHash,
    instrumentIdHash,
    askLimitPrice,
    askQuantity,
    askOrderSalt,
    askCollateralProofReceiptId,
    askEncumbranceProofReceiptId,
  ]);
  const executionCommitment = await poseidonHash([
    bidParticipantIdHash,
    askParticipantIdHash,
    instrumentIdHash,
    clearPrice,
    clearQuantity,
    executionSalt,
  ]);

  const input = {
    bidOrderCommitment,
    askOrderCommitment,
    instrumentIdHash,
    executionCommitment,
    statementHashHi,
    statementHashLo,
    bidParticipantIdHash,
    bidLimitPrice,
    bidQuantity,
    bidOrderSalt,
    bidCollateralProofReceiptId,
    bidEncumbranceProofReceiptId,
    askParticipantIdHash,
    askLimitPrice,
    askQuantity,
    askOrderSalt,
    askCollateralProofReceiptId,
    askEncumbranceProofReceiptId,
    clearPrice,
    clearQuantity,
    executionSalt,
  };

  const expected = {
    publicSignals: [
      bidOrderCommitment,
      askOrderCommitment,
      instrumentIdHash,
      executionCommitment,
      statementHashHi,
      statementHashLo,
    ],
    internal: {
      statementHashHex,
    },
  };

  return { input, expected };
}
