import { Router } from "express";
import { liveDemoData } from "../services/live-demo-data-instance.js";
import { asyncHandler } from "./async.js";
import { sendData } from "./utils.js";
const router = Router();
router.get("/", asyncHandler(async (_request, response) => {
    sendData(response, await liveDemoData.getComplianceSnapshot(), "live-chain");
}));
router.post("/freeze", (_request, response) => {
    sendData(response, {
        mode: "wallet-signed-live",
        status: "not-yet-wired",
        message: "Freeze actions will be executed by the compliance operator wallet in the frontend.",
    }, "frontend-wallet-action");
});
router.post("/unfreeze", (_request, response) => {
    sendData(response, {
        mode: "wallet-signed-live",
        status: "not-yet-wired",
        message: "Unfreeze actions will be executed by the compliance operator wallet in the frontend.",
    }, "frontend-wallet-action");
});
router.post("/pause-global", (_request, response) => {
    sendData(response, {
        mode: "wallet-signed-live",
        status: "not-yet-wired",
        message: "Global pause actions are reserved for the compliance operator wallet flow.",
    }, "frontend-wallet-action");
});
router.post("/pause-asset", (_request, response) => {
    sendData(response, {
        mode: "wallet-signed-live",
        status: "not-yet-wired",
        message: "Asset pause actions are reserved for the compliance operator wallet flow.",
    }, "frontend-wallet-action");
});
router.post("/revoke-receipt", (_request, response) => {
    sendData(response, {
        mode: "wallet-signed-live",
        status: "not-yet-wired",
        message: "Receipt revocation will be executed by the compliance operator wallet flow.",
    }, "frontend-wallet-action");
});
export { router as complianceRouter };
