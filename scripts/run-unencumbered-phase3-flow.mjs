import path from "node:path";
import {
  artifactDir,
  ensureGroth16Setup,
  generateWitness,
  prove,
  readJson,
  verify,
  writeFixtureInput,
} from "./unencumbered-phase3-lib.mjs";

await ensureGroth16Setup();
const { expected, fullPath } = await writeFixtureInput("input.json");
generateWitness(fullPath, path.join(artifactDir, "witness.wtns"));
const { proofPath, publicPath } = prove("proof", path.join(artifactDir, "witness.wtns"));
const verifyResult = verify(publicPath, proofPath);
process.stdout.write(verifyResult.stdout);
process.stderr.write(verifyResult.stderr);
if (verifyResult.status !== 0 || !verifyResult.stdout.includes("OK!")) {
  throw new Error("Groth16 verification did not succeed");
}

const publicSignals = await readJson(publicPath);

if (JSON.stringify(publicSignals) !== JSON.stringify(expected.publicSignals)) {
  throw new Error("Public signals did not match expected deterministic values");
}

console.log("\nPhase 3 Groth16 flow completed and public signals matched expected values.");
