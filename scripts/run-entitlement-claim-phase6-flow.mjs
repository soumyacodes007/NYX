import path from "node:path";
import {
  artifactDir,
  ensureGroth16Setup,
  generateWitness,
  prove,
  verify,
  writeFixtureInput,
} from "./entitlement-claim-phase6-lib.mjs";

console.log("Preparing Phase 6 circuit and Groth16 setup...");
await ensureGroth16Setup();

const { expected, fullPath } = await writeFixtureInput("phase6.runtime.input.json");
const witnessPath = path.join(artifactDir, "phase6.runtime.wtns");
generateWitness(fullPath, witnessPath);

const { proofPath, publicPath } = prove("phase6.runtime", witnessPath);
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
