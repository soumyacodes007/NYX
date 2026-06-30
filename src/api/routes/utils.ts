import type { Response } from "express";

import type { ApiResponse } from "../types.js";

export function sendData<T>(response: Response, data: T, source: string): void {
  const payload: ApiResponse<T> = {
    data,
    meta: {
      generatedAt: new Date().toISOString(),
      source,
    },
  };

  response.json(payload);
}
