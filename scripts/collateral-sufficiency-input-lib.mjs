import {
  DEFAULT_STATEMENT_HASH_HEX,
  fromHex,
  poseidonHash,
  splitStatementHashHex,
} from "./unencumbered-input-lib.mjs";

export async function buildCollateralSufficiencyFixture(options = {}) {
  const {
    statementHashHex,
    statementHashHi,
    statementHashLo,
  } = splitStatementHashHex(options.statementHashHex ?? DEFAULT_STATEMENT_HASH_HEX);

  const participantIdHash = fromHex(
    options.participantIdHashHex ??
      "0x101122223333444455556666777788889999aaaabbbbccccddddeeeeffff0001",
  );
  const proofNonce = fromHex(
    options.proofNonceHex ??
      "0x20223333444455556666777788889999aaaabbbbccccddddeeeeffff00001111",
  );

  const assetIdHash = [
    fromHex(
      options.assetAIdHashHex ??
        "0x3033444455556666777788889999aaaabbbbccccddddeeeeffff000011112222",
    ),
    fromHex(
      options.assetBIdHashHex ??
        "0x404455556666777788889999aaaabbbbccccddddeeeeffff0000111122223333",
    ),
    fromHex(
      options.assetCIdHashHex ??
        "0x50556666777788889999aaaabbbbccccddddeeeeffff00001111222233334444",
    ),
  ];
  const balance = [
    BigInt(options.assetABalance ?? 5_000_000),
    BigInt(options.assetBBalance ?? 120_000),
    BigInt(options.assetCBalance ?? 40_000),
  ];
  const price = [
    BigInt(options.assetAPrice ?? 1_000_000),
    BigInt(options.assetBPrice ?? 125_000),
    BigInt(options.assetCPrice ?? 525_000),
  ];
  const haircutBps = [
    BigInt(options.assetAHaircutBps ?? 10_000),
    BigInt(options.assetBHaircutBps ?? 8_000),
    BigInt(options.assetCHaircutBps ?? 7_500),
  ];
  const assetSalt = [
    fromHex(
      options.assetASaltHex ??
        "0x6066777788889999aaaabbbbccccddddeeeeffff000011112222333344445555",
    ),
    fromHex(
      options.assetBSaltHex ??
        "0x707788889999aaaabbbbccccddddeeeeffff0000111122223333444455556666",
    ),
    fromHex(
      options.assetCSaltHex ??
        "0x80889999aaaabbbbccccddddeeeeffff00001111222233334444555566667777",
    ),
  ];
  const requiredMargin = BigInt(options.requiredMargin ?? 1_000_000_0000);

  const lineCommitments = await Promise.all(
    assetIdHash.map((asset, index) =>
      poseidonHash([
        asset,
        balance[index],
        price[index],
        haircutBps[index],
        assetSalt[index],
      ]),
    ),
  );
  const portfolioCommitment = await poseidonHash([
    lineCommitments[0],
    lineCommitments[1],
    lineCommitments[2],
    participantIdHash,
    proofNonce,
  ]);
  const totalAdjustedValue = assetIdHash.reduce(
    (acc, _, index) => acc + (balance[index] * price[index] * haircutBps[index]),
    0n,
  );

  const input = {
    participantIdHash,
    portfolioCommitment,
    proofNonce,
    statementHashHi,
    statementHashLo,
    assetIdHash,
    balance,
    price,
    haircutBps,
    assetSalt,
    requiredMargin,
  };

  const expected = {
    publicSignals: [
      participantIdHash,
      portfolioCommitment,
      proofNonce,
      statementHashHi,
      statementHashLo,
    ],
    bundle: {
      participantIdHash,
      portfolioCommitment,
      proofNonce,
      requiredMargin,
      totalAdjustedValue,
    },
    internal: {
      statementHashHex,
      lineCommitments,
    },
  };

  return { input, expected };
}
