import { Router } from "express";

import { liveDemoData } from "../services/live-demo-data-instance.js";
import { asyncHandler } from "./async.js";
import { sendData } from "./utils.js";

const router = Router();

router.get("/", asyncHandler(async (_request, response) => {
  sendData(response, await liveDemoData.getProofScenarios(), "live-chain");
}));

router.post("/:scenarioKey/prepare", asyncHandler(async (request, response) => {
  const scenario = (await liveDemoData.getProofScenarios()).find((item) => item.key === request.params.scenarioKey);
  if (!scenario) {
    response.status(404).json({ error: `unknown proof scenario ${request.params.scenarioKey}` });
    return;
  }

  sendData(
    response,
    {
      scenario,
      mode: "precomputed-proof-artifact",
      nextStep: "submit the matching proof receipt through the wallet-connected frontend flow",
    },
    "proof-artifact-service",
  );
}));

export { router as proofsRouter };
