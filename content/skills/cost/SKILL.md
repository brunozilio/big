---
name: cost
description: What the work {{NAME}} ran cost in a period - per item and step, with conversations apart. Use when the user asks how much was spent, which item cost the most, or what a step costs.
---

Answer from the **Worklog** tool only.

1. Call Worklog with `action: "cost"`. The period is the last 7 days
   (`days: 7`) unless the user names another; pass `since` and `until` as
   ISO dates for a month or a sprint.
2. Asked about someone else? Pass `person`. When the tool says it is not
   allowed, tell the user that reading other people's cost needs the admin
   grant, or the team grant and a team in common, and stop.
3. Give the total first, then the items from the most expensive, each with
   its costliest steps, then conversations. Keep the dollar amounts as the
   tool gives them.
4. When the tool reports unpriced calls, say the total is a lower bound:
   those calls had no price.
5. For one item, call Worklog with `action: "item"` and its id to show its
   steps next to the cost.

Arguments: $ARGUMENTS
