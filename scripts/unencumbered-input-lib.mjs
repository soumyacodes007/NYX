import { buildPoseidon } from "circomlibjs";

export const FIELD_MODULUS =
  21888242871839275222246405745257275088548364400416034343698204186575808495617n;
export const DEPTH = 4;
export const LEAF_COUNT = 1 << DEPTH;
export const DEFAULT_STATEMENT_HASH_HEX =
  "0x8899aabbccddeeff00112233445566778899aabbccddeeff0011223344556677";

export function fromHex(hex) {
  return BigInt(hex) % FIELD_MODULUS;
}

function normalizeHex32(hex) {
  const raw = hex.startsWith("0x") ? hex.slice(2) : hex;
  if (!/^[0-9a-fA-F]{64}$/.test(raw)) {
    throw new Error("statement hash must be exactly 32 bytes of hex");
  }
  return raw.toLowerCase();
}

export function splitStatementHashHex(hex) {
  const normalized = normalizeHex32(hex);
  return {
    statementHashHex: `0x${normalized}`,
    statementHashHi: BigInt(`0x${normalized.slice(0, 32)}`),
    statementHashLo: BigInt(`0x${normalized.slice(32)}`),
  };
}

export function stringifyDeep(value) {
  if (typeof value === "bigint") {
    return value.toString();
  }
  if (Array.isArray(value)) {
    return value.map(stringifyDeep);
  }
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value).map(([key, inner]) => [key, stringifyDeep(inner)]),
    );
  }
  return value;
}

let poseidonState;

async function getPoseidonState() {
  if (!poseidonState) {
    const poseidon = await buildPoseidon();
    poseidonState = {
      poseidon,
      field: poseidon.F,
    };
  }
  return poseidonState;
}

export async function poseidonHash(inputs) {
  const { poseidon, field } = await getPoseidonState();
  return BigInt(field.toString(poseidon(inputs.map(BigInt))));
}

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

export async function buildUnencumberedLotFixture(options = {}) {
  const {
    statementHashHex,
    statementHashHi,
    statementHashLo,
  } = splitStatementHashHex(options.statementHashHex ?? DEFAULT_STATEMENT_HASH_HEX);
  const participantIdHash = fromHex(
    options.participantIdHashHex ??
      "0x111122223333444455556666777788889999aaaabbbbccccddddeeeeffff0001",
  );
  const assetIdHash = fromHex(
    "0x22223333444455556666777788889999aaaabbbbccccddddeeeeffff00001111",
  );
  const scopeHash = fromHex(
    "0x3333444455556666777788889999aaaabbbbccccddddeeeeffff000011112222",
  );
  const reasonHash = fromHex(
    "0x444455556666777788889999aaaabbbbccccddddeeeeffff0000111122223333",
  );
  const proofNonce = fromHex(
    "0x55556666777788889999aaaabbbbccccddddeeeeffff00001111222233334444",
  );
  const lotIdHash = fromHex(
    "0x6666777788889999aaaabbbbccccddddeeeeffff000011112222333344445555",
  );
  const quantity = 250000n;
  const lotSalt = fromHex(
    "0x777788889999aaaabbbbccccddddeeeeffff0000111122223333444455556666",
  );

  const targetLeaf = await poseidonHash([
    participantIdHash,
    assetIdHash,
    lotIdHash,
    quantity,
    lotSalt,
  ]);

  const leafIndex = 5;
  const leaves = [];
  for (let index = 0; index < LEAF_COUNT; index += 1) {
    if (index === leafIndex) {
      leaves.push(targetLeaf);
    } else {
      leaves.push(
        await poseidonHash([
          participantIdHash,
          assetIdHash,
          BigInt(index + 1),
          0n,
          BigInt(index + 9000),
        ]),
      );
    }
  }

  const levels = await buildTree(leaves);
  const availabilityRoot = levels.at(-1)[0];
  const { pathElements, pathIndices } = merkleProof(levels, leafIndex);
  const lotNullifier = await poseidonHash([
    scopeHash,
    reasonHash,
    proofNonce,
    lotIdHash,
    lotSalt,
  ]);

  const input = {
    participantIdHash,
    assetIdHash,
    availabilityRoot,
    scopeHash,
    reasonHash,
    proofNonce,
    statementHashHi,
    statementHashLo,
    lotNullifier,
    lotIdHash,
    quantity,
    lotSalt,
    pathElements,
    pathIndices,
  };

  const expected = {
    publicSignals: [
      participantIdHash,
      assetIdHash,
      availabilityRoot,
      scopeHash,
      reasonHash,
      proofNonce,
      statementHashHi,
      statementHashLo,
      lotNullifier,
    ],
    internal: {
      statementHashHex,
      targetLeaf,
      leafIndex,
    },
  };

  return { input, expected };
}
