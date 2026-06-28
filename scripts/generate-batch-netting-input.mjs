import path from "node:path";
import { artifactDir, writeFixtureInput } from "./batch-netting-phase5-lib.mjs";

const { fullPath } = await writeFixtureInput("sample.input.json");
console.log(`Wrote Phase 5 sample input to ${path.resolve(artifactDir, path.basename(fullPath))}`);
