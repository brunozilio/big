import { type Prices, over, usd } from "sdk/providers/index.ts";

export const VERTEX: Prices = {
  "gemini-4-neon": usd(3, 15, 0.15, 0),
  "gemini-4-carbon": usd(2, 10, 0.1, 0),
  "gemini-4-argon": usd(2, 10, 0.1, 0),
  "gemini-4-hydrogen": usd(1.5, 7.5, 0.075, 0),
  "gemini-3.8-flash": usd(0.75, 3.75, 0.075, 0),
  "gemini-3.6-flash": usd(0.75, 3.75, 0.075, 0),
  "gemini-3.5-flash-lite": usd(0.3, 2.5, 0.03, 0),
  "gemini-3.1-pro-preview": usd(2, 12, 0.2, 0, over(200_000, 4, 18, 0.4, 0)),
};
