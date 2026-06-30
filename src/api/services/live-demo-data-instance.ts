import { LiveDemoDataService, prewarmLiveDemoData } from "./live-demo-data-service.js";

export const liveDemoData = new LiveDemoDataService();
export const prewarmDemoData = () => prewarmLiveDemoData(liveDemoData);
