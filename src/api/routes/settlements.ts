import { Router } from "express";

import { liveDemoData } from "../services/live-demo-data-instance.js";
import { asyncHandler } from "./async.js";
import { sendData } from "./utils.js";

const router = Router();

router.get("/", asyncHandler(async (_request, response) => {
  sendData(response, await liveDemoData.getSettlementSummaries(), "live-chain");
}));

router.post("/direct", (_request, response) => {
  sendData(
    response,
    {
      mode: "wallet-signed-live",
      status: "not-yet-wired",
      message: "Direct settlement execution will call the live Stellar settlement path from the frontend wallet flow.",
    },
    "frontend-wallet-action",
  );
});

router.post("/batch", (_request, response) => {
  sendData(
    response,
    {
      mode: "precomputed-proof-plus-live-verification",
      status: "not-yet-wired",
      message: "Batch settlement will use the precomputed batch proof and submit live transactions from the frontend flow.",
    },
    "frontend-wallet-action",
  );
});

export { router as settlementsRouter };
