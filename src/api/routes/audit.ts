import { Router } from "express";

import { liveDemoData } from "../services/live-demo-data-instance.js";
import { asyncHandler } from "./async.js";
import { sendData } from "./utils.js";

const router = Router();

router.get("/cases/:caseId", asyncHandler(async (request, response) => {
  const caseId = Array.isArray(request.params.caseId) ? request.params.caseId[0] : request.params.caseId;
  const auditCase = await liveDemoData.getAuditCase(caseId);
  if (!auditCase) {
    response.status(404).json({ error: `unknown audit case ${caseId}` });
    return;
  }
  sendData(response, auditCase, "live-chain");
}));

router.post("/grants", (_request, response) => {
  sendData(
    response,
    {
      mode: "wallet-signed-live",
      status: "not-yet-wired",
      message: "Disclosure grants will be executed by the audit/compliance role from the frontend flow.",
    },
    "frontend-wallet-action",
  );
});

export { router as auditRouter };
