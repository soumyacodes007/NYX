import path from "node:path";
import {
  artifactDir,
  ensureGroth16Setup,
  generateWitness,
  prove,
  readJson,
  writeFixtureInput,
} from "./unencumbered-phase3-lib.mjs";
import { stringifyDeep } from "./unencumbered-input-lib.mjs";

const statementHashHex = process.argv[2];
const name = process.argv[3] ?? "runtime";
const participantIdHashHex = process.argv[4];

if (!statementHashHex) {
  throw new Error("usage: node scripts/generate-unencumbered-proof.mjs <statement-hash-hex> [name]");
}

await ensureGroth16Setup();
const { expected, fullPath } = await writeFixtureInput(`${name}.input.json`, {
  statementHashHex,
  participantIdHashHex,
});
const witnessPath = path.join(artifactDir, `${name}.wtns`);
generateWitness(fullPath, witnessPath);
const { proofPath, publicPath } = prove(name, witnessPath);
const verificationKeyPath = path.join(artifactDir, "verification_key.json");
const proof = await readJson(proofPath);
const publicSignals = await readJson(publicPath);
const verificationKey = await readJson(verificationKeyPath);

function toHex32(value) {
  return BigInt(value).toString(16).padStart(64, "0");
}

function toHex4(value) {
  return Number(value).toString(16).padStart(8, "0");
}

function encodeG1(point) {
  return `${toHex32(point[0])}${toHex32(point[1])}`;
}

function encodeFp2(coords) {
  return `${toHex32(coords[1])}${toHex32(coords[0])}`;
}

function encodeG2(point) {
  return `${encodeFp2(point[0])}${encodeFp2(point[1])}`;
}

function encodeVerificationKey(vk) {
  return [
    encodeG1(vk.vk_alpha_1),
    encodeG2(vk.vk_beta_2),
    encodeG2(vk.vk_gamma_2),
    encodeG2(vk.vk_delta_2),
    toHex4(vk.IC.length),
    ...vk.IC.map(encodeG1),
  ].join("");
}

function encodeProofPayload(bundleProof, bundlePublicSignals) {
  return [
    encodeG1(bundleProof.pi_a),
    encodeG2(bundleProof.pi_b),
    encodeG1(bundleProof.pi_c),
    toHex4(bundlePublicSignals.length),
    ...bundlePublicSignals.map(toHex32),
  ].join("");
}

function toPrefixedHex(value) {
  return `0x${value}`;
}

const bundle = {
  statementHashHex,
  participantIdHashHex: toPrefixedHex(toHex32(expected.publicSignals[0])),
  availabilityRootHex: toPrefixedHex(toHex32(expected.publicSignals[2])),
  proofNonceHex: toPrefixedHex(toHex32(expected.publicSignals[5])),
  lotNullifierHex: toPrefixedHex(toHex32(expected.publicSignals[8])),
  verificationKeyHex: toPrefixedHex(encodeVerificationKey(verificationKey)),
  proofPayloadHex: toPrefixedHex(encodeProofPayload(proof, publicSignals)),
  publicSignalsHex: publicSignals.map((signal) => toPrefixedHex(toHex32(signal))),
};

process.stdout.write(
  `__PHASE3_BUNDLE__${JSON.stringify(
    stringifyDeep({
      proofPath,
      publicPath,
      verificationKeyPath,
      bundle,
      expected,
    }),
  )}\n`,
);
