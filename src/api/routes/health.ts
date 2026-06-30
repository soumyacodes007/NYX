import { Router } from "express";

const router = Router();

router.get("/", (_request, response) => {
  response.json({
    status: "ok",
    service: "zk-dtcc-demo-api",
    generatedAt: new Date().toISOString(),
  });
});

export { router as healthRouter };
