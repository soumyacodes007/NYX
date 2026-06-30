import { Router } from "express";

import { liveDemoData } from "../services/live-demo-data-instance.js";
import { asyncHandler } from "./async.js";
import { sendData } from "./utils.js";

const router = Router();

router.get("/", asyncHandler(async (_request, response) => {
  sendData(response, await liveDemoData.getAssets(), "live-chain-plus-demo-catalog");
}));

export { router as assetsRouter };
