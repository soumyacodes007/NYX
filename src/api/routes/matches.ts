import { Router } from "express";

import { liveDemoData } from "../services/live-demo-data-instance.js";
import { asyncHandler } from "./async.js";
import { sendData } from "./utils.js";

const router = Router();

router.get("/", asyncHandler(async (_request, response) => {
  sendData(response, await liveDemoData.getOrderFlowSummary(), "live-chain");
}));

router.post("/:scenarioKey/execute", (request, response) => {
  sendData(
    response,
    {
      scenarioKey: request.params.scenarioKey,
      mode: "precomputed-proof-plus-live-verification",
      status: "not-yet-wired",
      message: "Private match execution will use a precomputed proof and a live matcher transaction flow.",
    },
    "frontend-wallet-action",
  );
});

export { router as matchesRouter };
