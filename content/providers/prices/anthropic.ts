import { type Prices, over, usd } from "sdk/providers/index.ts";

export const ANTHROPIC: Prices = {
  "claude-fable-5-2": usd(8, 40, 0.2, 10),
  "claude-mythos-5-2": usd(8, 40, 0.2, 10),
  "claude-fable-5-1": usd(10, 50, 0.25, 12.5),
  "claude-mythos-5-1": usd(10, 50, 0.25, 12.5),
  "claude-fable-5": usd(10, 50, 1, 12.5),
  "claude-mythos-5": usd(10, 50, 1, 12.5),
  "claude-opus-5-5": usd(4, 20, 0.2, 5),
  "claude-opus-5": usd(5, 25, 0.5, 6.25),
  "claude-opus-4-8": usd(5, 25, 0.5, 6.25),
  "claude-opus-4-7": usd(5, 25, 0.5, 6.25),
  "claude-opus-4-6": usd(5, 25, 0.5, 6.25),
  "claude-opus-4-5": usd(5, 25, 0.5, 6.25),
  "claude-opus-4-1": usd(15, 75, 1.5, 18.75),
  "claude-sonnet-5-5": usd(2, 10, 0.1, 2.5),
  "claude-sonnet-5": usd(2, 10, 0.2, 2.5),
  "claude-sonnet-4-6": usd(3, 15, 0.3, 3.75),
  "claude-sonnet-4-5": usd(3, 15, 0.3, 3.75),
  "claude-haiku-5-5": usd(0.1, 0.5, 0.01, 0.125, over(100_000, 0.5, 2.5, 0.05, 0.625)),
  "claude-haiku-4-5": usd(1, 5, 0.1, 1.25),
};
