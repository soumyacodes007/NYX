import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import {
  buildUnencumberedLotFixture,
  stringifyDeep,
} from "./unencumbered-input-lib.mjs";

const { input, expected } = await buildUnencumberedLotFixture();

const outDir = path.join("circuits", "artifacts", "unencumbered_lot");
await mkdir(outDir, { recursive: true });
await writeFile(
  path.join(outDir, "input.json"),
  JSON.stringify(stringifyDeep(input), null, 2),
);
await writeFile(
  path.join(outDir, "expected.json"),
  JSON.stringify(stringifyDeep(expected), null, 2),
);

console.log(`Wrote ${path.join(outDir, "input.json")}`);
console.log(`Wrote ${path.join(outDir, "expected.json")}`);
