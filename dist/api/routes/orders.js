import { Router } from "express";
import { liveDemoData } from "../services/live-demo-data-instance.js";
import { asyncHandler } from "./async.js";
import { sendData } from "./utils.js";
const router = Router();
router.get("/", asyncHandler(async (_request, response) => {
    sendData(response, await liveDemoData.getOrderFlowSummary(), "live-chain");
}));
router.post("/:participantKey", (request, response) => {
    sendData(response, {
        participantKey: request.params.participantKey,
        mode: "wallet-signed-live",
        status: "not-yet-wired",
        message: "Order commit and cancel flows will be executed by participant wallets in the frontend.",
    }, "frontend-wallet-action");
});
export { router as ordersRouter };
