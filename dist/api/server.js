import { createApp } from "./app.js";
import { prewarmDemoData } from "./services/live-demo-data-instance.js";
const port = Number(process.env.PORT ?? 4000);
const app = createApp();
app.listen(port, () => {
    console.log(`zk-dtcc demo api listening on http://localhost:${port}`);
    void prewarmDemoData()
        .then(() => {
        console.log("zk-dtcc demo api cache prewarm complete");
    })
        .catch((error) => {
        console.error("zk-dtcc demo api cache prewarm failed", error);
    });
});
