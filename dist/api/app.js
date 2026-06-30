import cors from "cors";
import express from "express";
import { apiRouter } from "./routes/index.js";
export function createApp() {
    const app = express();
    app.use(cors());
    app.use(express.json());
    app.get("/", (_request, response) => {
        response.json({
            name: "zk-dtcc-demo-api",
            status: "ok",
            docs: [
                "/api/health",
                "/api/overview",
                "/api/participants",
                "/api/assets",
                "/api/proofs",
                "/api/orders",
                "/api/matches",
                "/api/settlements",
                "/api/compliance",
                "/api/audit/cases/:caseId",
            ],
        });
    });
    app.use("/api", apiRouter);
    app.use((error, _request, response, _next) => {
        const message = error instanceof Error ? error.message : "unexpected api error";
        response.status(500).json({
            error: message,
            generatedAt: new Date().toISOString(),
        });
    });
    return app;
}
