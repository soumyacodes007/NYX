import path from "node:path";
import {
  artifactDir,
  ensureGroth16Setup,
  generateWitness,
  prove,
  verify,
  writeFixtureInput,
} from "./batch-netting-phase5-lib.mjs";

console.log("Preparing Phase 5 circuit and Groth16 setup...");
await ensureGroth16Setup();

const { expected, fullPath } = await writeFixtureInput("phase5.runtime.input.json");
const witnessPath = path.join(artifactDir, "phase5.runtime.wtns");
generateWitness(fullPath, witnessPath);

const { proofPath, publicPath } = prove("phase5.runtime", witnessPath);
const verifyResult = verify(publicPath, proofPath);
process.stdout.write(verifyResult.stdout);
process.stderr.write(verifyResult.stderr);

console.log("\nExpected public signals:");
console.log(expected.publicSignals.map(String));
console.log("\nArtifacts:");
console.log(`- input:   ${fullPath}`);
console.log(`- witness: ${witnessPath}`);
console.log(`- proof:   ${proofPath}`);
console.log(`- public:  ${publicPath}`);
