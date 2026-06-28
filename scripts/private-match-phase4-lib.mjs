import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { buildPrivateMatchFixture } from "./private-match-input-lib.mjs";
import { stringifyDeep } from "./unencumbered-input-lib.mjs";

export const buildDir = path.join("circuits", "build");
export const artifactDir = path.join("circuits", "artifacts", "private_match");
export const circuitName = "private_match";

export function ensureLocalTool(binName) {
  if (binName === "circom") {
    const localCircom = path.join(".tools", "circom2", "bin", "circom");
    if (existsSync(localCircom)) {
      return localCircom;
    }
  }
  const binPath = path.join("node_modules", ".bin", binName);
  return existsSync(binPath) ? binPath : binName;
}

function normalizeOutput(output) {
  return output ? output.toString() : "";
}

export function runOrThrow(command, args, options = {}) {
  const rendered = [command, ...args].join(" ");
  console.log(`\n> ${rendered}`);
  const result = spawnSync(command, args, {
    encoding: "utf8",
    stdio: options.capture ? "pipe" : "inherit",
    ...options,
  });
  if (result.status !== 0) {
    const stdout = normalizeOutput(result.stdout);
    const stderr = normalizeOutput(result.stderr);
    if (options.capture) {
      process.stdout.write(stdout);
      process.stderr.write(stderr);
    }
    const error = new Error(`Command failed: ${rendered}`);
    error.stdout = stdout;
    error.stderr = stderr;
    error.status = result.status;
    throw error;
  }
  return result;
}

export function runCapture(command, args, options = {}) {
  const rendered = [command, ...args].join(" ");
  const result = spawnSync(command, args, {
    encoding: "utf8",
    stdio: "pipe",
    ...options,
  });
  return {
    rendered,
    status: result.status ?? 1,
    stdout: normalizeOutput(result.stdout),
    stderr: normalizeOutput(result.stderr),
  };
}

export async function ensureCircuitBuild() {
  await mkdir(buildDir, { recursive: true });
  await mkdir(artifactDir, { recursive: true });
  await rm(path.join(buildDir, `${circuitName}_js`), { recursive: true, force: true });

  runOrThrow(ensureLocalTool("circom"), [
    path.join("circuits", `${circuitName}.circom`),
    "--r1cs",
    "--wasm",
    "--sym",
    "-o",
    buildDir,
    "-l",
    "node_modules",
  ]);

  await writeFile(
    path.join(buildDir, `${circuitName}_js`, "package.json"),
    JSON.stringify({ type: "commonjs" }, null, 2),
  );
}

export async function ensureGroth16Setup() {
  await ensureCircuitBuild();
  const snarkjs = ensureLocalTool("snarkjs");

  runOrThrow(snarkjs, [
    "powersoftau",
    "new",
    "bn128",
    "12",
    path.join(artifactDir, "pot12_0000.ptau"),
  ]);
  runOrThrow(snarkjs, [
    "powersoftau",
    "contribute",
    path.join(artifactDir, "pot12_0000.ptau"),
    path.join(artifactDir, "pot12_0001.ptau"),
    "--name=phase4-initial",
    "-e=zkdtcc-phase4-test-entropy",
  ]);
  runOrThrow(snarkjs, [
    "powersoftau",
    "prepare",
    "phase2",
    path.join(artifactDir, "pot12_0001.ptau"),
    path.join(artifactDir, "pot12_final.ptau"),
  ]);
  runOrThrow(snarkjs, [
    "groth16",
    "setup",
    path.join(buildDir, `${circuitName}.r1cs`),
    path.join(artifactDir, "pot12_final.ptau"),
    path.join(artifactDir, `${circuitName}_0000.zkey`),
  ]);
  runOrThrow(snarkjs, [
    "zkey",
    "contribute",
    path.join(artifactDir, `${circuitName}_0000.zkey`),
    path.join(artifactDir, `${circuitName}_final.zkey`),
    "--name=phase4-zkey",
    "-e=zkdtcc-phase4-zkey-entropy",
  ]);
  runOrThrow(snarkjs, [
    "zkey",
    "export",
    "verificationkey",
    path.join(artifactDir, `${circuitName}_final.zkey`),
    path.join(artifactDir, "verification_key.json"),
  ]);
}

function resolveFixtureOptions(optionsOrTransform) {
  if (typeof optionsOrTransform === "function") {
    return { transform: optionsOrTransform };
  }
  return optionsOrTransform ?? {};
}

export async function writeFixtureInput(filename, optionsOrTransform) {
  const { transform, ...fixtureOptions } = resolveFixtureOptions(optionsOrTransform);
  const fixture = await buildPrivateMatchFixture(fixtureOptions);
  const input = transform ? transform(structuredClone(fixture.input)) : fixture.input;
  const fullPath = path.join(artifactDir, filename);
  await writeFile(fullPath, JSON.stringify(stringifyDeep(input), null, 2));
  return { input, expected: fixture.expected, fullPath };
}

export function witnessCommandArgs(inputPath, witnessPath) {
  return [
    path.join(buildDir, `${circuitName}_js`, "generate_witness.js"),
    path.join(buildDir, `${circuitName}_js`, `${circuitName}.wasm`),
    inputPath,
    witnessPath,
  ];
}

export function generateWitness(inputPath, witnessPath) {
  return runOrThrow("node", witnessCommandArgs(inputPath, witnessPath));
}

export function tryGenerateWitness(inputPath, witnessPath) {
  return runCapture("node", witnessCommandArgs(inputPath, witnessPath));
}

export function prove(name, witnessPath) {
  const snarkjs = ensureLocalTool("snarkjs");
  const proofPath = path.join(artifactDir, `${name}.proof.json`);
  const publicPath = path.join(artifactDir, `${name}.public.json`);
  runOrThrow(snarkjs, [
    "groth16",
    "prove",
    path.join(artifactDir, `${circuitName}_final.zkey`),
    witnessPath,
    proofPath,
    publicPath,
  ]);
  return { proofPath, publicPath };
}

export function verify(publicPath, proofPath) {
  const snarkjs = ensureLocalTool("snarkjs");
  return runCapture(snarkjs, [
    "groth16",
    "verify",
    path.join(artifactDir, "verification_key.json"),
    publicPath,
    proofPath,
  ]);
}

export async function readJson(filePath) {
  return JSON.parse(await readFile(filePath, "utf8"));
}
