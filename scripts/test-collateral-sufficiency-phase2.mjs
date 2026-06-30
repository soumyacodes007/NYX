import path from "node:path";
import {
  artifactDir,
  ensureGroth16Setup,
  generateWitness,
  prove,
  readJson,
  tryGenerateWitness,
  verify,
  writeFixtureInput,
} from "./collateral-sufficiency-phase2-lib.mjs";

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

console.log("Preparing circuit build and Groth16 setup once for all Phase 2 tests...");
await ensureGroth16Setup();

console.log("\nTest 1: valid witness, proof, and verification");
{
  const { expected, fullPath } = await writeFixtureInput("test1.valid.input.json");
  const witnessPath = path.join(artifactDir, "test1.valid.wtns");
  generateWitness(fullPath, witnessPath);
  const { proofPath, publicPath } = prove("test1.valid", witnessPath);
  const verifyResult = verify(publicPath, proofPath);
  process.stdout.write(verifyResult.stdout);
  process.stderr.write(verifyResult.stderr);
  assert(verifyResult.status === 0, "Test 1 verification process failed");
  assert(verifyResult.stdout.includes("OK!"), "Test 1 proof did not verify");
  const publicSignals = await readJson(publicPath);
  assert(
    JSON.stringify(publicSignals) === JSON.stringify(expected.publicSignals.map(String)),
    "Test 1 public signals did not match expected values",
  );
}

console.log("\nTest 2: insufficient margin should fail witness generation");
{
  const { fullPath } = await writeFixtureInput("test2.bad-margin.input.json", (input) => {
    input.requiredMargin = (
      BigInt(input.balance[0]) * BigInt(input.price[0]) * BigInt(input.haircutBps[0]) +
      BigInt(input.balance[1]) * BigInt(input.price[1]) * BigInt(input.haircutBps[1]) +
      BigInt(input.balance[2]) * BigInt(input.price[2]) * BigInt(input.haircutBps[2]) +
      1n
    ).toString();
    return input;
  });
  const witnessPath = path.join(artifactDir, "test2.bad-margin.wtns");
  const result = tryGenerateWitness(fullPath, witnessPath);
  process.stdout.write(result.stdout);
  process.stderr.write(result.stderr);
  assert(result.status !== 0, "Test 2 witness generation unexpectedly succeeded");
}

console.log("\nTest 3: tampered public signals should fail verification");
{
  const { fullPath } = await writeFixtureInput("test3.valid.input.json");
  const witnessPath = path.join(artifactDir, "test3.valid.wtns");
  generateWitness(fullPath, witnessPath);
  const { proofPath, publicPath } = prove("test3.valid", witnessPath);
  const tamperedPublicPath = path.join(artifactDir, "test3.tampered.public.json");
  const publicSignals = await readJson(publicPath);
  publicSignals[1] = (BigInt(publicSignals[1]) + 1n).toString();
  await import("node:fs/promises").then(({ writeFile }) =>
    writeFile(tamperedPublicPath, JSON.stringify(publicSignals, null, 2)),
  );
  const verifyResult = verify(tamperedPublicPath, proofPath);
  process.stdout.write(verifyResult.stdout);
  process.stderr.write(verifyResult.stderr);
  assert(
    verifyResult.status !== 0 || verifyResult.stdout.includes("Invalid proof"),
    "Test 3 tampered public signals unexpectedly verified",
  );
}

console.log("\nAll three Phase 2 real tests passed.");
