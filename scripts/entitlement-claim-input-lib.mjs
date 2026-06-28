import {
  DEFAULT_STATEMENT_HASH_HEX,
  DEPTH,
  LEAF_COUNT,
  fromHex,
  poseidonHash,
  splitStatementHashHex,
} from "./unencumbered-input-lib.mjs";

async function buildTree(leaves) {
  const levels = [leaves];
  let current = leaves;
  while (current.length > 1) {
    const next = [];
    for (let i = 0; i < current.length; i += 2) {
      next.push(await poseidonHash([current[i], current[i + 1]]));
    }
    levels.push(next);
    current = next;
  }
  return levels;
}

function merkleProof(levels, leafIndex) {
  const pathElements = [];
  const pathIndices = [];
  let index = leafIndex;

  for (let level = 0; level < levels.length - 1; level += 1) {
    const siblingIndex = index ^ 1;
    pathElements.push(levels[level][siblingIndex]);
    pathIndices.push(index & 1);
    index >>= 1;
  }

  return { pathElements, pathIndices };
}

export async function buildEntitlementClaimFixture(options = {}) {
  const {
    statementHashHex,
    statementHashHi,
    statementHashLo,
  } = splitStatementHashHex(options.statementHashHex ?? DEFAULT_STATEMENT_HASH_HEX);

  const participantIdHash = fromHex(
    options.participantIdHashHex ??
      "0x101122223333444455556666777788889999aaaabbbbccccddddeeeeffff0001",
  );
  const assetIdHash = fromHex(
    options.assetIdHashHex ??
      "0x20223333444455556666777788889999aaaabbbbccccddddeeeeffff00001111",
  );
  const eventIdHash = fromHex(
    options.eventIdHashHex ??
      "0x3033444455556666777788889999aaaabbbbccccddddeeeeffff000011112222",
  );
  const entitlementQuantity = BigInt(options.entitlementQuantity ?? 125);
  const payoutRate = BigInt(options.payoutRate ?? 25);
  const snapshotSalt = fromHex(
    options.snapshotSaltHex ??
      "0x404455556666777788889999aaaabbbbccccddddeeeeffff0000111122223333",
  );
  const claimSalt = fromHex(
    options.claimSaltHex ??
      "0x50556666777788889999aaaabbbbccccddddeeeeffff00001111222233334444",
  );

  const claimAmount = entitlementQuantity * payoutRate;
  const targetLeaf = await poseidonHash([
    participantIdHash,
    assetIdHash,
    eventIdHash,
    entitlementQuantity,
    snapshotSalt,
  ]);

  const leafIndex = 6;
  const leaves = [];
  for (let index = 0; index < LEAF_COUNT; index += 1) {
    if (index === leafIndex) {
      leaves.push(targetLeaf);
      continue;
    }

    const fillerParticipant = await poseidonHash([
      participantIdHash,
      BigInt(index + 1),
    ]);
    const fillerLeaf = await poseidonHash([
      fillerParticipant,
      assetIdHash,
      eventIdHash,
      BigInt(10 + index),
      BigInt(9_000 + index),
    ]);
    leaves.push(fillerLeaf);
  }

  const levels = await buildTree(leaves);
  const eventRoot = levels.at(-1)[0];
  const { pathElements, pathIndices } = merkleProof(levels, leafIndex);
  const claimCommitment = await poseidonHash([
    eventIdHash,
    participantIdHash,
    entitlementQuantity,
    claimAmount,
    claimSalt,
  ]);
  const claimNullifier = await poseidonHash([
    eventIdHash,
    participantIdHash,
    claimSalt,
  ]);

  const input = {
    participantIdHash,
    assetIdHash,
    eventIdHash,
    eventRoot,
    claimCommitment,
    claimNullifier,
    claimAmount,
    statementHashHi,
    statementHashLo,
    entitlementQuantity,
    payoutRate,
    snapshotSalt,
    claimSalt,
    pathElements,
    pathIndices,
  };

  const expected = {
    publicSignals: [
      participantIdHash,
      assetIdHash,
      eventIdHash,
      eventRoot,
      claimCommitment,
      claimNullifier,
      claimAmount,
      statementHashHi,
      statementHashLo,
    ],
    bundle: {
      assetIdHash,
      eventIdHash,
      eventRoot,
      claimCommitment,
      claimNullifier,
      claimAmount,
      entitlementQuantity,
    },
    internal: {
      statementHashHex,
      targetLeaf,
      leafIndex,
    },
  };

  return { input, expected };
}
