import path from "node:path";
import { artifactDir, writeFixtureInput } from "./entitlement-claim-phase6-lib.mjs";

const { fullPath } = await writeFixtureInput("sample.input.json");
console.log(`Wrote Phase 6 sample input to ${path.resolve(artifactDir, path.basename(fullPath))}`);
