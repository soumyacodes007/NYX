import { Router } from "express";

import { liveDemoData } from "../services/live-demo-data-instance.js";
import { asyncHandler } from "./async.js";
import { sendData } from "./utils.js";

const router = Router();

router.get("/", asyncHandler(async (_request, response) => {
  sendData(response, await liveDemoData.getParticipants(), "live-chain");
}));

router.get("/:participantKey", asyncHandler(async (request, response) => {
  const participantKey = Array.isArray(request.params.participantKey)
    ? request.params.participantKey[0]
    : request.params.participantKey;
  const participant = await liveDemoData.getParticipant(participantKey);
  if (!participant) {
    response.status(404).json({ error: `unknown participant ${participantKey}` });
    return;
  }

  sendData(response, participant, "live-chain");
}));

export { router as participantsRouter };
