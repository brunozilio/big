import { type Prices, over, usd } from "sdk/providers/index.ts";

export const XAI: Prices = {
  "grok-4.7": usd(2, 6, 0.5, 0, over(199_999, 4, 12, 1, 0)),
  "grok-4.6": usd(2, 6, 0.5, 0, over(199_999, 4, 12, 1, 0)),
  "grok-4.5": usd(2, 6, 0.3, 0, over(199_999, 4, 12, 0.6, 0)),
  "grok-4.3": usd(1.25, 2.5, 0.2, 0, over(199_999, 2.5, 5, 0.4, 0)),
  "grok-4.20-0309-reasoning": usd(1.25, 2.5, 0.2, 0, over(199_999, 2.5, 5, 0.4, 0)),
  "grok-4.20-0309-non-reasoning": usd(1.25, 2.5, 0.2, 0, over(199_999, 2.5, 5, 0.4, 0)),
  "grok-4.20-multi-agent-0309": usd(1.25, 2.5, 0.2, 0, over(199_999, 2.5, 5, 0.4, 0)),
};
